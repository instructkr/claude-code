//! Per-provider integration tests against a mocked image backend.
//!
//! These tests stand up a tiny `TcpListener`-backed HTTP server, point the
//! `generate_image` / `inpaint_region` / validator tools at it via the
//! provider adapter registry, and verify that:
//!   1. the request hits the correct adapter-specific path (`/prompt` for
//!      `ComfyUI`, `/v1/text-to-image` for `Diffusers`, `/jobs/generate` for
//!      the internal worker, etc.),
//!   2. the request body is the canonical → provider-specific translation
//!      we expect,
//!   3. the response is normalised back into the harness schema, and
//!   4. validators raise/clear violations as the policy gate expects.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde_json::{json, Value};
use tools::execute_tool;

#[derive(Debug, Clone)]
struct RecordedRequest {
    method: String,
    path: String,
    body: String,
}

struct MockBackend {
    addr: SocketAddr,
    received: Arc<Mutex<Vec<RecordedRequest>>>,
    shutdown: Option<std::sync::mpsc::Sender<()>>,
    handle: Option<thread::JoinHandle<()>>,
}

type Responder = Arc<dyn Fn(&RecordedRequest) -> (u16, Value) + Send + Sync + 'static>;

impl MockBackend {
    fn spawn(responder: Responder) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock backend");
        listener
            .set_nonblocking(true)
            .expect("nonblocking listener");
        let addr = listener.local_addr().expect("local addr");
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let received = Arc::new(Mutex::new(Vec::<RecordedRequest>::new()));
        let received_clone = received.clone();

        let handle = thread::spawn(move || {
            loop {
                if rx.try_recv().is_ok() {
                    break;
                }
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        // Read request fully (header + body up to 64 KiB).
                        let mut buffer = vec![0_u8; 65_536];
                        let mut size = 0;
                        // Drain available bytes; for this short-lived loopback
                        // request a single read is sufficient because the
                        // reqwest client writes the entire request before
                        // reading the response.
                        match stream.read(&mut buffer[size..]) {
                            Ok(n) => size += n,
                            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                                thread::sleep(Duration::from_millis(5));
                            }
                            Err(error) => panic!("mock read failed: {error}"),
                        }
                        let raw = String::from_utf8_lossy(&buffer[..size]).into_owned();
                        let mut lines = raw.split("\r\n");
                        let request_line = lines.next().unwrap_or_default().to_string();
                        let mut parts = request_line.splitn(3, ' ');
                        let method = parts.next().unwrap_or("").to_string();
                        let path = parts.next().unwrap_or("").to_string();
                        let body = raw
                            .split_once("\r\n\r\n")
                            .map_or_else(String::new, |(_, body)| body.to_string());
                        let recorded = RecordedRequest { method, path, body };
                        received_clone.lock().unwrap().push(recorded.clone());

                        let (status, payload) = responder(&recorded);
                        let body = serde_json::to_string(&payload).expect("serialise");
                        let response = format!(
                            "HTTP/1.1 {} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            status,
                            body.len(),
                            body
                        );
                        stream.write_all(response.as_bytes()).expect("write");
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("mock accept failed: {error}"),
                }
            }
        });

        Self {
            addr,
            received,
            shutdown: Some(tx),
            handle: Some(handle),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.addr, path)
    }

    fn base(&self) -> String {
        format!("http://{}", self.addr)
    }

    fn requests(&self) -> Vec<RecordedRequest> {
        self.received.lock().unwrap().clone()
    }
}

impl Drop for MockBackend {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn parse_body(body: &str) -> Value {
    serde_json::from_str(body).unwrap_or_else(|err| panic!("body not JSON: {err} — raw={body}"))
}

fn pretty_value(raw: &str) -> Value {
    serde_json::from_str(raw).expect("tool output is valid JSON")
}

#[test]
fn comfyui_provider_translates_generate_request_and_normalises_response() {
    let server = MockBackend::spawn(Arc::new(|req| {
        assert_eq!(req.method, "POST");
        assert_eq!(req.path, "/prompt");
        let parsed = parse_body(&req.body);
        let workflow = parsed["prompt"].as_object().expect("workflow obj");
        assert_eq!(workflow["3"]["class_type"], "KSampler");
        assert_eq!(workflow["3"]["inputs"]["seed"], 17);
        assert_eq!(workflow["4"]["inputs"]["ckpt_name"], "sdxl-base-1.0");
        (
            200,
            json!({
                "prompt_id": "abc-123",
                "outputs": {
                    "9": {"images": [{"filename": "iqh_001.png", "subfolder": "iqh", "type": "output"}]}
                }
            }),
        )
    }));

    let result = execute_tool(
        "generate_image",
        &json!({
            "backend_url": server.base(),
            "provider": "comfyui",
            "prompt": "knight in mirrored armor",
            "negative_prompt": "extra fingers",
            "width": 1024,
            "height": 1024,
            "steps": 30,
            "cfg": 6.0,
            "seed": 17,
            "sampler": "dpmpp_2m_karras",
            "model": "sdxl-base-1.0"
        }),
    )
    .expect("comfyui generate");
    let parsed = pretty_value(&result);
    assert_eq!(parsed["provider"], "comfyui");
    assert_eq!(parsed["endpoint"], server.url("/prompt"));
    let images = parsed["images"].as_array().expect("images");
    assert_eq!(images.len(), 1);
    assert_eq!(images[0]["uri"], "comfyui://output/iqh/iqh_001.png");
    assert_eq!(server.requests().len(), 1);
}

#[test]
fn diffusers_provider_translates_generate_request_and_normalises_artifacts() {
    let server = MockBackend::spawn(Arc::new(|req| {
        assert_eq!(req.method, "POST");
        assert_eq!(req.path, "/v1/text-to-image");
        let body = parse_body(&req.body);
        assert_eq!(body["num_inference_steps"], 28);
        assert_eq!(body["guidance_scale"], 6.5);
        assert_eq!(body["scheduler"], "dpmpp_2m_karras");
        assert_eq!(body["seed"], 99);
        (
            200,
            json!({
                "artifacts": [
                    {"uri": "s3://run/0.png", "seed": 99},
                    {"uri": "s3://run/1.png", "seed": 100}
                ],
                "metadata": {"sampler": "dpmpp_2m_karras"}
            }),
        )
    }));

    let result = execute_tool(
        "generate_image",
        &json!({
            "backend_url": server.base(),
            "provider": "diffusers",
            "prompt": "armored knight",
            "negative_prompt": "extra digits",
            "width": 1024,
            "height": 1024,
            "steps": 28,
            "cfg": 6.5,
            "seed": 99,
            "sampler": "dpmpp_2m_karras",
            "model": "sdxl-base-1.0",
            "batch_size": 2
        }),
    )
    .expect("diffusers generate");
    let parsed = pretty_value(&result);
    assert_eq!(parsed["provider"], "diffusers");
    assert_eq!(parsed["endpoint"], server.url("/v1/text-to-image"));
    let images = parsed["images"].as_array().expect("images");
    assert_eq!(images.len(), 2);
    assert_eq!(images[0]["seed"], 99);
    assert_eq!(images[1]["uri"], "s3://run/1.png");
}

#[test]
fn internal_worker_provider_routes_inpaint_through_jobs_endpoint() {
    let server = MockBackend::spawn(Arc::new(|req| {
        assert_eq!(req.method, "POST");
        assert_eq!(req.path, "/jobs/inpaint");
        let body = parse_body(&req.body);
        assert_eq!(body["task"], "inpaint_region");
        assert_eq!(body["params"]["denoise_strength"], 0.32);
        assert_eq!(body["params"]["mask_uri"], "img://mask.png");
        (
            200,
            json!({
                "job_id": "job-7",
                "queue": "highprio",
                "duration_ms": 1450,
                "result": {
                    "images": [{"uri": "wkr://job-7/0.png", "seed": 5}]
                }
            }),
        )
    }));

    let result = execute_tool(
        "inpaint_region",
        &json!({
            "backend_url": server.base(),
            "provider": "internal_worker",
            "image_uri": "img://1.png",
            "mask_uri": "img://mask.png",
            "prompt": "fix hand",
            "negative_prompt": "fused",
            "denoise_strength": 0.32,
            "steps": 24,
            "cfg": 6.0,
            "seed": 5,
            "model": "sdxl-inpaint",
            "preserve_edges": true
        }),
    )
    .expect("internal worker inpaint");
    let parsed = pretty_value(&result);
    assert_eq!(parsed["provider"], "internal_worker");
    assert_eq!(parsed["endpoint"], server.url("/jobs/inpaint"));
    assert_eq!(parsed["images"][0]["uri"], "wkr://job-7/0.png");
    assert_eq!(parsed["metadata"]["job_id"], "job-7");
    assert_eq!(parsed["metadata"]["queue"], "highprio");
}

#[test]
fn diffusers_provider_inpaint_uses_v1_inpaint_path() {
    let server = MockBackend::spawn(Arc::new(|req| {
        assert_eq!(req.path, "/v1/inpaint");
        let body = parse_body(&req.body);
        assert_eq!(body["model"], "sdxl-inpaint");
        assert_eq!(body["mask_image"], "img://mask.png");
        assert_eq!(body["strength"], 0.4);
        (
            200,
            json!({
                "artifacts": [{"uri": "s3://patched.png", "seed": 21}]
            }),
        )
    }));
    let result = execute_tool(
        "inpaint_region",
        &json!({
            "backend_url": server.base(),
            "provider": "diffusers",
            "image_uri": "img://1.png",
            "mask_uri": "img://mask.png",
            "prompt": "fix hand",
            "negative_prompt": "fused",
            "denoise_strength": 0.4,
            "steps": 30,
            "cfg": 6.0,
            "seed": 21,
            "model": "sdxl-inpaint"
        }),
    )
    .expect("diffusers inpaint");
    let parsed = pretty_value(&result);
    assert_eq!(parsed["provider"], "diffusers");
    assert_eq!(parsed["images"][0]["uri"], "s3://patched.png");
}

#[test]
fn unknown_provider_id_short_circuits_with_actionable_error() {
    let result = execute_tool(
        "generate_image",
        &json!({
            "backend_url": "http://127.0.0.1:1",
            "provider": "stable-diffusion-xyz",
            "prompt": "x",
            "negative_prompt": "y",
            "width": 512,
            "height": 512,
            "steps": 8,
            "cfg": 5.0,
            "seed": 1,
            "sampler": "euler",
            "model": "sdxl"
        }),
    );
    let err = result.expect_err("unknown provider should fail");
    assert!(err.contains("unknown image provider"));
    assert!(err.contains("comfyui"));
}

#[test]
fn legacy_generate_image_without_provider_forwards_payload_verbatim() {
    let server = MockBackend::spawn(Arc::new(|req| {
        assert_eq!(req.method, "POST");
        assert_eq!(req.path, "/legacy");
        let body = parse_body(&req.body);
        // Should be the canonical schema as-is — no provider translation.
        assert_eq!(body["prompt"], "verbatim");
        assert_eq!(body["seed"], 7);
        (200, json!({"echo": body, "status": "ok"}))
    }));
    let result = execute_tool(
        "generate_image",
        &json!({
            "backend_url": server.url("/legacy"),
            "prompt": "verbatim",
            "negative_prompt": "n",
            "width": 512,
            "height": 512,
            "steps": 8,
            "cfg": 5.0,
            "seed": 7,
            "sampler": "euler",
            "model": "sdxl"
        }),
    )
    .expect("legacy passthrough");
    let parsed = pretty_value(&result);
    assert_eq!(parsed["tool"], "generate_image");
    assert_eq!(parsed["backend_url"], server.url("/legacy"));
    assert_eq!(parsed["result"]["status"], "ok");
}

#[test]
fn detect_hands_feet_forwards_payload_to_validator_endpoint() {
    let server = MockBackend::spawn(Arc::new(|req| {
        assert_eq!(req.path, "/v1/anatomy");
        let body = parse_body(&req.body);
        assert_eq!(body["image_uri"], "img://x");
        assert_eq!(body["min_confidence"], 0.3);
        (
            200,
            json!({
                "regions": [
                    {"kind": "left_hand", "bbox": [0,0,1,1], "confidence": 0.9, "issues": ["extra_digits"]}
                ],
                "scores": {"anatomy_score": 0.81}
            }),
        )
    }));
    let result = execute_tool(
        "detect_hands_feet",
        &json!({
            "backend_url": server.url("/v1/anatomy"),
            "image_uri": "img://x",
            "min_confidence": 0.3
        }),
    )
    .expect("detect hands_feet");
    let parsed = pretty_value(&result);
    assert_eq!(parsed["result"]["scores"]["anatomy_score"], 0.81);
    assert_eq!(server.requests().len(), 1);
}

#[test]
fn check_symmetry_forwards_expectations_and_normalises_violations() {
    let server = MockBackend::spawn(Arc::new(|req| {
        assert_eq!(req.path, "/v1/symmetry");
        let body = parse_body(&req.body);
        let expectations = body["expectations"].as_array().expect("expectations array");
        assert_eq!(expectations.len(), 1);
        assert_eq!(expectations[0]["label"], "pauldrons");
        (
            200,
            json!({
                "symmetry_score": 0.88,
                "violations": [{
                    "label": "pauldrons",
                    "delta": 0.21,
                    "left_bbox": [0,0,1,1],
                    "right_bbox": [1,0,2,1]
                }]
            }),
        )
    }));
    let result = execute_tool(
        "check_symmetry",
        &json!({
            "backend_url": server.url("/v1/symmetry"),
            "image_uri": "img://x",
            "expectations": [{
                "label": "pauldrons",
                "left_region": [0.0, 0.0, 1.0, 1.0],
                "right_region": [1.0, 0.0, 2.0, 1.0]
            }]
        }),
    )
    .expect("check symmetry");
    let parsed = pretty_value(&result);
    assert_eq!(parsed["result"]["symmetry_score"], 0.88);
    assert_eq!(parsed["result"]["violations"][0]["label"], "pauldrons");
}

#[test]
fn check_pattern_consistency_round_trips_violations() {
    let server = MockBackend::spawn(Arc::new(|req| {
        assert_eq!(req.path, "/v1/pattern");
        let body = parse_body(&req.body);
        let regions = body["pattern_regions"].as_array().expect("regions");
        assert_eq!(regions.len(), 1);
        (
            200,
            json!({
                "pattern_score": 0.79,
                "violations": [{
                    "label": "chest",
                    "error_type": "scale_drift",
                    "bbox": [0,0,1,1],
                    "severity": 0.45
                }]
            }),
        )
    }));
    let result = execute_tool(
        "check_pattern_consistency",
        &json!({
            "backend_url": server.url("/v1/pattern"),
            "image_uri": "img://x",
            "pattern_regions": [{"label": "chest", "bbox": [0.0, 0.0, 1.0, 1.0]}]
        }),
    )
    .expect("pattern check");
    let parsed = pretty_value(&result);
    assert_eq!(parsed["result"]["pattern_score"], 0.79);
    assert_eq!(
        parsed["result"]["violations"][0]["error_type"],
        "scale_drift"
    );
}

#[test]
fn image_validator_run_aggregates_full_score_vector_from_mocked_endpoints() {
    // Use a single backend with path-based dispatch so each axis returns
    // its own response.
    let dispatch: BTreeMap<&'static str, Value> = [
        (
            "/v1/anatomy",
            json!({
                "regions": [],
                "scores": {"anatomy_score": 0.94}
            }),
        ),
        (
            "/v1/symmetry",
            json!({
                "symmetry_score": 0.91,
                "violations": []
            }),
        ),
        (
            "/v1/pattern",
            json!({"pattern_score": 0.93, "violations": []}),
        ),
        (
            "/v1/artifact",
            json!({"artifact_score": 0.88, "issues": []}),
        ),
        ("/v1/creative", json!({"creative_score": 0.81})),
    ]
    .into_iter()
    .collect();
    let dispatch = Arc::new(dispatch);
    let dispatch_clone = dispatch.clone();
    let server = MockBackend::spawn(Arc::new(move |req| {
        let value = dispatch_clone
            .get(req.path.as_str())
            .cloned()
            .unwrap_or_else(|| json!({"error": format!("unknown path {}", req.path)}));
        (200, value)
    }));

    let result = execute_tool(
        "ImageValidatorRun",
        &json!({
            "image_uri": "img://x",
            "endpoints": {
                "hands_feet": server.url("/v1/anatomy"),
                "symmetry": server.url("/v1/symmetry"),
                "pattern": server.url("/v1/pattern"),
                "artifact": server.url("/v1/artifact"),
                "creative": server.url("/v1/creative")
            },
            "symmetry_expectations": [{
                "label": "pauldrons",
                "left_region": [0.0, 0.0, 1.0, 1.0],
                "right_region": [1.0, 0.0, 2.0, 1.0]
            }],
            "pattern_regions": [{"label": "chest", "bbox": [0.0, 0.0, 1.0, 1.0]}],
            "creative_intent": "knight in mirrored armor"
        }),
    )
    .expect("validator run");
    let parsed = pretty_value(&result);
    assert_eq!(parsed["scores"]["anatomy_score"], 0.94);
    assert_eq!(parsed["scores"]["symmetry_score"], 0.91);
    assert_eq!(parsed["scores"]["pattern_score"], 0.93);
    assert_eq!(parsed["scores"]["artifact_score"], 0.88);
    assert_eq!(parsed["scores"]["creative_score"], 0.81);
    let weighted = parsed["scores"]["weighted_total"]
        .as_f64()
        .expect("weighted");
    let expected = 0.4 * 0.94 + 0.25 * 0.91 + 0.2 * 0.93 + 0.1 * 0.88 + 0.05 * 0.81;
    assert!((weighted - expected).abs() < 1e-6);
}

#[test]
fn image_profile_select_resolves_known_profile_with_warnings() {
    let result = execute_tool(
        "ImageProfileSelect",
        &json!({
            "profile": "production",
            "requested_steps": 200,
            "requested_cfg": 0.5
        }),
    )
    .expect("profile select");
    let parsed = pretty_value(&result);
    assert_eq!(parsed["profile"], "production");
    assert_eq!(parsed["params"]["steps"], 40); // clamped to profile max
    assert_eq!(parsed["params"]["cfg"], 5.0); // clamped to profile min
    let warnings = parsed["warnings"].as_array().expect("warnings");
    assert_eq!(warnings.len(), 2);
    assert!(parsed["thresholds"]["weighted_total"].as_f64().unwrap() >= 0.90);
}

#[test]
fn image_provider_list_returns_three_built_ins() {
    let result = execute_tool("ImageProviderList", &json!({})).expect("list providers");
    let parsed = pretty_value(&result);
    assert_eq!(parsed["count"], 3);
    let providers = parsed["providers"].as_array().expect("providers array");
    let ids: Vec<&str> = providers
        .iter()
        .map(|p| p["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"comfyui"));
    assert!(ids.contains(&"diffusers"));
    assert!(ids.contains(&"internal_worker"));
}

#[test]
fn image_regression_run_executes_diffusers_fixture_and_emits_markdown_summary() {
    let dispatch = Arc::new(Mutex::new(true));
    let dispatch_clone = dispatch.clone();
    let server = MockBackend::spawn(Arc::new(move |req| match req.path.as_str() {
        "/v1/text-to-image" => (
            200,
            json!({"artifacts": [{"uri": "s3://run/0.png", "seed": 1}]}),
        ),
        "/v1/anatomy" => (
            200,
            json!({"regions": [], "scores": {"anatomy_score": 0.95}}),
        ),
        "/v1/artifact" => (200, json!({"artifact_score": 0.92, "issues": []})),
        other => {
            *dispatch_clone.lock().unwrap() = false;
            (404, json!({"error": format!("unexpected path {other}")}))
        }
    }));

    let result = execute_tool(
        "ImageRegressionRun",
        &json!({
            "run_id": "iqh_integration",
            "profile": "production",
            "fixtures": [{
                "id": "scene1",
                "prompt": "armored knight",
                "negative_prompt": "extra digits",
                "width": 1024,
                "height": 1024,
                "model": "sdxl-base-1.0",
                "seeds": [1],
                "expectations": {
                    "requires_hands": true,
                    "requires_feet": false,
                    "symmetry_labels": [],
                    "pattern_labels": []
                },
                "provider": "diffusers",
                "backend_url": server.base()
            }],
            "validator_endpoints": {
                "hands_feet": server.url("/v1/anatomy"),
                "artifact": server.url("/v1/artifact")
            }
        }),
    )
    .expect("regression run");
    let parsed = pretty_value(&result);
    assert_eq!(parsed["run_id"], "iqh_integration");
    assert_eq!(parsed["profile"], "production");
    assert_eq!(parsed["summary"]["scenes_total"], 1);
    assert_eq!(parsed["summary"]["accepted"], 1);
    assert_eq!(parsed["summary"]["pass_rate"], 1.0);
    assert_eq!(parsed["summary"]["release_gate"]["passed"], true);
    let markdown = parsed["markdown"].as_str().expect("markdown body");
    assert!(markdown.contains("# Image-Quality Regression Report"));
    assert!(markdown.contains("scene1"));
    assert!(*dispatch.lock().unwrap(), "all paths matched a known route");
}

#[test]
fn image_regression_run_emits_lane_events_when_emit_lane_events_is_set() {
    let server = MockBackend::spawn(Arc::new(move |req| match req.path.as_str() {
        "/v1/text-to-image" => (
            200,
            json!({"artifacts": [{"uri": "s3://lane/0.png", "seed": 42}]}),
        ),
        "/v1/anatomy" => (
            200,
            json!({"regions": [], "scores": {"anatomy_score": 0.95}}),
        ),
        "/v1/artifact" => (200, json!({"artifact_score": 0.92, "issues": []})),
        other => (404, json!({"error": format!("unexpected path {other}")})),
    }));

    let result = execute_tool(
        "ImageRegressionRun",
        &json!({
            "run_id": "iqh_lane_events",
            "profile": "production",
            "emit_lane_events": true,
            "fixtures": [{
                "id": "scene_lane_events",
                "prompt": "p",
                "negative_prompt": "n",
                "width": 512,
                "height": 512,
                "model": "sdxl",
                "seeds": [42],
                "expectations": {
                    "requires_hands": true,
                    "requires_feet": false,
                    "symmetry_labels": [],
                    "pattern_labels": []
                },
                "provider": "diffusers",
                "backend_url": server.base()
            }],
            "validator_endpoints": {
                "hands_feet": server.url("/v1/anatomy"),
                "artifact": server.url("/v1/artifact")
            }
        }),
    )
    .expect("regression with events");
    let parsed = pretty_value(&result);
    let lane_events = parsed["lane_events"]
        .as_array()
        .expect("lane_events array")
        .clone();
    let event_kinds: Vec<&str> = lane_events
        .iter()
        .filter_map(|e| e["event"].as_str())
        .collect();
    assert!(
        event_kinds.contains(&"image.generate.started"),
        "got {event_kinds:?}"
    );
    assert!(event_kinds.contains(&"image.validator.ran"));
    assert!(event_kinds.contains(&"image.gate.verdict"));
    assert!(event_kinds.contains(&"image.scene.accepted"));
    assert!(event_kinds.contains(&"image.regression.summary"));

    // Verify provenance carries through the canonical fields.
    let started = lane_events
        .iter()
        .find(|e| e["event"] == "image.generate.started")
        .expect("started event");
    assert_eq!(started["data"]["scene_id"], "scene_lane_events");
    assert_eq!(started["data"]["provider"], "diffusers");
    assert_eq!(started["data"]["seed"], 42);
}

#[test]
fn image_regression_run_omits_lane_events_array_when_flag_unset() {
    let server = MockBackend::spawn(Arc::new(|req| match req.path.as_str() {
        "/v1/text-to-image" => (
            200,
            json!({"artifacts": [{"uri": "s3://e/0.png", "seed": 1}]}),
        ),
        "/v1/anatomy" => (
            200,
            json!({"regions": [], "scores": {"anatomy_score": 0.95}}),
        ),
        "/v1/artifact" => (200, json!({"artifact_score": 0.92, "issues": []})),
        _ => (404, json!({"error": "no"})),
    }));
    let result = execute_tool(
        "ImageRegressionRun",
        &json!({
            "run_id": "iqh_no_events",
            "profile": "production",
            "fixtures": [{
                "id": "scene_no_events",
                "prompt": "p",
                "negative_prompt": "n",
                "width": 512,
                "height": 512,
                "model": "sdxl",
                "seeds": [1],
                "provider": "diffusers",
                "backend_url": server.base()
            }],
            "validator_endpoints": {
                "hands_feet": server.url("/v1/anatomy"),
                "artifact": server.url("/v1/artifact")
            }
        }),
    )
    .expect("regression without events");
    let parsed = pretty_value(&result);
    let lane_events = parsed["lane_events"].as_array().expect("array");
    assert!(lane_events.is_empty());
}
