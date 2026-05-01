use std::collections::VecDeque;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::ApiError;
use crate::http_client::build_http_client_or_default;
use crate::types::{
    ContentBlockDelta, ContentBlockDeltaEvent, ContentBlockStartEvent, ContentBlockStopEvent,
    InputContentBlock, InputMessage, MessageDelta, MessageDeltaEvent, MessageRequest,
    MessageResponse, MessageStartEvent, MessageStopEvent, OutputContentBlock, StreamEvent, Usage,
};

use super::{Provider, ProviderFuture};

pub const DEFAULT_WHAM_BASE_URL: &str = "https://chatgpt.com/backend-api/wham";
const REQUEST_ID_HEADER: &str = "request-id";
const ALT_REQUEST_ID_HEADER: &str = "x-request-id";

#[derive(Debug, Clone)]
pub struct WhamClient {
    http: reqwest::Client,
    token: std::sync::Arc<std::sync::Mutex<WhamToken>>,
    base_url: String,
}

#[derive(Debug, Clone)]
struct WhamToken {
    access_token: String,
    refresh_token: Option<String>,
    expires_at: Option<u64>,
    account_id: Option<String>,
    token_url: String,
    client_id: String,
}

impl WhamClient {
    #[must_use]
    pub fn new(access_token: impl Into<String>, account_id: Option<String>) -> Self {
        Self {
            http: build_http_client_or_default(),
            token: std::sync::Arc::new(std::sync::Mutex::new(WhamToken {
                access_token: access_token.into(),
                refresh_token: None,
                expires_at: None,
                account_id,
                token_url: "https://auth.openai.com/oauth/token".to_string(),
                client_id: "app_EMoamEEZ73f0CkXaXp7hrann".to_string(),
            })),
            base_url: DEFAULT_WHAM_BASE_URL.to_string(),
        }
    }

    /// Create a WHAM client with full OAuth token info, enabling automatic refresh.
    #[must_use]
    pub fn from_oauth_token_set(
        token_set: runtime::OAuthTokenSet,
        account_id: Option<String>,
        token_url: impl Into<String>,
        client_id: impl Into<String>,
    ) -> Self {
        Self {
            http: build_http_client_or_default(),
            token: std::sync::Arc::new(std::sync::Mutex::new(WhamToken {
                access_token: token_set.access_token,
                refresh_token: token_set.refresh_token,
                expires_at: token_set.expires_at,
                account_id,
                token_url: token_url.into(),
                client_id: client_id.into(),
            })),
            base_url: DEFAULT_WHAM_BASE_URL.to_string(),
        }
    }

    #[must_use]
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    async fn ensure_token_valid(&self) -> Result<(), ApiError> {
        let needs_refresh = {
            let token = self.token.lock().map_err(|e| {
                ApiError::Auth(format!("token mutex poisoned: {e}"))
            })?;
            match token.expires_at {
                None => false,
                Some(expires_at) => {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    // Refresh if fewer than 60 seconds remain.
                    now + 60 >= expires_at
                }
            }
        };

        if !needs_refresh {
            return Ok(());
        }

        let (refresh_token, token_url, client_id) = {
            let token = self.token.lock().map_err(|e| {
                ApiError::Auth(format!("token mutex poisoned: {e}"))
            })?;
            let refresh = token.refresh_token.clone().ok_or_else(|| {
                ApiError::Auth("OAuth token expired and no refresh token available".to_string())
            })?;
            (refresh, token.token_url.clone(), token.client_id.clone())
        };

        let new_token = runtime::refresh_oauth_token(&self.http, &token_url, &client_id, &refresh_token)
            .await
            .map_err(|e| ApiError::Auth(format!("OAuth token refresh failed: {e}")))?;

        {
            let mut token = self.token.lock().map_err(|e| {
                ApiError::Auth(format!("token mutex poisoned: {e}"))
            })?;
            token.access_token = new_token.access_token.clone();
            token.refresh_token = new_token.refresh_token.clone();
            token.expires_at = new_token.expires_at;
        }

        // Persist the refreshed token.
        let _ = runtime::save_provider_oauth("openai", &new_token);

        Ok(())
    }

    pub async fn send_message(
        &self,
        request: &MessageRequest,
    ) -> Result<MessageResponse, ApiError> {
        self.ensure_token_valid().await?;
        let (access_token, account_id) = {
            let token = self.token.lock().map_err(|e| {
                ApiError::Auth(format!("token mutex poisoned: {e}"))
            })?;
            (token.access_token.clone(), token.account_id.clone())
        };

        let request_url = responses_endpoint(&self.base_url);
        let body = build_responses_request(request);

        let mut req_builder = self
            .http
            .post(&request_url)
            .header("content-type", "application/json")
            .bearer_auth(&access_token);

        if let Some(ref id) = account_id {
            req_builder = req_builder.header("ChatGPT-Account-Id", id);
        }

        let response = req_builder.json(&body).send().await.map_err(ApiError::from)?;
        let request_id = request_id_from_headers(response.headers());

        if !response.status().is_success() {
            let status = response.status();
            let body_text = response.text().await.unwrap_or_default();
            return Err(ApiError::Api {
                status,
                error_type: Some("wham_error".to_string()),
                message: Some(body_text.clone()),
                request_id,
                body: body_text,
                retryable: status.is_server_error(),
                suggested_action: suggested_action_for_status(status),
            });
        }

        let resp_body = response.text().await.map_err(ApiError::from)?;
        let wham_resp: ResponsesResponse = serde_json::from_str(&resp_body).map_err(|error| {
            ApiError::json_deserialize("OpenAI WHAM", &request.model, &resp_body, error)
        })?;

        Ok(convert_to_message_response(wham_resp, request_id))
    }

    pub async fn stream_message(
        &self,
        request: &MessageRequest,
    ) -> Result<WhamMessageStream, ApiError> {
        self.ensure_token_valid().await?;
        let (access_token, account_id) = {
            let token = self.token.lock().map_err(|e| {
                ApiError::Auth(format!("token mutex poisoned: {e}"))
            })?;
            (token.access_token.clone(), token.account_id.clone())
        };

        let request_url = responses_endpoint(&self.base_url);
        let body = build_responses_request(request);

        let mut req_builder = self
            .http
            .post(&request_url)
            .header("content-type", "application/json")
            .bearer_auth(&access_token);

        if let Some(ref id) = account_id {
            req_builder = req_builder.header("ChatGPT-Account-Id", id);
        }

        let response = req_builder.json(&body).send().await.map_err(ApiError::from)?;
        let request_id = request_id_from_headers(response.headers());

        if !response.status().is_success() {
            let status = response.status();
            let body_text = response.text().await.unwrap_or_default();
            return Err(ApiError::Api {
                status,
                error_type: Some("wham_error".to_string()),
                message: Some(body_text.clone()),
                request_id,
                body: body_text,
                retryable: status.is_server_error(),
                suggested_action: suggested_action_for_status(status),
            });
        }

        Ok(WhamMessageStream {
            response,
            buffer: Vec::new(),
            pending: VecDeque::new(),
            done: false,
            state: WhamStreamState::new(request_id),
        })
    }
}

impl Provider for WhamClient {
    type Stream = WhamMessageStream;

    fn send_message<'a>(
        &'a self,
        request: &'a MessageRequest,
    ) -> ProviderFuture<'a, MessageResponse> {
        Box::pin(async move { self.send_message(request).await })
    }

    fn stream_message<'a>(
        &'a self,
        request: &'a MessageRequest,
    ) -> ProviderFuture<'a, Self::Stream> {
        Box::pin(async move { self.stream_message(request).await })
    }
}

#[derive(Debug)]
struct WhamStreamState {
    request_id: Option<String>,
    message_started: bool,
    content_index: u32,
    text_started: bool,
    finished: bool,
    usage: Option<Usage>,
}

impl WhamStreamState {
    fn new(request_id: Option<String>) -> Self {
        Self {
            request_id,
            message_started: false,
            content_index: 0,
            text_started: false,
            finished: false,
            usage: None,
        }
    }

    fn ingest_event(&mut self, event: WhamSseEvent) -> Vec<StreamEvent> {
        let mut events = Vec::new();

        match event {
            WhamSseEvent::Created { response } | WhamSseEvent::InProgress { response } => {
                if !self.message_started {
                    self.message_started = true;
                    events.push(StreamEvent::MessageStart(MessageStartEvent {
                        message: MessageResponse {
                            id: response.id,
                            kind: "message".to_string(),
                            role: "assistant".to_string(),
                            content: Vec::new(),
                            model: response.model,
                            stop_reason: None,
                            stop_sequence: None,
                            usage: Usage::default(),
                            request_id: self.request_id.clone(),
                        },
                    }));
                }
            }
            WhamSseEvent::OutputTextDelta { content_index, delta } => {
                if !self.text_started {
                    self.text_started = true;
                    self.content_index = content_index;
                    events.push(StreamEvent::ContentBlockStart(ContentBlockStartEvent {
                        index: content_index,
                        content_block: OutputContentBlock::Text { text: String::new() },
                    }));
                }
                events.push(StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
                    index: content_index,
                    delta: ContentBlockDelta::TextDelta { text: delta },
                }));
            }
            WhamSseEvent::OutputTextDone { content_index } => {
                events.push(StreamEvent::ContentBlockStop(ContentBlockStopEvent {
                    index: content_index,
                }));
                self.text_started = false;
            }
            WhamSseEvent::Completed { usage } => {
                self.usage = usage;
            }
            _ => {}
        }

        events
    }

    fn finish(&mut self) -> Vec<StreamEvent> {
        let mut events = Vec::new();
        if self.text_started {
            events.push(StreamEvent::ContentBlockStop(ContentBlockStopEvent {
                index: self.content_index,
            }));
            self.text_started = false;
        }
        if self.message_started && !self.finished {
            self.finished = true;
            events.push(StreamEvent::MessageDelta(MessageDeltaEvent {
                delta: MessageDelta {
                    stop_reason: Some("end_turn".to_string()),
                    stop_sequence: None,
                },
                usage: self.usage.clone().unwrap_or_default(),
            }));
            events.push(StreamEvent::MessageStop(MessageStopEvent {}));
        }
        events
    }
}

#[derive(Debug)]
pub struct WhamMessageStream {
    response: reqwest::Response,
    buffer: Vec<u8>,
    pending: VecDeque<StreamEvent>,
    done: bool,
    state: WhamStreamState,
}

impl WhamMessageStream {
    #[must_use]
    pub fn request_id(&self) -> Option<&str> {
        self.state.request_id.as_deref()
    }

    pub async fn next_event(&mut self) -> Result<Option<StreamEvent>, ApiError> {
        loop {
            if let Some(event) = self.pending.pop_front() {
                return Ok(Some(event));
            }

            if self.done {
                self.pending.extend(self.state.finish());
                if let Some(event) = self.pending.pop_front() {
                    return Ok(Some(event));
                }
                return Ok(None);
            }

            match self.response.chunk().await? {
                Some(chunk) => {
                    self.buffer.extend_from_slice(&chunk);
                    while let Some(frame) = next_sse_frame(&mut self.buffer) {
                        if let Some(event) = parse_wham_sse_frame(&frame)? {
                            self.pending.extend(self.state.ingest_event(event));
                        }
                    }
                }
                None => {
                    self.done = true;
                }
            }
        }
    }
}

fn next_sse_frame(buffer: &mut Vec<u8>) -> Option<String> {
    let separator = buffer
        .windows(2)
        .position(|window| window == b"\n\n")
        .map(|position| (position, 2))
        .or_else(|| {
            buffer
                .windows(4)
                .position(|window| window == b"\r\n\r\n")
                .map(|position| (position, 4))
        })?;

    let (position, separator_len) = separator;
    let frame = buffer.drain(..position + separator_len).collect::<Vec<_>>();
    let frame_len = frame.len().saturating_sub(separator_len);
    Some(String::from_utf8_lossy(&frame[..frame_len]).into_owned())
}

#[derive(Debug, Clone, Deserialize)]
struct WhamResponseStub {
    id: String,
    model: String,
}

#[derive(Debug, Clone, Deserialize)]
struct WhamUsageStub {
    input_tokens: u32,
    output_tokens: u32,
}

#[derive(Debug, Clone)]
enum WhamSseEvent {
    Created { response: WhamResponseStub },
    InProgress { response: WhamResponseStub },
    OutputTextDelta { content_index: u32, delta: String },
    OutputTextDone { content_index: u32 },
    Completed { usage: Option<Usage> },
    Other,
}

fn parse_wham_sse_frame(frame: &str) -> Result<Option<WhamSseEvent>, ApiError> {
    let trimmed = frame.trim();
    if trimmed.is_empty() || trimmed.starts_with(':') {
        return Ok(None);
    }

    let mut event_type = String::new();
    let mut data_json = String::new();

    for line in trimmed.lines() {
        if let Some(et) = line.strip_prefix("event:") {
            event_type = et.trim().to_string();
        } else if let Some(data) = line.strip_prefix("data:") {
            data_json.push_str(data.trim_start());
        }
    }

    if data_json.is_empty() {
        return Ok(None);
    }

    let data: serde_json::Value = serde_json::from_str(&data_json)
        .map_err(|e| ApiError::json_deserialize("OpenAI WHAM", "", &data_json, e))?;

    let event = match event_type.as_str() {
        "response.created" => WhamSseEvent::Created {
            response: serde_json::from_value(data.get("response").cloned().unwrap_or_default())
                .unwrap_or(WhamResponseStub { id: String::new(), model: String::new() }),
        },
        "response.in_progress" => WhamSseEvent::InProgress {
            response: serde_json::from_value(data.get("response").cloned().unwrap_or_default())
                .unwrap_or(WhamResponseStub { id: String::new(), model: String::new() }),
        },
        "response.output_text.delta" => WhamSseEvent::OutputTextDelta {
            content_index: data["content_index"].as_u64().unwrap_or(0) as u32,
            delta: data["delta"].as_str().unwrap_or("").to_string(),
        },
        "response.output_text.done" => WhamSseEvent::OutputTextDone {
            content_index: data["content_index"].as_u64().unwrap_or(0) as u32,
        },
        "response.completed" => {
            let usage = data.get("response").and_then(|r| r.get("usage")).map(|u| Usage {
                input_tokens: u["input_tokens"].as_u64().unwrap_or(0) as u32,
                output_tokens: u["output_tokens"].as_u64().unwrap_or(0) as u32,
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: 0,
            });
            WhamSseEvent::Completed { usage }
        }
        _ => WhamSseEvent::Other,
    };

    Ok(Some(event))
}

fn responses_endpoint(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with("/responses") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/responses")
    }
}

fn request_id_from_headers(headers: &reqwest::header::HeaderMap) -> Option<String> {
    headers
        .get(REQUEST_ID_HEADER)
        .or_else(|| headers.get(ALT_REQUEST_ID_HEADER))
        .and_then(|v| v.to_str().ok())
        .map(String::from)
}

fn suggested_action_for_status(status: reqwest::StatusCode) -> Option<String> {
    match status.as_u16() {
        401 => Some("OAuth token may be expired. Try re-authenticating with `claw auth login openai`".to_string()),
        403 => Some("Verify ChatGPT subscription is active (Plus/Pro required)".to_string()),
        429 => Some("Wait a moment before retrying; consider reducing request rate".to_string()),
        _ => None,
    }
}

/// Build a Responses API request body from our internal MessageRequest.
fn build_responses_request(request: &MessageRequest) -> Value {
    // WHAM backend requires streaming mode.
    let mut body = json!({
        "model": request.model,
        "store": false,
        "stream": true,
    });

    if let Some(ref system) = request.system {
        body["instructions"] = json!(system);
    }

    // Convert messages to Responses API `input` format
    let input: Vec<Value> = request
        .messages
        .iter()
        .map(|msg| {
            let content_blocks: Vec<Value> = msg
                .content
                .iter()
                .map(|block| match block {
                    InputContentBlock::Text { text } => {
                        json!({"type": "input_text", "text": text})
                    }
                    InputContentBlock::ToolUse { id, name, input } => {
                        if msg.role == "assistant" {
                            json!({
                                "type": "tool_use",
                                "id": id,
                                "name": name,
                                "input": input,
                            })
                        } else {
                            json!({"type": "input_text", "text": "[tool input]"})
                        }
                    }
                    InputContentBlock::ToolResult { tool_use_id, content, is_error } => {
                        let text = content
                            .iter()
                            .map(|c| match c {
                                crate::types::ToolResultContentBlock::Text { text } => text.clone(),
                                crate::types::ToolResultContentBlock::Json { value } => {
                                    value.to_string()
                                }
                            })
                            .collect::<Vec<_>>()
                            .join("\n");
                        json!({
                            "type": "input_text",
                            "text": format!(
                                "[tool result {} {}]\n{}",
                                tool_use_id,
                                if *is_error { "(error)" } else { "" },
                                text
                            ),
                        })
                    }
                })
                .collect();
            json!({"role": msg.role, "content": content_blocks})
        })
        .collect();

    body["input"] = json!(input);

    // Note: WHAM backend does not support `max_output_tokens`.
    // if request.max_tokens > 0 {
    //     body["max_output_tokens"] = json!(request.max_tokens);
    // }

    if let Some(temp) = request.temperature {
        body["temperature"] = json!(temp);
    }
    if let Some(top_p) = request.top_p {
        body["top_p"] = json!(top_p);
    }

    body
}

/// Responses API response shape.
#[derive(Debug, Clone, Deserialize)]
struct ResponsesResponse {
    id: String,
    model: String,
    output: Vec<ResponsesOutputItem>,
    #[serde(default)]
    usage: Option<ResponsesUsage>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ResponsesOutputItem {
    Message {
        id: String,
        role: String,
        content: Vec<ResponsesContentItem>,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ResponsesContentItem {
    OutputText { text: String },
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct ResponsesUsage {
    input_tokens: u32,
    output_tokens: u32,
}

fn convert_to_message_response(
    wham: ResponsesResponse,
    request_id: Option<String>,
) -> MessageResponse {
    let mut content = Vec::new();
    for item in &wham.output {
        if let ResponsesOutputItem::Message { content: blocks, .. } = item {
            for block in blocks {
                if let ResponsesContentItem::OutputText { text } = block {
                    content.push(OutputContentBlock::Text { text: text.clone() });
                }
            }
        }
    }

    let usage = wham.usage.map(|u| Usage {
        input_tokens: u.input_tokens,
        output_tokens: u.output_tokens,
        cache_creation_input_tokens: 0,
        cache_read_input_tokens: 0,
    }).unwrap_or_default();

    MessageResponse {
        id: wham.id,
        kind: "message".to_string(),
        role: "assistant".to_string(),
        content,
        model: wham.model,
        stop_reason: None,
        stop_sequence: None,
        usage,
        request_id,
    }
}

fn response_to_stream_events(response: MessageResponse) -> Vec<StreamEvent> {
    let mut events = Vec::new();

    events.push(StreamEvent::MessageStart(MessageStartEvent {
        message: response.clone(),
    }));

    for (index, block) in response.content.iter().enumerate() {
        let block_index = index as u32;
        match block {
            OutputContentBlock::Text { text } => {
                events.push(StreamEvent::ContentBlockStart(ContentBlockStartEvent {
                    index: block_index,
                    content_block: OutputContentBlock::Text { text: String::new() },
                }));
                events.push(StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
                    index: block_index,
                    delta: ContentBlockDelta::TextDelta { text: text.clone() },
                }));
                events.push(StreamEvent::ContentBlockStop(ContentBlockStopEvent {
                    index: block_index,
                }));
            }
            OutputContentBlock::ToolUse { id, name, input } => {
                events.push(StreamEvent::ContentBlockStart(ContentBlockStartEvent {
                    index: block_index,
                    content_block: OutputContentBlock::ToolUse {
                        id: id.clone(),
                        name: name.clone(),
                        input: input.clone(),
                    },
                }));
                events.push(StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
                    index: block_index,
                    delta: ContentBlockDelta::InputJsonDelta {
                        partial_json: input.to_string(),
                    },
                }));
                events.push(StreamEvent::ContentBlockStop(ContentBlockStopEvent {
                    index: block_index,
                }));
            }
            OutputContentBlock::Thinking { thinking, .. } => {
                events.push(StreamEvent::ContentBlockStart(ContentBlockStartEvent {
                    index: block_index,
                    content_block: OutputContentBlock::Text { text: String::new() },
                }));
                events.push(StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
                    index: block_index,
                    delta: ContentBlockDelta::ThinkingDelta { thinking: thinking.clone() },
                }));
                events.push(StreamEvent::ContentBlockStop(ContentBlockStopEvent {
                    index: block_index,
                }));
            }
            OutputContentBlock::RedactedThinking { .. } => {
                // Skip redacted thinking blocks in stream replay
            }
        }
    }

    events.push(StreamEvent::MessageDelta(MessageDeltaEvent {
        delta: MessageDelta {
            stop_reason: response.stop_reason.clone(),
            stop_sequence: response.stop_sequence.clone(),
        },
        usage: response.usage.clone(),
    }));

    events
}
