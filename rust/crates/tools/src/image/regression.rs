//! Regression runner + CI summary output (Spec §8 / §10.6).
//!
//! Loads scene fixtures, runs each scene through the configured generator
//! and validator stages with the requested profile, and emits both a
//! machine-readable JSON report (matching §11's report contract) and a
//! markdown summary that CI can post as a status check or comment.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use runtime::{
    ImageGateVerdict, ImageRegressionSummaryPayload, ImageStepProvenance, LaneEvent,
    LaneFailureClass,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::profile::{ProfileRegistry, ResolveParams, Thresholds};
use super::provider::{
    CanonicalGenerationRequest, CanonicalGenerationResponse, HttpInvoker, ImageProviderRegistry,
};
use super::validator::{
    PatternRegion, SymmetryExpectation, ValidatorReport, ValidatorRequest, ValidatorStage,
};

/// Sink that receives `LaneEvent`s emitted during a regression run. Buffered
/// in a `Mutex<Vec<_>>` so a CI orchestrator can stream / persist them after
/// `RegressionRun::execute` returns.
#[derive(Debug, Default, Clone)]
pub struct LaneEventSink {
    inner: Arc<Mutex<Vec<LaneEvent>>>,
}

impl LaneEventSink {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&self, event: LaneEvent) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.push(event);
        }
    }

    #[must_use]
    pub fn drain(&self) -> Vec<LaneEvent> {
        self.inner
            .lock()
            .map(|mut guard| std::mem::take(&mut *guard))
            .unwrap_or_default()
    }

    #[must_use]
    pub fn snapshot(&self) -> Vec<LaneEvent> {
        self.inner.lock().map(|g| g.clone()).unwrap_or_default()
    }
}

/// Fixture format from §8.2 with the additional `provider` field needed to
/// route the scene through one of the registered adapters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegressionFixture {
    pub id: String,
    pub prompt: String,
    pub negative_prompt: String,
    pub width: u32,
    pub height: u32,
    pub model: String,
    pub seeds: Vec<u64>,
    #[serde(default)]
    pub expectations: SceneExpectations,
    pub provider: String,
    pub backend_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SceneExpectations {
    #[serde(default)]
    pub requires_hands: bool,
    #[serde(default)]
    pub requires_feet: bool,
    #[serde(default)]
    pub symmetry_labels: Vec<String>,
    #[serde(default)]
    pub pattern_labels: Vec<String>,
    #[serde(default)]
    pub symmetry_regions: Vec<SymmetryExpectation>,
    #[serde(default)]
    pub pattern_regions: Vec<PatternRegion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneOutcome {
    pub scene_id: String,
    pub seed: u64,
    pub accepted: bool,
    pub iterations: u32,
    pub catastrophic_failure: bool,
    pub scores: super::validator::NormalisedScores,
    pub anatomy_issues: Vec<String>,
    pub symmetry_violations: Vec<String>,
    pub pattern_violations: Vec<String>,
    pub artifact_issues: Vec<String>,
    pub final_image_uri: Option<String>,
    pub history: Vec<String>,
    pub error: Option<String>,
}

/// Aggregated metrics emitted by the runner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegressionSummary {
    pub run_id: String,
    pub profile: String,
    pub scenes_total: usize,
    pub seeds_total: usize,
    pub accepted: usize,
    pub rejected: usize,
    pub errored: usize,
    pub pass_rate: f64,
    pub avg_iterations_to_pass: f64,
    pub catastrophic_failure_rate: f64,
    pub regional_fix_success: BTreeMap<String, RegionalFixStats>,
    pub release_gate: ReleaseGateVerdict,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionalFixStats {
    pub repaired: usize,
    pub remaining: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseGateVerdict {
    pub passed: bool,
    pub pass_rate_threshold: f64,
    pub catastrophic_failure_threshold: f64,
    pub avg_iterations_threshold: f64,
    pub failed_metrics: Vec<String>,
}

/// Final report combining the per-scene outcomes, summary, and the CI
/// markdown body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegressionRunReport {
    pub run_id: String,
    pub profile: String,
    pub thresholds: Thresholds,
    pub outcomes: Vec<SceneOutcome>,
    pub summary: RegressionSummary,
    pub markdown: String,
}

/// Top-level config for one regression invocation.
pub struct RegressionRun<'a> {
    pub run_id: String,
    pub profile: String,
    pub fixtures: Vec<RegressionFixture>,
    pub providers: &'a ImageProviderRegistry,
    pub profiles: &'a ProfileRegistry,
    pub validator: &'a ValidatorStage,
    pub generator: Arc<dyn HttpInvoker>,
    pub release_gate: Option<ReleaseGate>,
    /// Optional sink that buffers `LaneEvent`s emitted as each scene
    /// progresses through generate / validate / gate / scene-accept-or-reject.
    /// Wire this through to surface image runs alongside code lanes in the
    /// same CI status pane.
    pub event_sink: Option<LaneEventSink>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ReleaseGate {
    pub min_pass_rate: f64,
    pub max_catastrophic_failure_rate: f64,
    pub max_avg_iterations: f64,
}

impl Default for ReleaseGate {
    fn default() -> Self {
        // Defaults from §8.3.
        Self {
            min_pass_rate: 0.85,
            max_catastrophic_failure_rate: 0.02,
            max_avg_iterations: 2.5,
        }
    }
}

impl RegressionRun<'_> {
    pub fn execute(self) -> Result<RegressionRunReport, String> {
        let resolved = self
            .profiles
            .resolve(&self.profile, ResolveParams::default())?;
        let mut outcomes = Vec::new();
        for fixture in &self.fixtures {
            for &seed in &fixture.seeds {
                outcomes.push(self.run_seed(fixture, seed, &resolved));
            }
        }
        let summary = aggregate_summary(
            &self.run_id,
            &self.profile,
            &self.fixtures,
            &outcomes,
            self.release_gate.unwrap_or_default(),
        );
        if let Some(sink) = &self.event_sink {
            let payload = ImageRegressionSummaryPayload {
                run_id: summary.run_id.clone(),
                profile: summary.profile.clone(),
                scenes_total: summary.scenes_total,
                seeds_total: summary.seeds_total,
                accepted: summary.accepted,
                rejected: summary.rejected,
                errored: summary.errored,
                pass_rate: summary.pass_rate,
                catastrophic_failure_rate: summary.catastrophic_failure_rate,
                release_gate_passed: summary.release_gate.passed,
            };
            sink.push(LaneEvent::image_regression_summary(
                emitted_at_now(),
                &payload,
            ));
        }
        let markdown = render_markdown(&summary, &outcomes);
        Ok(RegressionRunReport {
            run_id: self.run_id.clone(),
            profile: self.profile.clone(),
            thresholds: resolved.thresholds,
            outcomes,
            summary,
            markdown,
        })
    }

    fn provenance(&self, fixture: &RegressionFixture, seed: u64) -> ImageStepProvenance {
        ImageStepProvenance::new(&self.run_id, &fixture.id, seed, &fixture.provider)
            .with_profile(&self.profile)
    }

    fn emit(&self, event: LaneEvent) {
        if let Some(sink) = &self.event_sink {
            sink.push(event);
        }
    }

    #[allow(clippy::too_many_lines)]
    fn run_seed(
        &self,
        fixture: &RegressionFixture,
        seed: u64,
        resolved: &super::profile::ResolvedParams,
    ) -> SceneOutcome {
        let provenance = self.provenance(fixture, seed);
        self.emit(LaneEvent::image_generate_started(
            emitted_at_now(),
            &provenance,
        ));

        let canonical = CanonicalGenerationRequest {
            prompt: fixture.prompt.clone(),
            negative_prompt: fixture.negative_prompt.clone(),
            width: fixture.width,
            height: fixture.height,
            steps: resolved.steps,
            cfg: resolved.cfg,
            seed,
            sampler: resolved.sampler.clone(),
            model: fixture.model.clone(),
            batch_size: Some(1),
            style_preset: None,
            control_inputs: Vec::new(),
        };

        let invocation = match self.providers.translate_generate(
            &fixture.provider,
            &fixture.backend_url,
            &canonical,
        ) {
            Ok(inv) => inv,
            Err(err) => {
                let detail = format!("translate failed: {err}");
                self.emit(LaneEvent::image_scene_rejected(
                    emitted_at_now(),
                    &provenance,
                    LaneFailureClass::ImageBackend,
                    detail.clone(),
                ));
                return failure_outcome(fixture, seed, detail);
            }
        };
        let raw = match self
            .generator
            .post_json(&invocation.endpoint, &invocation.payload)
        {
            Ok(value) => value,
            Err(err) => {
                let detail = format!("generate HTTP: {err}");
                self.emit(LaneEvent::image_scene_rejected(
                    emitted_at_now(),
                    &provenance,
                    LaneFailureClass::ImageBackend,
                    detail.clone(),
                ));
                return failure_outcome(fixture, seed, detail);
            }
        };
        let parsed: CanonicalGenerationResponse =
            match self.providers.parse_response(&fixture.provider, &raw) {
                Ok(p) => p,
                Err(err) => {
                    let detail = format!("parse failed: {err}");
                    self.emit(LaneEvent::image_scene_rejected(
                        emitted_at_now(),
                        &provenance,
                        LaneFailureClass::ImageBackend,
                        detail.clone(),
                    ));
                    return failure_outcome(fixture, seed, detail);
                }
            };
        let Some(image) = parsed.images.first().cloned() else {
            let detail = "no images returned".to_string();
            self.emit(LaneEvent::image_scene_rejected(
                emitted_at_now(),
                &provenance,
                LaneFailureClass::ImageBackend,
                detail.clone(),
            ));
            return failure_outcome(fixture, seed, detail);
        };

        let scene_provenance = provenance.clone().with_final_image_uri(image.uri.clone());

        let req = ValidatorRequest {
            image_uri: image.uri.clone(),
            symmetry_expectations: fixture.expectations.symmetry_regions.clone(),
            pattern_regions: fixture.expectations.pattern_regions.clone(),
            ..Default::default()
        };
        let report: ValidatorReport = match self.validator.run(&req) {
            Ok(r) => r,
            Err(err) => {
                let detail = format!("validator: {err}");
                self.emit(LaneEvent::image_scene_rejected(
                    emitted_at_now(),
                    &scene_provenance,
                    LaneFailureClass::ImageValidator,
                    detail.clone(),
                ));
                return failure_outcome(fixture, seed, detail);
            }
        };
        self.emit(LaneEvent::image_validator_ran(
            emitted_at_now(),
            &scene_provenance,
            report.scores.weighted_total(),
        ));

        let expects_symmetry = !fixture.expectations.symmetry_regions.is_empty();
        let expects_pattern = !fixture.expectations.pattern_regions.is_empty();
        let accepted =
            report
                .scores
                .passes(&resolved.thresholds, expects_symmetry, expects_pattern);

        let verdict = ImageGateVerdict {
            passed: accepted,
            anatomy_score: report.scores.anatomy_score,
            symmetry_score: report.scores.symmetry_score,
            pattern_score: report.scores.pattern_score,
            artifact_score: report.scores.artifact_score,
            creative_score: report.scores.creative_score,
            weighted_total: report.scores.weighted_total(),
        };
        self.emit(LaneEvent::image_gate_verdict(
            emitted_at_now(),
            &scene_provenance,
            &verdict,
        ));

        // Catastrophic = anatomy/symmetry below "0.5 × threshold" floor.
        let catastrophic = report.scores.anatomy_score < resolved.thresholds.anatomy * 0.5
            || (expects_symmetry
                && report.scores.symmetry_score < resolved.thresholds.symmetry * 0.5);

        if accepted {
            self.emit(LaneEvent::image_scene_accepted(
                emitted_at_now(),
                &scene_provenance,
            ));
        } else {
            // Plan a repair if violations are recoverable; emit the repair-planned
            // event so CI sees what the inpaint would target. The actual inpaint
            // call lives in the iterative loop (Spec §6) — this hook keeps the
            // event surface ready when that lands.
            let mut repair_kinds = Vec::new();
            if !report.anatomy_issues.is_empty() {
                repair_kinds.push("anatomy");
            }
            if !report.symmetry_violations.is_empty() {
                repair_kinds.push("symmetry");
            }
            if !report.pattern_violations.is_empty() {
                repair_kinds.push("pattern");
            }
            if !report.artifact_issues.is_empty() {
                repair_kinds.push("artifact");
            }
            if !repair_kinds.is_empty() {
                self.emit(LaneEvent::image_repair_planned(
                    emitted_at_now(),
                    &scene_provenance,
                    &repair_kinds,
                ));
            }
            let detail = if catastrophic {
                "catastrophic anatomy/symmetry score".to_string()
            } else {
                format!(
                    "weighted_total={:.3} below gate {:.3}",
                    report.scores.weighted_total(),
                    resolved.thresholds.weighted_total
                )
            };
            self.emit(LaneEvent::image_scene_rejected(
                emitted_at_now(),
                &scene_provenance,
                LaneFailureClass::ImageRegressionGate,
                detail,
            ));
        }

        SceneOutcome {
            scene_id: fixture.id.clone(),
            seed,
            accepted,
            iterations: u32::from(!accepted), // 0 if accepted on first pass, 1 if a repair would be required
            catastrophic_failure: catastrophic && !accepted,
            scores: report.scores,
            anatomy_issues: report.anatomy_issues,
            symmetry_violations: report.symmetry_violations,
            pattern_violations: report.pattern_violations,
            artifact_issues: report.artifact_issues,
            final_image_uri: Some(image.uri),
            history: parsed.images.iter().map(|img| img.uri.clone()).collect(),
            error: None,
        }
    }
}

fn emitted_at_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or_else(|_| "0".to_string(), |d| d.as_millis().to_string())
}

fn failure_outcome(fixture: &RegressionFixture, seed: u64, error: String) -> SceneOutcome {
    SceneOutcome {
        scene_id: fixture.id.clone(),
        seed,
        accepted: false,
        iterations: 0,
        catastrophic_failure: true,
        scores: super::validator::NormalisedScores {
            anatomy_score: 0.0,
            symmetry_score: 0.0,
            pattern_score: 0.0,
            artifact_score: 0.0,
            creative_score: 0.0,
        },
        anatomy_issues: Vec::new(),
        symmetry_violations: Vec::new(),
        pattern_violations: Vec::new(),
        artifact_issues: Vec::new(),
        final_image_uri: None,
        history: Vec::new(),
        error: Some(error),
    }
}

/// Aggregate scene outcomes into the run summary used for CI status.
fn aggregate_summary(
    run_id: &str,
    profile: &str,
    fixtures: &[RegressionFixture],
    outcomes: &[SceneOutcome],
    gate: ReleaseGate,
) -> RegressionSummary {
    let scenes_total = fixtures.len();
    let seeds_total = outcomes.len();
    let accepted = outcomes.iter().filter(|o| o.accepted).count();
    let errored = outcomes.iter().filter(|o| o.error.is_some()).count();
    let rejected = seeds_total - accepted - errored;
    let pass_rate = ratio(accepted, seeds_total);
    let total_iters: u32 = outcomes
        .iter()
        .filter(|o| o.accepted)
        .map(|o| o.iterations)
        .sum();
    let avg_iterations_to_pass = if accepted == 0 {
        0.0
    } else {
        f64::from(total_iters) / usize_to_f64(accepted)
    };
    let cat_count = outcomes.iter().filter(|o| o.catastrophic_failure).count();
    let catastrophic_failure_rate = ratio(cat_count, seeds_total);

    let mut regional_fix_success: BTreeMap<String, RegionalFixStats> = BTreeMap::new();
    for outcome in outcomes {
        for axis in [
            ("anatomy", &outcome.anatomy_issues),
            ("symmetry", &outcome.symmetry_violations),
            ("pattern", &outcome.pattern_violations),
            ("artifact", &outcome.artifact_issues),
        ] {
            let entry = regional_fix_success
                .entry(axis.0.to_string())
                .or_insert_with(|| RegionalFixStats {
                    repaired: 0,
                    remaining: 0,
                });
            if outcome.accepted {
                entry.repaired += axis.1.len();
            } else {
                entry.remaining += axis.1.len();
            }
        }
    }

    let mut failed_metrics = Vec::new();
    if pass_rate < gate.min_pass_rate {
        failed_metrics.push(format!(
            "pass_rate={pass_rate:.3} < {:.3}",
            gate.min_pass_rate
        ));
    }
    if catastrophic_failure_rate > gate.max_catastrophic_failure_rate {
        failed_metrics.push(format!(
            "catastrophic_failure_rate={catastrophic_failure_rate:.3} > {:.3}",
            gate.max_catastrophic_failure_rate
        ));
    }
    if avg_iterations_to_pass > gate.max_avg_iterations {
        failed_metrics.push(format!(
            "avg_iterations_to_pass={avg_iterations_to_pass:.3} > {:.3}",
            gate.max_avg_iterations
        ));
    }

    RegressionSummary {
        run_id: run_id.to_string(),
        profile: profile.to_string(),
        scenes_total,
        seeds_total,
        accepted,
        rejected,
        errored,
        pass_rate,
        avg_iterations_to_pass,
        catastrophic_failure_rate,
        regional_fix_success,
        release_gate: ReleaseGateVerdict {
            passed: failed_metrics.is_empty(),
            pass_rate_threshold: gate.min_pass_rate,
            catastrophic_failure_threshold: gate.max_catastrophic_failure_rate,
            avg_iterations_threshold: gate.max_avg_iterations,
            failed_metrics,
        },
    }
}

fn render_markdown(summary: &RegressionSummary, outcomes: &[SceneOutcome]) -> String {
    use std::fmt::Write;
    let mut buf = String::new();
    buf.push_str("# Image-Quality Regression Report\n\n");
    let _ = writeln!(buf, "- **Run id**: `{}`", summary.run_id);
    let _ = writeln!(buf, "- **Profile**: `{}`", summary.profile);
    let _ = writeln!(
        buf,
        "- **Scenes / seeds**: {} / {}",
        summary.scenes_total, summary.seeds_total
    );
    let _ = writeln!(
        buf,
        "- **Accepted**: {} ({:.1}%)",
        summary.accepted,
        summary.pass_rate * 100.0
    );
    let _ = writeln!(buf, "- **Rejected**: {}", summary.rejected);
    let _ = writeln!(buf, "- **Errored**: {}", summary.errored);
    let _ = writeln!(
        buf,
        "- **Catastrophic failures**: {:.2}% (gate ≤ {:.2}%)",
        summary.catastrophic_failure_rate * 100.0,
        summary.release_gate.catastrophic_failure_threshold * 100.0
    );
    let _ = writeln!(
        buf,
        "- **Avg iterations to pass**: {:.2} (gate ≤ {:.2})",
        summary.avg_iterations_to_pass, summary.release_gate.avg_iterations_threshold
    );
    let verdict = if summary.release_gate.passed {
        "✅ PASSED"
    } else {
        "❌ FAILED"
    };
    let _ = writeln!(buf, "- **Release gate**: {verdict}");
    if !summary.release_gate.failed_metrics.is_empty() {
        buf.push_str("  - Failed metrics:\n");
        for m in &summary.release_gate.failed_metrics {
            let _ = writeln!(buf, "    - {m}");
        }
    }
    buf.push_str("\n## Per-axis fix stats\n\n");
    buf.push_str("| Axis | Repaired | Remaining |\n|------|----------|-----------|\n");
    for (axis, stats) in &summary.regional_fix_success {
        let _ = writeln!(buf, "| {axis} | {} | {} |", stats.repaired, stats.remaining);
    }
    buf.push_str("\n## Per-scene outcomes\n\n");
    buf.push_str(
        "| Scene | Seed | Accepted | Anatomy | Symmetry | Pattern | Artifact | Weighted |\n",
    );
    buf.push_str(
        "|-------|------|----------|---------|----------|---------|----------|----------|\n",
    );
    for outcome in outcomes {
        let s = &outcome.scores;
        let _ = writeln!(
            buf,
            "| `{}` | {} | {} | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} |",
            outcome.scene_id,
            outcome.seed,
            if outcome.accepted { "yes" } else { "no" },
            s.anatomy_score,
            s.symmetry_score,
            s.pattern_score,
            s.artifact_score,
            s.weighted_total()
        );
    }
    buf
}

/// `usize → f64` conversion that matches the precision-loss expectations
/// implied by `clippy::cast_precision_loss` while still letting us divide.
#[allow(clippy::cast_precision_loss)]
fn usize_to_f64(value: usize) -> f64 {
    value as f64
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        usize_to_f64(numerator) / usize_to_f64(denominator)
    }
}

/// Helper used by CI to load fixtures from a JSON file path.
pub fn load_fixtures(path: &Path) -> Result<Vec<RegressionFixture>, String> {
    let raw = std::fs::read_to_string(path).map_err(|err| err.to_string())?;
    parse_fixtures(&raw)
}

pub fn parse_fixtures(raw: &str) -> Result<Vec<RegressionFixture>, String> {
    let value: Value = serde_json::from_str(raw).map_err(|err| err.to_string())?;
    if let Some(array) = value.as_array() {
        return serde_json::from_value::<Vec<RegressionFixture>>(Value::Array(array.clone()))
            .map_err(|err| err.to_string());
    }
    if let Some(scenes) = value.get("scenes").cloned() {
        return serde_json::from_value::<Vec<RegressionFixture>>(scenes)
            .map_err(|err| err.to_string());
    }
    Err("regression fixture JSON must be array or object with `scenes`".to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use serde_json::json;

    use super::*;
    use crate::image::profile::ProfileRegistry;
    use crate::image::provider::ImageProviderRegistry;
    use crate::image::validator::{NormalisedScores, ValidatorEndpoints};

    #[derive(Default)]
    struct ScriptedInvoker {
        responses: Mutex<Vec<(String, Value)>>,
    }

    impl ScriptedInvoker {
        fn push(&self, url: &str, value: Value) {
            self.responses
                .lock()
                .unwrap()
                .push((url.to_string(), value));
        }
    }

    impl HttpInvoker for ScriptedInvoker {
        fn post_json(&self, url: &str, _payload: &Value) -> Result<Value, String> {
            let mut guard = self.responses.lock().unwrap();
            let pos = guard
                .iter()
                .position(|(u, _)| u == url)
                .ok_or_else(|| format!("no scripted response for {url}"))?;
            Ok(guard.remove(pos).1)
        }
    }

    fn diffusers_response(uri: &str, seed: u64) -> Value {
        json!({"artifacts": [{"uri": uri, "seed": seed}]})
    }

    fn validator_pass() -> Value {
        json!({
            "regions": [],
            "scores": {"anatomy_score": 0.95}
        })
    }

    #[test]
    fn parse_fixtures_accepts_array_or_scenes_object() {
        let raw = r#"[{
            "id": "s1", "prompt": "p", "negative_prompt": "n",
            "width": 512, "height": 512, "model": "m",
            "seeds": [1], "provider": "diffusers", "backend_url": "http://x"
        }]"#;
        let fixtures = parse_fixtures(raw).expect("array");
        assert_eq!(fixtures.len(), 1);

        let raw_obj = format!(r#"{{"scenes": {raw}}}"#);
        let fixtures = parse_fixtures(&raw_obj).expect("object");
        assert_eq!(fixtures.len(), 1);
    }

    #[test]
    fn execute_reports_pass_for_high_quality_diffusers_response() {
        let providers = ImageProviderRegistry::builtin();
        let profiles = ProfileRegistry::builtin();
        let validator_invoker = Arc::new(ScriptedInvoker::default());
        validator_invoker.push("http://val/hands", validator_pass());
        validator_invoker.push(
            "http://val/art",
            json!({"artifact_score": 0.92, "issues": []}),
        );

        let validator = ValidatorStage::new(
            ValidatorEndpoints {
                hands_feet: Some("http://val/hands".to_string()),
                artifact: Some("http://val/art".to_string()),
                ..Default::default()
            },
            validator_invoker,
        );

        let generator = Arc::new(ScriptedInvoker::default());
        generator.push(
            "http://diff/v1/text-to-image",
            diffusers_response("s3://run/0.png", 1),
        );
        let fixtures = vec![RegressionFixture {
            id: "scene1".to_string(),
            prompt: "p".to_string(),
            negative_prompt: "n".to_string(),
            width: 1024,
            height: 1024,
            model: "sdxl".to_string(),
            seeds: vec![1],
            expectations: SceneExpectations::default(),
            provider: "diffusers".to_string(),
            backend_url: "http://diff".to_string(),
        }];

        let report = RegressionRun {
            run_id: "iqh_test".to_string(),
            profile: "production".to_string(),
            fixtures,
            providers: &providers,
            profiles: &profiles,
            validator: &validator,
            generator,
            release_gate: None,
            event_sink: None,
        }
        .execute()
        .expect("execute");

        assert_eq!(report.outcomes.len(), 1);
        let outcome = &report.outcomes[0];
        assert!(outcome.accepted, "scores: {:?}", outcome.scores);
        assert_eq!(outcome.final_image_uri.as_deref(), Some("s3://run/0.png"));
        assert!((report.summary.pass_rate - 1.0).abs() < f64::EPSILON);
        assert!(report.summary.release_gate.passed);
        assert!(report.markdown.contains("✅ PASSED"));
        assert!(report.markdown.contains("scene1"));
    }

    #[test]
    fn execute_marks_release_gate_failed_when_pass_rate_drops_below_threshold() {
        let providers = ImageProviderRegistry::builtin();
        let profiles = ProfileRegistry::builtin();
        let validator_invoker = Arc::new(ScriptedInvoker::default());
        // Anatomy too low to pass production thresholds.
        validator_invoker.push(
            "http://val/hands",
            json!({"regions": [], "scores": {"anatomy_score": 0.40}}),
        );

        let validator = ValidatorStage::new(
            ValidatorEndpoints {
                hands_feet: Some("http://val/hands".to_string()),
                ..Default::default()
            },
            validator_invoker,
        );

        let generator = Arc::new(ScriptedInvoker::default());
        generator.push(
            "http://diff/v1/text-to-image",
            diffusers_response("s3://bad/0.png", 7),
        );

        let fixtures = vec![RegressionFixture {
            id: "scene-bad".to_string(),
            prompt: "p".to_string(),
            negative_prompt: "n".to_string(),
            width: 512,
            height: 512,
            model: "sdxl".to_string(),
            seeds: vec![7],
            expectations: SceneExpectations::default(),
            provider: "diffusers".to_string(),
            backend_url: "http://diff".to_string(),
        }];

        let report = RegressionRun {
            run_id: "iqh_test_bad".to_string(),
            profile: "production".to_string(),
            fixtures,
            providers: &providers,
            profiles: &profiles,
            validator: &validator,
            generator,
            release_gate: None,
            event_sink: None,
        }
        .execute()
        .expect("execute");

        let outcome = &report.outcomes[0];
        assert!(!outcome.accepted);
        assert!(outcome.catastrophic_failure);
        assert!(report.summary.pass_rate.abs() < f64::EPSILON);
        assert!(!report.summary.release_gate.passed);
        assert!(report
            .summary
            .release_gate
            .failed_metrics
            .iter()
            .any(|m| m.contains("pass_rate")));
        assert!(report.markdown.contains("❌ FAILED"));
    }

    #[test]
    fn execute_handles_provider_translation_failure_as_errored_outcome() {
        let providers = ImageProviderRegistry::builtin();
        let profiles = ProfileRegistry::builtin();
        let validator = ValidatorStage::new(
            ValidatorEndpoints::default(),
            Arc::new(ScriptedInvoker::default()),
        );
        let generator = Arc::new(ScriptedInvoker::default());
        let fixtures = vec![RegressionFixture {
            id: "bad-provider".to_string(),
            prompt: "p".to_string(),
            negative_prompt: "n".to_string(),
            width: 512,
            height: 512,
            model: "sdxl".to_string(),
            seeds: vec![1],
            expectations: SceneExpectations::default(),
            provider: "nope".to_string(),
            backend_url: "http://x".to_string(),
        }];
        let report = RegressionRun {
            run_id: "iqh_err".to_string(),
            profile: "production".to_string(),
            fixtures,
            providers: &providers,
            profiles: &profiles,
            validator: &validator,
            generator,
            release_gate: None,
            event_sink: None,
        }
        .execute()
        .expect("execute");
        let outcome = &report.outcomes[0];
        assert!(!outcome.accepted);
        assert!(outcome.error.as_deref().unwrap().contains("translate"));
        assert_eq!(report.summary.errored, 1);
        assert_eq!(report.summary.rejected, 0);
    }

    #[test]
    fn render_markdown_includes_fix_stats_table() {
        let outcomes = vec![SceneOutcome {
            scene_id: "x".to_string(),
            seed: 1,
            accepted: true,
            iterations: 0,
            catastrophic_failure: false,
            scores: NormalisedScores {
                anatomy_score: 0.95,
                symmetry_score: 0.95,
                pattern_score: 0.95,
                artifact_score: 0.95,
                creative_score: 0.5,
            },
            anatomy_issues: vec![],
            symmetry_violations: vec![],
            pattern_violations: vec![],
            artifact_issues: vec![],
            final_image_uri: None,
            history: vec![],
            error: None,
        }];
        let summary =
            aggregate_summary("rid", "production", &[], &outcomes, ReleaseGate::default());
        let md = render_markdown(&summary, &outcomes);
        assert!(md.contains("| Axis | Repaired | Remaining |"));
        assert!(md.contains("| anatomy |"));
    }
}
