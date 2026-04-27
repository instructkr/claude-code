use std::io;
use std::process::Stdio;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::time::timeout;

const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum LspId {
    Number(u64),
    String(String),
    Null,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LspRequest {
    pub jsonrpc: String,
    pub id: LspId,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<JsonValue>,
}

impl LspRequest {
    pub fn new(id: LspId, method: impl Into<String>, params: Option<JsonValue>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.into(),
            params,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LspNotification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<JsonValue>,
}

impl LspNotification {
    pub fn new(method: impl Into<String>, params: Option<JsonValue>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method: method.into(),
            params,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LspError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<JsonValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LspResponse {
    pub jsonrpc: String,
    pub id: LspId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<LspError>,
}

impl LspResponse {
    #[must_use]
    pub fn is_error(&self) -> bool {
        self.error.is_some()
    }

    pub fn into_result(self) -> Result<JsonValue, LspError> {
        if let Some(error) = self.error {
            Err(error)
        } else {
            Ok(self.result.unwrap_or(JsonValue::Null))
        }
    }
}

#[derive(Debug)]
pub enum LspTransportError {
    Io(io::Error),
    Timeout { method: String, timeout: Duration },
    JsonRpc(LspError),
    InvalidResponse { method: String, details: String },
    ServerExited,
}

impl std::fmt::Display for LspTransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::Timeout { method, timeout } => {
                write!(
                    f,
                    "LSP request `{method}` timed out after {}s",
                    timeout.as_secs()
                )
            }
            Self::JsonRpc(error) => {
                write!(f, "LSP JSON-RPC error: {} ({})", error.message, error.code)
            }
            Self::InvalidResponse { method, details } => {
                write!(f, "LSP invalid response for `{method}`: {details}")
            }
            Self::ServerExited => write!(f, "LSP server process exited unexpectedly"),
        }
    }
}

impl std::error::Error for LspTransportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::JsonRpc(_)
            | Self::Timeout { .. }
            | Self::InvalidResponse { .. }
            | Self::ServerExited => None,
        }
    }
}

impl From<io::Error> for LspTransportError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Debug)]
pub struct LspTransport {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
    request_timeout: Duration,
}

impl LspTransport {
    pub fn spawn(command: &str, args: &[String]) -> io::Result<Self> {
        Self::spawn_with_timeout(command, args, DEFAULT_REQUEST_TIMEOUT)
    }

    pub fn spawn_with_timeout(
        command: &str,
        args: &[String],
        request_timeout: Duration,
    ) -> io::Result<Self> {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());

        let mut child = cmd.spawn()?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("LSP process missing stdin pipe"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("LSP process missing stdout pipe"))?;

        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: 1,
            request_timeout,
        })
    }

    /// Construct an `LspTransport` from an already-spawned child process.
    /// Primarily useful for testing.
    #[cfg(test)]
    fn from_child(mut child: Child, request_timeout: Duration) -> Self {
        let stdin = child.stdin.take().expect("LSP process missing stdin pipe");
        let stdout = child
            .stdout
            .take()
            .expect("LSP process missing stdout pipe");
        Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: 1,
            request_timeout,
        }
    }

    fn allocate_id(&mut self) -> LspId {
        let id = self.next_id;
        self.next_id += 1;
        LspId::Number(id)
    }

    pub async fn send_notification(
        &mut self,
        method: &str,
        params: Option<JsonValue>,
    ) -> Result<(), LspTransportError> {
        let notification = LspNotification::new(method, params);
        let body = serde_json::to_vec(&notification)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        self.write_frame(&body).await
    }

    pub async fn send_request(
        &mut self,
        method: &str,
        params: Option<JsonValue>,
    ) -> Result<LspResponse, LspTransportError> {
        let id = self.allocate_id();
        self.send_request_with_id(method, params, id).await
    }

    pub async fn send_request_with_id(
        &mut self,
        method: &str,
        params: Option<JsonValue>,
        id: LspId,
    ) -> Result<LspResponse, LspTransportError> {
        let request = LspRequest::new(id.clone(), method, params);
        let body = serde_json::to_vec(&request)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        self.write_frame(&body).await?;

        let method_owned = method.to_string();
        let timeout_duration = self.request_timeout;
        let response = timeout(timeout_duration, self.read_response())
            .await
            .map_err(|_| LspTransportError::Timeout {
                method: method_owned,
                timeout: timeout_duration,
            })??;

        if response.jsonrpc != "2.0" {
            return Err(LspTransportError::InvalidResponse {
                method: method.to_string(),
                details: format!("unsupported jsonrpc version `{}`", response.jsonrpc),
            });
        }

        if response.id != id {
            return Err(LspTransportError::InvalidResponse {
                method: method.to_string(),
                details: format!("mismatched id: expected {:?}, got {:?}", id, response.id),
            });
        }

        if let Some(error) = &response.error {
            return Err(LspTransportError::JsonRpc(error.clone()));
        }

        Ok(response)
    }

    pub async fn read_response(&mut self) -> Result<LspResponse, LspTransportError> {
        self.read_jsonrpc_message().await
    }

    pub async fn shutdown(&mut self) -> Result<(), LspTransportError> {
        let _ = self.send_notification("shutdown", None).await;

        let _ = self.send_notification("exit", None).await;

        match self.child.try_wait() {
            Ok(Some(_)) => {}
            Ok(None) | Err(_) => {
                let _ = self.child.kill().await;
            }
        }

        Ok(())
    }

    pub fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    async fn write_frame(&mut self, payload: &[u8]) -> Result<(), LspTransportError> {
        let header = format!("Content-Length: {}\r\n\r\n", payload.len());
        self.stdin.write_all(header.as_bytes()).await?;
        self.stdin.write_all(payload).await?;
        self.stdin.flush().await?;
        Ok(())
    }

    async fn read_frame(&mut self) -> Result<Vec<u8>, LspTransportError> {
        let mut content_length: Option<usize> = None;

        loop {
            let mut line = String::new();
            let bytes_read = self.stdout.read_line(&mut line).await?;
            if bytes_read == 0 {
                return Err(LspTransportError::ServerExited);
            }
            if line == "\r\n" {
                break;
            }
            let header = line.trim_end_matches(['\r', '\n']);
            if let Some((name, value)) = header.split_once(':') {
                if name.trim().eq_ignore_ascii_case("Content-Length") {
                    let parsed = value.trim().parse::<usize>().map_err(|error| {
                        LspTransportError::Io(io::Error::new(io::ErrorKind::InvalidData, error))
                    })?;
                    content_length = Some(parsed);
                }
            }
        }

        let content_length = content_length.ok_or_else(|| LspTransportError::InvalidResponse {
            method: "unknown".to_string(),
            details: "missing Content-Length header".to_string(),
        })?;

        let mut payload = vec![0u8; content_length];
        self.stdout
            .read_exact(&mut payload)
            .await
            .map_err(|error| {
                if error.kind() == io::ErrorKind::UnexpectedEof {
                    LspTransportError::ServerExited
                } else {
                    LspTransportError::Io(error)
                }
            })?;

        Ok(payload)
    }

    async fn read_jsonrpc_message<T: serde::de::DeserializeOwned>(
        &mut self,
    ) -> Result<T, LspTransportError> {
        let payload = self.read_frame().await?;
        serde_json::from_slice(&payload).map_err(|error| LspTransportError::InvalidResponse {
            method: "unknown".to_string(),
            details: error.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

    #[test]
    fn content_length_header_roundtrip() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async {
            let payload = br#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":null}"#;

            // Write frame into a buffer
            let mut write_buf = Vec::new();
            {
                let header = format!("Content-Length: {}\r\n\r\n", payload.len());
                write_buf.extend_from_slice(header.as_bytes());
                write_buf.extend_from_slice(payload);
            }

            // Read frame back using the same logic as LspTransport::read_frame
            let cursor = Cursor::new(write_buf);
            let mut reader = BufReader::new(cursor);

            let mut content_length: Option<usize> = None;
            loop {
                let mut line = String::new();
                let bytes_read = reader.read_line(&mut line).await.unwrap();
                assert!(bytes_read > 0, "unexpected EOF reading header");
                if line == "\r\n" {
                    break;
                }
                let header = line.trim_end_matches(['\r', '\n']);
                if let Some((name, value)) = header.split_once(':') {
                    if name.trim().eq_ignore_ascii_case("Content-Length") {
                        content_length = Some(value.trim().parse::<usize>().unwrap());
                    }
                }
            }

            let content_length = content_length.expect("should have Content-Length");
            assert_eq!(content_length, payload.len());

            let mut read_payload = vec![0u8; content_length];
            reader.read_exact(&mut read_payload).await.unwrap();

            let original: serde_json::Value = serde_json::from_slice(payload).unwrap();
            let roundtripped: serde_json::Value = serde_json::from_slice(&read_payload).unwrap();
            assert_eq!(original, roundtripped);
        });
    }

    #[test]
    fn request_has_incrementing_ids() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async {
            // Spawn cat so we can construct a real LspTransport.
            let child = tokio::process::Command::new("cat")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .expect("cat should be available");

            let mut transport = LspTransport::from_child(child, Duration::from_secs(5));

            // Allocate IDs by inspecting what send_request would produce.
            let id1 = transport.allocate_id();
            let id2 = transport.allocate_id();
            let id3 = transport.allocate_id();

            assert_eq!(id1, LspId::Number(1));
            assert_eq!(id2, LspId::Number(2));
            assert_eq!(id3, LspId::Number(3));

            // Clean up
            let _ = transport.shutdown().await;
        });
    }

    #[test]
    fn notification_has_no_id() {
        let notification = LspNotification::new("initialized", Some(serde_json::json!({})));
        let serialized = serde_json::to_string(&notification).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&serialized).unwrap();
        assert!(
            parsed.get("id").is_none(),
            "notification should not contain an 'id' field, got: {serialized}"
        );
        assert_eq!(parsed["jsonrpc"], "2.0");
        assert_eq!(parsed["method"], "initialized");
    }

    #[test]
    fn malformed_header_handling() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async {
            // Feed garbage bytes that don't contain a valid Content-Length header.
            let garbage = b"THIS IS NOT A VALID HEADER\r\n\r\n";
            let cursor = Cursor::new(garbage.to_vec());
            let mut reader = BufReader::new(cursor);

            let mut content_length: Option<usize> = None;
            loop {
                let mut line = String::new();
                let bytes_read = reader.read_line(&mut line).await.unwrap();
                if bytes_read == 0 || line == "\r\n" {
                    break;
                }
                let header = line.trim_end_matches(['\r', '\n']);
                if let Some((name, value)) = header.split_once(':') {
                    if name.trim().eq_ignore_ascii_case("Content-Length") {
                        content_length = value.trim().parse::<usize>().ok();
                    }
                }
            }

            // The garbage header should not produce a valid Content-Length.
            assert!(
                content_length.is_none(),
                "garbage input should not produce a valid Content-Length"
            );
        });
    }
}
