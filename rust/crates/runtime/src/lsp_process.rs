//! LSP process manager: spawns language servers and drives the LSP lifecycle.

use std::path::Path;

use serde_json::Value as JsonValue;

use crate::lsp_client::{
    LspCompletionItem, LspHoverResult, LspLocation, LspServerStatus, LspSymbol,
};
use crate::lsp_transport::{LspTransport, LspTransportError};

#[derive(Debug)]
pub struct LspProcess {
    transport: LspTransport,
    language: String,
    root_uri: String,
    capabilities: Option<JsonValue>,
    status: LspServerStatus,
}

#[allow(clippy::cast_possible_truncation)]
impl LspProcess {
    /// Spawn a language server process and perform the LSP initialize handshake.
    pub async fn start(
        command: &str,
        args: &[String],
        root_path: &Path,
    ) -> Result<Self, LspProcessError> {
        let transport = LspTransport::spawn(command, args)
            .map_err(|e| LspProcessError::Transport(LspTransportError::Io(e)))?;

        let canonical = canonicalize_root(root_path)?;
        let root_uri = format!("file://{canonical}");

        let mut process = Self {
            transport,
            language: command.to_owned(),
            root_uri: root_uri.clone(),
            capabilities: None,
            status: LspServerStatus::Starting,
        };

        process.initialize(&canonical).await?;
        process.status = LspServerStatus::Connected;

        Ok(process)
    }

    /// Send the LSP `initialize` request followed by the `initialized` notification.
    async fn initialize(&mut self, root_path: &str) -> Result<JsonValue, LspProcessError> {
        let root_uri = format!("file://{root_path}");
        let pid = std::process::id();

        let params = serde_json::json!({
            "processId": pid,
            "rootUri": root_uri,
            "capabilities": {
                "textDocument": {
                    "hover": { "contentFormat": ["markdown", "plaintext"] },
                    "definition": { "linkSupport": true },
                    "references": {},
                    "completion": {
                        "completionItem": { "snippetSupport": false }
                    },
                    "documentSymbol": { "hierarchicalDocumentSymbolSupport": true },
                    "publishDiagnostics": { "relatedInformation": true }
                }
            }
        });

        let response = self
            .transport
            .send_request("initialize", Some(params))
            .await
            .map_err(LspProcessError::Transport)?;

        let result = response
            .into_result()
            .map_err(|e| LspProcessError::Transport(LspTransportError::JsonRpc(e)))?;

        self.capabilities = Some(result.clone());

        self.transport
            .send_notification("initialized", Some(serde_json::json!({})))
            .await
            .map_err(LspProcessError::Transport)?;

        Ok(result)
    }

    /// Gracefully shut down the language server.
    pub async fn shutdown(&mut self) -> Result<(), LspProcessError> {
        self.status = LspServerStatus::Disconnected;

        let shutdown_result = self
            .transport
            .send_request("shutdown", None)
            .await
            .map_err(LspProcessError::Transport);

        if shutdown_result.is_ok() {
            self.transport
                .send_notification("exit", None)
                .await
                .map_err(LspProcessError::Transport)?;
        }

        self.transport
            .shutdown()
            .await
            .map_err(LspProcessError::Transport)?;

        Ok(())
    }

    /// Query hover information at a position.
    pub async fn hover(
        &mut self,
        path: &str,
        line: u32,
        character: u32,
    ) -> Result<Option<LspHoverResult>, LspProcessError> {
        let uri = path_to_uri(path);
        let params = text_document_position_params(&uri, line, character);

        let response = self
            .transport
            .send_request("textDocument/hover", Some(params))
            .await
            .map_err(LspProcessError::Transport)?;

        let result = response
            .into_result()
            .map_err(|e| LspProcessError::Transport(LspTransportError::JsonRpc(e)))?;

        if result.is_null() {
            return Ok(None);
        }

        Ok(parse_hover(&result))
    }

    /// Go to definition at a position.
    pub async fn goto_definition(
        &mut self,
        path: &str,
        line: u32,
        character: u32,
    ) -> Result<Vec<LspLocation>, LspProcessError> {
        let uri = path_to_uri(path);
        let params = text_document_position_params(&uri, line, character);

        let response = self
            .transport
            .send_request("textDocument/definition", Some(params))
            .await
            .map_err(LspProcessError::Transport)?;

        let result = response
            .into_result()
            .map_err(|e| LspProcessError::Transport(LspTransportError::JsonRpc(e)))?;

        Ok(parse_locations(&result))
    }

    /// Find references at a position.
    pub async fn references(
        &mut self,
        path: &str,
        line: u32,
        character: u32,
    ) -> Result<Vec<LspLocation>, LspProcessError> {
        let uri = path_to_uri(path);
        let params = serde_json::json!({
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": character },
            "context": { "includeDeclaration": true }
        });

        let response = self
            .transport
            .send_request("textDocument/references", Some(params))
            .await
            .map_err(LspProcessError::Transport)?;

        let result = response
            .into_result()
            .map_err(|e| LspProcessError::Transport(LspTransportError::JsonRpc(e)))?;

        Ok(parse_locations(&result))
    }

    /// Get document symbols for a file.
    pub async fn document_symbols(
        &mut self,
        path: &str,
    ) -> Result<Vec<LspSymbol>, LspProcessError> {
        let uri = path_to_uri(path);
        let params = serde_json::json!({
            "textDocument": { "uri": uri }
        });

        let response = self
            .transport
            .send_request("textDocument/documentSymbol", Some(params))
            .await
            .map_err(LspProcessError::Transport)?;

        let result = response
            .into_result()
            .map_err(|e| LspProcessError::Transport(LspTransportError::JsonRpc(e)))?;

        if result.is_null() {
            return Ok(Vec::new());
        }

        Ok(parse_symbols(&result, path))
    }

    /// Get completions at a position.
    pub async fn completion(
        &mut self,
        path: &str,
        line: u32,
        character: u32,
    ) -> Result<Vec<LspCompletionItem>, LspProcessError> {
        let uri = path_to_uri(path);
        let params = text_document_position_params(&uri, line, character);

        let response = self
            .transport
            .send_request("textDocument/completion", Some(params))
            .await
            .map_err(LspProcessError::Transport)?;

        let result = response
            .into_result()
            .map_err(|e| LspProcessError::Transport(LspTransportError::JsonRpc(e)))?;

        if result.is_null() {
            return Ok(Vec::new());
        }

        // The response may be a CompletionList or a plain array.
        let items = if let Some(list) = result.get("items") {
            list
        } else {
            &result
        };

        Ok(parse_completions(items))
    }

    /// Format a document.
    pub async fn format(&mut self, path: &str) -> Result<Vec<JsonValue>, LspProcessError> {
        let uri = path_to_uri(path);
        let params = serde_json::json!({
            "textDocument": { "uri": uri },
            "options": { "tabSize": 4, "insertSpaces": true }
        });

        let response = self
            .transport
            .send_request("textDocument/formatting", Some(params))
            .await
            .map_err(LspProcessError::Transport)?;

        let result = response
            .into_result()
            .map_err(|e| LspProcessError::Transport(LspTransportError::JsonRpc(e)))?;

        if result.is_null() {
            return Ok(Vec::new());
        }

        match result.as_array() {
            Some(arr) => Ok(arr.clone()),
            None => Ok(Vec::new()),
        }
    }

    #[must_use]
    pub fn status(&self) -> LspServerStatus {
        self.status
    }

    #[must_use]
    pub fn language(&self) -> &str {
        &self.language
    }

    #[must_use]
    pub fn root_uri(&self) -> &str {
        &self.root_uri
    }
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum LspProcessError {
    Transport(LspTransportError),
    InvalidPath(String),
}

impl std::fmt::Display for LspProcessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transport(e) => write!(f, "LSP transport error: {e}"),
            Self::InvalidPath(p) => write!(f, "invalid path: {p}"),
        }
    }
}

impl std::error::Error for LspProcessError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Transport(e) => Some(e),
            Self::InvalidPath(_) => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn canonicalize_root(path: &Path) -> Result<String, LspProcessError> {
    path.canonicalize()
        .map_err(|e| LspProcessError::InvalidPath(format!("{}: {e}", path.display())))
        .map(|p| p.to_string_lossy().into_owned())
}

fn path_to_uri(path: &str) -> String {
    let canonical = std::path::Path::new(path);
    if canonical.is_absolute() {
        format!("file://{path}")
    } else {
        let resolved =
            std::env::current_dir().map_or_else(|_| canonical.to_path_buf(), |d| d.join(path));
        let canonicalized = resolved
            .canonicalize()
            .unwrap_or(resolved)
            .to_string_lossy()
            .into_owned();
        format!("file://{canonicalized}")
    }
}

fn text_document_position_params(uri: &str, line: u32, character: u32) -> JsonValue {
    serde_json::json!({
        "textDocument": { "uri": uri },
        "position": { "line": line, "character": character }
    })
}

fn uri_to_path(uri: &str) -> String {
    uri.strip_prefix("file://").unwrap_or(uri).to_owned()
}

fn parse_hover(value: &JsonValue) -> Option<LspHoverResult> {
    let contents = value.get("contents")?;

    // MarkupContent: { kind, value }
    if let (Some(kind), Some(val)) = (contents.get("kind"), contents.get("value")) {
        let language = if kind.as_str() == Some("plaintext") {
            None
        } else {
            Some(kind.as_str().unwrap_or("markdown").to_owned())
        };
        return Some(LspHoverResult {
            content: val.as_str().unwrap_or("").to_owned(),
            language,
        });
    }

    // MarkedString object: { language, value }
    if let (Some(lang), Some(val)) = (contents.get("language"), contents.get("value")) {
        return Some(LspHoverResult {
            content: val.as_str().unwrap_or("").to_owned(),
            language: Some(lang.as_str().unwrap_or("").to_owned()),
        });
    }

    // Plain string MarkedString
    if let Some(s) = contents.as_str() {
        return Some(LspHoverResult {
            content: s.to_owned(),
            language: None,
        });
    }

    // Array of MarkedString
    if let Some(arr) = contents.as_array() {
        let parts: Vec<&str> = arr
            .iter()
            .filter_map(|item| {
                if let Some(s) = item.as_str() {
                    Some(s)
                } else {
                    item.get("value").and_then(JsonValue::as_str)
                }
            })
            .collect();
        if parts.is_empty() {
            return None;
        }
        return Some(LspHoverResult {
            content: parts.join("\n"),
            language: None,
        });
    }

    None
}

#[allow(clippy::cast_possible_truncation)]
fn parse_locations(value: &JsonValue) -> Vec<LspLocation> {
    let Some(locations) = value.as_array() else {
        return Vec::new();
    };

    locations
        .iter()
        .filter_map(|loc| {
            let uri = loc.get("uri")?.as_str()?;
            let path = uri_to_path(uri);
            let range = loc.get("range")?;
            let start = range.get("start")?;
            let end = range.get("end")?;

            Some(LspLocation {
                path,
                line: start.get("line")?.as_u64()? as u32,
                character: start.get("character")?.as_u64()? as u32,
                end_line: end
                    .get("line")
                    .and_then(JsonValue::as_u64)
                    .map(|v| v as u32),
                end_character: end
                    .get("character")
                    .and_then(JsonValue::as_u64)
                    .map(|v| v as u32),
                preview: None,
            })
        })
        .collect()
}

fn extract_symbols(items: &[JsonValue], path: &str, out: &mut Vec<LspSymbol>) {
    for item in items {
        let name = item.get("name").and_then(JsonValue::as_str).unwrap_or("");
        let kind = item
            .get("kind")
            .and_then(JsonValue::as_u64)
            .map_or_else(|| "Unknown".into(), symbol_kind_name);

        let (sym_path, line, character) = if let Some(range) = item.get("range") {
            let start = range.get("start");
            (
                path.to_owned(),
                u32::try_from(
                    start
                        .and_then(|s| s.get("line"))
                        .and_then(JsonValue::as_u64)
                        .unwrap_or(0),
                )
                .unwrap_or(0),
                u32::try_from(
                    start
                        .and_then(|s| s.get("character"))
                        .and_then(JsonValue::as_u64)
                        .unwrap_or(0),
                )
                .unwrap_or(0),
            )
        } else {
            (path.to_owned(), 0, 0)
        };

        out.push(LspSymbol {
            name: name.to_owned(),
            kind: kind.clone(),
            path: sym_path,
            line,
            character,
        });

        if let Some(children) = item.get("children").and_then(JsonValue::as_array) {
            extract_symbols(children, path, out);
        }
    }
}

fn parse_symbols(value: &JsonValue, default_path: &str) -> Vec<LspSymbol> {
    let Some(items) = value.as_array() else {
        return Vec::new();
    };

    let mut result = Vec::new();
    extract_symbols(items, default_path, &mut result);
    result
}

fn parse_completions(value: &JsonValue) -> Vec<LspCompletionItem> {
    let Some(items) = value.as_array() else {
        return Vec::new();
    };

    items
        .iter()
        .map(|item| LspCompletionItem {
            label: item
                .get("label")
                .and_then(JsonValue::as_str)
                .unwrap_or("")
                .to_owned(),
            kind: item
                .get("kind")
                .and_then(JsonValue::as_u64)
                .map(completion_kind_name),
            detail: item
                .get("detail")
                .and_then(JsonValue::as_str)
                .map(str::to_owned),
            insert_text: item
                .get("insertText")
                .and_then(JsonValue::as_str)
                .map(str::to_owned),
        })
        .collect()
}

fn symbol_kind_name(kind: u64) -> String {
    match kind {
        1 => "File".into(),
        2 => "Module".into(),
        3 => "Namespace".into(),
        4 => "Package".into(),
        5 => "Class".into(),
        6 => "Method".into(),
        7 => "Property".into(),
        8 => "Field".into(),
        9 => "Constructor".into(),
        10 => "Enum".into(),
        11 => "Interface".into(),
        12 => "Function".into(),
        13 => "Variable".into(),
        14 => "Constant".into(),
        15 => "String".into(),
        16 => "Number".into(),
        17 => "Boolean".into(),
        18 => "Array".into(),
        19 => "Object".into(),
        20 => "Key".into(),
        21 => "Null".into(),
        22 => "EnumMember".into(),
        23 => "Struct".into(),
        24 => "Event".into(),
        25 => "Operator".into(),
        26 => "TypeParameter".into(),
        _ => format!("Unknown({kind})"),
    }
}

fn completion_kind_name(kind: u64) -> String {
    match kind {
        1 => "Text".into(),
        2 => "Method".into(),
        3 => "Function".into(),
        4 => "Constructor".into(),
        5 => "Field".into(),
        6 => "Variable".into(),
        7 => "Class".into(),
        8 => "Interface".into(),
        9 => "Module".into(),
        10 => "Property".into(),
        11 => "Unit".into(),
        12 => "Value".into(),
        13 => "Enum".into(),
        14 => "Keyword".into(),
        15 => "Snippet".into(),
        16 => "Color".into(),
        17 => "File".into(),
        18 => "Reference".into(),
        19 => "Folder".into(),
        20 => "EnumMember".into(),
        21 => "Constant".into(),
        22 => "Struct".into(),
        23 => "Event".into(),
        24 => "Operator".into(),
        25 => "TypeParameter".into(),
        _ => format!("Unknown({kind})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Requires rust-analyzer to be installed on the system.
    /// Run with: cargo test -p runtime -- --ignored
    #[tokio::test]
    #[ignore = "requires rust-analyzer installed on PATH"]
    async fn spawn_and_initialize_rust_analyzer() {
        let root = std::env::current_dir().expect("should have cwd");
        let process = LspProcess::start("rust-analyzer", &[], &root).await;
        assert!(process.is_ok(), "should spawn and initialize rust-analyzer");

        let mut process = process.unwrap();
        assert_eq!(process.status(), LspServerStatus::Connected);
        assert_eq!(process.language(), "rust-analyzer");

        let shutdown_result = process.shutdown().await;
        assert!(
            shutdown_result.is_ok(),
            "shutdown should succeed: {shutdown_result:?}"
        );
    }

    /// Requires rust-analyzer to be installed and a Rust project on disk.
    /// Run with: cargo test -p runtime -- --ignored
    #[tokio::test]
    #[ignore = "requires rust-analyzer installed on PATH"]
    async fn hover_on_real_file() {
        let root = std::env::current_dir().expect("should have cwd");
        let mut process = LspProcess::start("rust-analyzer", &[], &root)
            .await
            .expect("should start rust-analyzer");

        // Try hover on src/main.rs — the result might be None if the file
        // doesn't exist at that path, but the call itself should not error.
        let file_path = root.join("src").join("main.rs");
        let path_str = file_path.to_string_lossy();
        let result = process.hover(&path_str, 0, 0).await;
        assert!(
            result.is_ok(),
            "hover should not return an error: {:?}",
            result.err()
        );

        let _ = process.shutdown().await;
    }

    #[test]
    fn parse_hover_markup_content() {
        let value = serde_json::json!({
            "contents": {
                "kind": "plaintext",
                "value": "fn main()"
            }
        });
        let result = parse_hover(&value);
        assert!(result.is_some());
        let hover = result.unwrap();
        assert_eq!(hover.content, "fn main()");
    }

    #[test]
    fn parse_hover_marked_string_object() {
        let value = serde_json::json!({
            "contents": {
                "language": "rust",
                "value": "pub fn foo()"
            }
        });
        let result = parse_hover(&value);
        assert!(result.is_some());
        let hover = result.unwrap();
        assert_eq!(hover.content, "pub fn foo()");
        assert_eq!(hover.language.as_deref(), Some("rust"));
    }

    #[test]
    fn parse_hover_plain_string() {
        let value = serde_json::json!({
            "contents": "some text"
        });
        let result = parse_hover(&value);
        assert!(result.is_some());
        let hover = result.unwrap();
        assert_eq!(hover.content, "some text");
        assert!(hover.language.is_none());
    }

    #[test]
    fn parse_hover_array_of_marked_strings() {
        let value = serde_json::json!({
            "contents": [
                "first line",
                { "language": "rust", "value": "fn bar()" }
            ]
        });
        let result = parse_hover(&value);
        assert!(result.is_some());
        let hover = result.unwrap();
        assert!(hover.content.contains("first line"));
        assert!(hover.content.contains("fn bar()"));
    }

    #[test]
    fn parse_locations_empty_array() {
        let value = serde_json::json!([]);
        let locations = parse_locations(&value);
        assert!(locations.is_empty());
    }

    #[test]
    fn parse_locations_valid() {
        let value = serde_json::json!([
            {
                "uri": "file:///tmp/test.rs",
                "range": {
                    "start": { "line": 5, "character": 10 },
                    "end": { "line": 5, "character": 15 }
                }
            }
        ]);
        let locations = parse_locations(&value);
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].line, 5);
        assert_eq!(locations[0].character, 10);
        assert_eq!(locations[0].end_line, Some(5));
        assert_eq!(locations[0].end_character, Some(15));
    }

    #[test]
    fn parse_symbols_basic() {
        let value = serde_json::json!([
            {
                "name": "main",
                "kind": 12,
                "range": {
                    "start": { "line": 1, "character": 0 },
                    "end": { "line": 5, "character": 1 }
                }
            }
        ]);
        let symbols = parse_symbols(&value, "/tmp/test.rs");
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "main");
        assert_eq!(symbols[0].kind, "Function");
        assert_eq!(symbols[0].line, 1);
    }

    #[test]
    fn parse_completions_basic() {
        let value = serde_json::json!([
            { "label": "foo", "kind": 3, "detail": "fn foo()" },
            { "label": "bar", "kind": 6 }
        ]);
        let completions = parse_completions(&value);
        assert_eq!(completions.len(), 2);
        assert_eq!(completions[0].label, "foo");
        assert_eq!(completions[0].kind.as_deref(), Some("Function"));
        assert_eq!(completions[0].detail.as_deref(), Some("fn foo()"));
        assert_eq!(completions[1].label, "bar");
        assert_eq!(completions[1].kind.as_deref(), Some("Variable"));
    }

    #[test]
    fn symbol_kind_name_all_variants() {
        assert_eq!(symbol_kind_name(1), "File");
        assert_eq!(symbol_kind_name(6), "Method");
        assert_eq!(symbol_kind_name(12), "Function");
        assert_eq!(symbol_kind_name(13), "Variable");
        assert_eq!(symbol_kind_name(23), "Struct");
        assert_eq!(symbol_kind_name(99), "Unknown(99)");
    }

    #[test]
    fn completion_kind_name_all_variants() {
        assert_eq!(completion_kind_name(1), "Text");
        assert_eq!(completion_kind_name(3), "Function");
        assert_eq!(completion_kind_name(6), "Variable");
        assert_eq!(completion_kind_name(14), "Keyword");
        assert_eq!(completion_kind_name(99), "Unknown(99)");
    }

    #[test]
    fn text_document_position_params_structure() {
        let params = text_document_position_params("file:///test.rs", 5, 10);
        assert_eq!(params["textDocument"]["uri"], "file:///test.rs");
        assert_eq!(params["position"]["line"], 5);
        assert_eq!(params["position"]["character"], 10);
    }

    #[test]
    fn path_to_uri_absolute() {
        let uri = path_to_uri("/tmp/test.rs");
        assert_eq!(uri, "file:///tmp/test.rs");
    }

    #[test]
    fn uri_to_path_extracts_path() {
        assert_eq!(uri_to_path("file:///tmp/test.rs"), "/tmp/test.rs");
        assert_eq!(uri_to_path("/no/prefix"), "/no/prefix");
    }
}
