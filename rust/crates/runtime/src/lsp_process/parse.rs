//! Helper functions for LSP URI/path conversion, parameter building, and
//! response parsing.

use std::path::Path;

use serde_json::Value as JsonValue;

use crate::lsp_client::{LspCompletionItem, LspHoverResult, LspLocation, LspSymbol};
use crate::lsp_process::LspProcessError;

pub(super) fn canonicalize_root(path: &Path) -> Result<String, LspProcessError> {
    path.canonicalize()
        .map_err(|e| LspProcessError::InvalidPath(format!("{}: {e}", path.display())))
        .map(|p| p.to_string_lossy().into_owned())
}

pub(super) fn path_to_uri(path: &str) -> String {
    let canonical = std::path::Path::new(path);
    if canonical.is_absolute() {
        format!("file://{path}")
    } else {
        let resolved = std::env::current_dir()
            .map_or_else(|_| canonical.to_path_buf(), |d| d.join(path));
        let canonicalized = resolved
            .canonicalize()
            .unwrap_or(resolved)
            .to_string_lossy()
            .into_owned();
        format!("file://{canonicalized}")
    }
}

pub(super) fn text_document_position_params(uri: &str, line: u32, character: u32) -> JsonValue {
    serde_json::json!({
        "textDocument": { "uri": uri },
        "position": { "line": line, "character": character }
    })
}

pub(super) fn uri_to_path(uri: &str) -> String {
    uri.strip_prefix("file://").unwrap_or(uri).to_owned()
}

pub(super) fn language_id_for_path(path: &str) -> String {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    match ext {
        "rs" => "rust",
        "ts" => "typescript",
        "tsx" => "typescriptreact",
        "js" => "javascript",
        "jsx" => "javascriptreact",
        "py" => "python",
        "go" => "go",
        "java" => "java",
        "c" | "h" => "c",
        "cpp" | "hpp" | "cc" => "cpp",
        "rb" => "ruby",
        "lua" => "lua",
        _ => ext,
    }
    .to_owned()
}

pub(super) fn severity_name(code: u64) -> String {
    match code {
        1 => "error".to_owned(),
        2 => "warning".to_owned(),
        3 => "info".to_owned(),
        4 => "hint".to_owned(),
        _ => format!("unknown({code})"),
    }
}

pub(super) fn parse_hover(value: &JsonValue) -> Option<LspHoverResult> {
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
pub(super) fn parse_locations(value: &JsonValue) -> Vec<LspLocation> {
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

pub(super) fn parse_symbols(value: &JsonValue, default_path: &str) -> Vec<LspSymbol> {
    let Some(items) = value.as_array() else {
        return Vec::new();
    };

    let mut result = Vec::new();
    extract_symbols(items, default_path, &mut result);
    result
}

pub(super) fn parse_completions(value: &JsonValue) -> Vec<LspCompletionItem> {
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

pub(super) fn symbol_kind_name(kind: u64) -> String {
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

pub(super) fn completion_kind_name(kind: u64) -> String {
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
