//! LSP tools — language-server integration for hover and go-to-definition.
//!
//! Uses a lightweight JSON-RPC client over stdio to communicate with external
//! language servers.  The language server binary is auto-detected based on
//! the file extension.
//!
//! Supported servers:
//! - Rust: `rust-analyzer`
//! - TypeScript/JavaScript: `typescript-language-server`
//! - Python: `pyright-langserver` / `pylsp`
//! - Go: `gopls`
//!
//! If the required server is not found in `PATH`, the tool returns a helpful
//! error message.
//!
//! Ref: src/tools/LSPTool/LSPTool.ts

use std::path::{Path, PathBuf};
use std::process::Stdio;

use async_trait::async_trait;
use code_permissions::evaluator::ToolCallContext;
use code_types::tool::{ToolInputSchema, ToolResult, ValidationResult};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{error_result, ok_result, ProgressSender, Tool, ToolContext};

// ── Language server configuration ────────────────────────────────────────────

struct LspServerConfig {
    /// Command to start the server.
    command: &'static str,
    /// Arguments to pass.
    args: &'static [&'static str],
    /// Language ID for textDocument/didOpen.
    language_id: &'static str,
}

fn server_for_extension(ext: &str) -> Option<LspServerConfig> {
    match ext {
        "rs" => Some(LspServerConfig {
            command: "rust-analyzer",
            args: &[],
            language_id: "rust",
        }),
        "ts" | "tsx" | "js" | "jsx" => Some(LspServerConfig {
            command: "typescript-language-server",
            args: &["--stdio"],
            language_id: if ext.starts_with("ts") { "typescript" } else { "javascript" },
        }),
        "py" | "pyi" => Some(LspServerConfig {
            command: "pyright-langserver",
            args: &["--stdio"],
            language_id: "python",
        }),
        "go" => Some(LspServerConfig {
            command: "gopls",
            args: &["serve"],
            language_id: "go",
        }),
        "c" | "h" => Some(LspServerConfig {
            command: "clangd",
            args: &[],
            language_id: "c",
        }),
        "cpp" | "cxx" | "cc" | "hpp" | "hxx" => Some(LspServerConfig {
            command: "clangd",
            args: &[],
            language_id: "cpp",
        }),
        "java" => Some(LspServerConfig {
            command: "jdtls",
            args: &[],
            language_id: "java",
        }),
        _ => None,
    }
}

/// Supported file extensions.
const SUPPORTED_EXTENSIONS: &[&str] = &[
    "rs", "ts", "tsx", "js", "jsx", "py", "pyi", "go",
    "c", "h", "cpp", "cxx", "cc", "hpp", "hxx", "java",
];

// ── JSON-RPC types ──────────────────────────────────────────────────────────

#[derive(Serialize)]
struct JsonRpcRequest {
    jsonrpc: &'static str,
    id: u64,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
}

#[derive(Deserialize)]
struct JsonRpcResponse {
    #[allow(dead_code)]
    id: Option<u64>,
    result: Option<Value>,
    error: Option<JsonRpcError>,
}

#[derive(Deserialize)]
struct JsonRpcError {
    #[allow(dead_code)]
    code: i64,
    message: String,
}

// ── LSP Client ──────────────────────────────────────────────────────────────

/// A lightweight, single-shot LSP client.
///
/// Starts a language server, initializes it, opens a document, performs one
/// request (hover or definition), then shuts down.
struct LspClient {
    child: tokio::process::Child,
    stdin: tokio::process::ChildStdin,
    stdout: tokio::io::BufReader<tokio::process::ChildStdout>,
    next_id: u64,
}

impl LspClient {
    /// Start a new LSP client for the given file.
    async fn start(
        file_path: &Path,
        cwd: &Path,
    ) -> anyhow::Result<(Self, LspServerConfig)> {
        let ext = file_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let config = server_for_extension(ext)
            .ok_or_else(|| anyhow::anyhow!(
                "No language server configured for .{ext} files. \
                 Supported: {}",
                SUPPORTED_EXTENSIONS.join(", ")
            ))?;

        // Check if the server binary is available.
        if which::which(config.command).is_err() {
            anyhow::bail!(
                "Language server '{}' not found in PATH. \
                 Install it to enable LSP features for {} files.",
                config.command,
                config.language_id,
            );
        }

        let mut cmd = tokio::process::Command::new(config.command);
        cmd.args(config.args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        let mut child = cmd.spawn()
            .map_err(|e| anyhow::anyhow!("Failed to start {}: {e}", config.command))?;

        let stdin = child.stdin.take()
            .ok_or_else(|| anyhow::anyhow!("Failed to capture stdin of language server"))?;

        let stdout = child.stdout.take()
            .ok_or_else(|| anyhow::anyhow!("Failed to capture stdout of language server"))?;
        let stdout = tokio::io::BufReader::new(stdout);

        Ok((
            Self {
                child,
                stdin,
                stdout,
                next_id: 1,
            },
            config,
        ))
    }

    /// Send a JSON-RPC request and read the response.
    async fn request(&mut self, method: &str, params: Option<Value>) -> anyhow::Result<Value> {
        let id = self.next_id;
        self.next_id += 1;

        let req = JsonRpcRequest {
            jsonrpc: "2.0",
            id,
            method: method.to_owned(),
            params,
        };

        let body = serde_json::to_string(&req)?;
        self.write_message(&body).await?;

        // Read response with timeout.
        let response = tokio::time::timeout(
            std::time::Duration::from_secs(15),
            self.read_response(),
        )
        .await
        .map_err(|_| anyhow::anyhow!("LSP request timed out after 15s"))??;

        if let Some(error) = response.error {
            anyhow::bail!("LSP error: {}", error.message);
        }

        Ok(response.result.unwrap_or(Value::Null))
    }

    /// Send a JSON-RPC notification (no response expected).
    async fn notify(&mut self, method: &str, params: Option<Value>) -> anyhow::Result<()> {
        let body = if let Some(params) = params {
            serde_json::to_string(&json!({
                "jsonrpc": "2.0",
                "method": method,
                "params": params,
            }))?
        } else {
            serde_json::to_string(&json!({
                "jsonrpc": "2.0",
                "method": method,
            }))?
        };

        self.write_message(&body).await
    }

    /// Write an LSP message to the server's stdin.
    async fn write_message(&mut self, body: &str) -> anyhow::Result<()> {
        use tokio::io::AsyncWriteExt;
        let header = format!("Content-Length: {}\r\n\r\n", body.len());
        self.stdin.write_all(header.as_bytes()).await?;
        self.stdin.write_all(body.as_bytes()).await?;
        self.stdin.flush().await?;
        Ok(())
    }

    /// Read a single JSON-RPC response from stdout (async).
    async fn read_response(&mut self) -> anyhow::Result<JsonRpcResponse> {
        use tokio::io::{AsyncBufReadExt, AsyncReadExt};

        // Read messages, skipping notifications (which have no `id`).
        loop {
            let mut content_length: Option<usize> = None;

            // Read headers line by line.
            loop {
                let mut line = String::new();
                self.stdout.read_line(&mut line).await?;
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    break; // End of headers.
                }
                if let Some(len_str) = trimmed.strip_prefix("Content-Length: ") {
                    content_length = len_str.trim().parse().ok();
                }
            }

            let len = content_length
                .ok_or_else(|| anyhow::anyhow!("Missing Content-Length header in LSP response"))?;

            // Read body.
            let mut body = vec![0u8; len];
            self.stdout.read_exact(&mut body).await?;

            let body_str = String::from_utf8(body)?;

            // Try to parse as a response (has "id" field).
            if let Ok(response) = serde_json::from_str::<JsonRpcResponse>(&body_str) {
                if response.id.is_some() {
                    return Ok(response);
                }
            }
            // If it's a notification (no id), skip and read the next message.
        }
    }

    /// Initialize the server, open a document, and prepare for queries.
    async fn initialize_and_open(
        &mut self,
        cwd: &Path,
        file_path: &Path,
        language_id: &str,
    ) -> anyhow::Result<()> {
        let root_uri = format!("file://{}", cwd.to_string_lossy().replace('\\', "/"));

        // Initialize.
        let _init_result = self.request("initialize", Some(json!({
            "processId": std::process::id(),
            "rootUri": root_uri,
            "capabilities": {
                "textDocument": {
                    "hover": { "contentFormat": ["markdown", "plaintext"] },
                    "definition": {},
                }
            },
        })))
        .await?;

        // Send initialized notification.
        self.notify("initialized", Some(json!({}))).await?;

        // Open the document.
        let file_content = tokio::fs::read_to_string(file_path).await
            .map_err(|e| anyhow::anyhow!("Cannot read {}: {e}", file_path.display()))?;

        let doc_uri = format!("file://{}", file_path.to_string_lossy().replace('\\', "/"));
        self.notify("textDocument/didOpen", Some(json!({
            "textDocument": {
                "uri": doc_uri,
                "languageId": language_id,
                "version": 1,
                "text": file_content,
            }
        })))
        .await?;

        // Give the server a moment to process.
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        Ok(())
    }

    /// Shut down the server gracefully.
    async fn shutdown(mut self) {
        let _ = self.request("shutdown", None).await;
        let _ = self.notify("exit", None).await;
        let _ = self.child.kill().await;
    }
}

// ── Shared parsing ──────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct LspInput {
    file_path: String,
    line: u64,
    character: u64,
}

fn validate_lsp_input(input: &Value) -> ValidationResult {
    let file_path = match input.get("file_path").and_then(|v| v.as_str()) {
        Some(p) if !p.is_empty() => p,
        _ => return ValidationResult::err("file_path is required", 1),
    };

    if input.get("line").and_then(|v| v.as_u64()).is_none() {
        return ValidationResult::err("line is required (0-based)", 1);
    }
    if input.get("character").and_then(|v| v.as_u64()).is_none() {
        return ValidationResult::err("character is required (0-based)", 1);
    }

    // Check file extension is supported.
    let ext = Path::new(file_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    if !SUPPORTED_EXTENSIONS.contains(&ext) {
        return ValidationResult::err(
            &format!(
                "Unsupported file type '.{ext}'. Supported: {}",
                SUPPORTED_EXTENSIONS.join(", ")
            ),
            1,
        );
    }

    ValidationResult::ok()
}

/// Format hover contents from an LSP hover response.
fn format_hover_result(hover: &Value) -> String {
    if hover.is_null() {
        return "No hover information available at this position.".to_owned();
    }

    let contents = &hover["contents"];

    // MarkupContent: { kind, value }
    if let Some(value) = contents.get("value").and_then(|v| v.as_str()) {
        return value.to_owned();
    }

    // String content.
    if let Some(s) = contents.as_str() {
        return s.to_owned();
    }

    // Array of MarkedString.
    if let Some(arr) = contents.as_array() {
        let parts: Vec<String> = arr
            .iter()
            .map(|item| {
                if let Some(s) = item.as_str() {
                    s.to_owned()
                } else if let Some(val) = item.get("value").and_then(|v| v.as_str()) {
                    let lang = item.get("language").and_then(|l| l.as_str()).unwrap_or("");
                    if lang.is_empty() {
                        val.to_owned()
                    } else {
                        format!("```{lang}\n{val}\n```")
                    }
                } else {
                    String::new()
                }
            })
            .filter(|s| !s.is_empty())
            .collect();
        return parts.join("\n\n");
    }

    format!("{}", serde_json::to_string_pretty(contents).unwrap_or_default())
}

/// Format definition locations from an LSP definition response.
fn format_definition_result(definition: &Value) -> String {
    if definition.is_null() {
        return "No definition found at this position.".to_owned();
    }

    let locations = if definition.is_array() {
        definition.as_array().unwrap().clone()
    } else {
        vec![definition.clone()]
    };

    if locations.is_empty() {
        return "No definition found at this position.".to_owned();
    }

    let mut parts = Vec::new();
    for loc in &locations {
        let uri = loc
            .get("uri")
            .or_else(|| loc.get("targetUri"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let range = loc
            .get("range")
            .or_else(|| loc.get("targetRange"));

        let file_path = uri
            .strip_prefix("file:///")
            .or_else(|| uri.strip_prefix("file://"))
            .unwrap_or(uri);

        if let Some(range) = range {
            let start_line = range["start"]["line"].as_u64().unwrap_or(0) + 1;
            let start_char = range["start"]["character"].as_u64().unwrap_or(0) + 1;
            parts.push(format!("{file_path}:{start_line}:{start_char}"));
        } else {
            parts.push(file_path.to_owned());
        }
    }

    if parts.len() == 1 {
        format!("Definition: {}", parts[0])
    } else {
        format!(
            "Definitions ({} locations):\n{}",
            parts.len(),
            parts.iter().map(|p| format!("  - {p}")).collect::<Vec<_>>().join("\n")
        )
    }
}

// ── LspHoverTool ────────────────────────────────────────────────────────────

pub struct LspHoverTool;

#[async_trait]
impl Tool for LspHoverTool {
    fn name(&self) -> &str { "LspHover" }

    fn description(&self) -> &str {
        "Get hover information (type, documentation) for a symbol at a specific \
        position in a source file via the Language Server Protocol. \
        Supports Rust, TypeScript, JavaScript, Python, Go, C/C++, and Java."
    }

    fn input_schema(&self) -> ToolInputSchema {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Absolute path to the source file"
                },
                "line": {
                    "type": "number",
                    "description": "0-based line number"
                },
                "character": {
                    "type": "number",
                    "description": "0-based character offset on the line"
                }
            },
            "required": ["file_path", "line", "character"]
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool { true }

    fn is_enabled(&self) -> bool {
        // Enable if at least one common language server is available.
        which::which("rust-analyzer").is_ok()
            || which::which("typescript-language-server").is_ok()
            || which::which("pyright-langserver").is_ok()
            || which::which("gopls").is_ok()
            || which::which("clangd").is_ok()
    }

    async fn validate_input(&self, input: &Value, _ctx: &ToolContext) -> ValidationResult {
        validate_lsp_input(input)
    }

    fn permission_context<'a>(&'a self, input: &'a Value, cwd: &'a Path) -> ToolCallContext<'a> {
        ToolCallContext {
            tool_name: self.name(),
            content: input.get("file_path").and_then(|v| v.as_str()),
            input: Some(input),
            is_read_only: true,
            cwd,
        }
    }

    async fn call(
        &self,
        tool_use_id: &str,
        input: Value,
        ctx: &ToolContext,
        _progress: Option<&ProgressSender>,
    ) -> ToolResult {
        let parsed: LspInput = match serde_json::from_value(input) {
            Ok(v) => v,
            Err(e) => return error_result(tool_use_id, format!("Invalid input: {e}")),
        };

        let file_path = PathBuf::from(&parsed.file_path);

        // Start the language server.
        let (mut client, config) = match LspClient::start(&file_path, &ctx.cwd).await {
            Ok(pair) => pair,
            Err(e) => return error_result(tool_use_id, format!("{e}")),
        };

        // Initialize and open the file.
        if let Err(e) = client.initialize_and_open(&ctx.cwd, &file_path, config.language_id).await {
            client.shutdown().await;
            return error_result(tool_use_id, format!("LSP initialization failed: {e}"));
        }

        // Send hover request.
        let doc_uri = format!("file://{}", file_path.to_string_lossy().replace('\\', "/"));
        let hover_result = client
            .request(
                "textDocument/hover",
                Some(json!({
                    "textDocument": { "uri": doc_uri },
                    "position": { "line": parsed.line, "character": parsed.character }
                })),
            )
            .await;

        client.shutdown().await;

        match hover_result {
            Ok(result) => ok_result(tool_use_id, format_hover_result(&result)),
            Err(e) => error_result(tool_use_id, format!("LSP hover failed: {e}")),
        }
    }
}

// ── LspDefinitionTool ───────────────────────────────────────────────────────

pub struct LspDefinitionTool;

#[async_trait]
impl Tool for LspDefinitionTool {
    fn name(&self) -> &str { "LspDefinition" }

    fn description(&self) -> &str {
        "Jump to the definition of a symbol at a specific position in a source file. \
        Returns the file path and line number of the definition. \
        Supports Rust, TypeScript, JavaScript, Python, Go, C/C++, and Java."
    }

    fn input_schema(&self) -> ToolInputSchema {
        json!({
            "type": "object",
            "properties": {
                "file_path": { "type": "string", "description": "Absolute path to the source file" },
                "line": { "type": "number", "description": "0-based line number" },
                "character": { "type": "number", "description": "0-based character offset" }
            },
            "required": ["file_path", "line", "character"]
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool { true }

    fn is_enabled(&self) -> bool {
        which::which("rust-analyzer").is_ok()
            || which::which("typescript-language-server").is_ok()
            || which::which("pyright-langserver").is_ok()
            || which::which("gopls").is_ok()
            || which::which("clangd").is_ok()
    }

    async fn validate_input(&self, input: &Value, _ctx: &ToolContext) -> ValidationResult {
        validate_lsp_input(input)
    }

    fn permission_context<'a>(&'a self, input: &'a Value, cwd: &'a Path) -> ToolCallContext<'a> {
        ToolCallContext {
            tool_name: self.name(),
            content: input.get("file_path").and_then(|v| v.as_str()),
            input: Some(input),
            is_read_only: true,
            cwd,
        }
    }

    async fn call(
        &self,
        tool_use_id: &str,
        input: Value,
        ctx: &ToolContext,
        _progress: Option<&ProgressSender>,
    ) -> ToolResult {
        let parsed: LspInput = match serde_json::from_value(input) {
            Ok(v) => v,
            Err(e) => return error_result(tool_use_id, format!("Invalid input: {e}")),
        };

        let file_path = PathBuf::from(&parsed.file_path);

        let (mut client, config) = match LspClient::start(&file_path, &ctx.cwd).await {
            Ok(pair) => pair,
            Err(e) => return error_result(tool_use_id, format!("{e}")),
        };

        if let Err(e) = client.initialize_and_open(&ctx.cwd, &file_path, config.language_id).await {
            client.shutdown().await;
            return error_result(tool_use_id, format!("LSP initialization failed: {e}"));
        }

        let doc_uri = format!("file://{}", file_path.to_string_lossy().replace('\\', "/"));
        let def_result = client
            .request(
                "textDocument/definition",
                Some(json!({
                    "textDocument": { "uri": doc_uri },
                    "position": { "line": parsed.line, "character": parsed.character }
                })),
            )
            .await;

        client.shutdown().await;

        match def_result {
            Ok(result) => ok_result(tool_use_id, format_definition_result(&result)),
            Err(e) => error_result(tool_use_id, format!("LSP definition failed: {e}")),
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_config_for_rust() {
        let config = server_for_extension("rs").unwrap();
        assert_eq!(config.command, "rust-analyzer");
        assert_eq!(config.language_id, "rust");
    }

    #[test]
    fn server_config_for_typescript() {
        let config = server_for_extension("ts").unwrap();
        assert_eq!(config.command, "typescript-language-server");
        assert_eq!(config.language_id, "typescript");
    }

    #[test]
    fn server_config_for_python() {
        let config = server_for_extension("py").unwrap();
        assert_eq!(config.command, "pyright-langserver");
        assert_eq!(config.language_id, "python");
    }

    #[test]
    fn server_config_for_unknown_returns_none() {
        assert!(server_for_extension("xyz").is_none());
        assert!(server_for_extension("").is_none());
    }

    #[test]
    fn format_hover_null() {
        assert_eq!(
            format_hover_result(&Value::Null),
            "No hover information available at this position."
        );
    }

    #[test]
    fn format_hover_markup_content() {
        let hover = json!({
            "contents": {
                "kind": "markdown",
                "value": "```rust\nfn main() {}\n```\nEntry point"
            }
        });
        let result = format_hover_result(&hover);
        assert!(result.contains("fn main()"));
        assert!(result.contains("Entry point"));
    }

    #[test]
    fn format_hover_string_content() {
        let hover = json!({ "contents": "A simple string hover" });
        assert_eq!(format_hover_result(&hover), "A simple string hover");
    }

    #[test]
    fn format_hover_array_content() {
        let hover = json!({
            "contents": [
                { "language": "rust", "value": "fn foo() -> i32" },
                "Returns a number"
            ]
        });
        let result = format_hover_result(&hover);
        assert!(result.contains("```rust\nfn foo() -> i32\n```"));
        assert!(result.contains("Returns a number"));
    }

    #[test]
    fn format_definition_null() {
        assert_eq!(
            format_definition_result(&Value::Null),
            "No definition found at this position."
        );
    }

    #[test]
    fn format_definition_single() {
        let def = json!({
            "uri": "file:///home/user/project/src/main.rs",
            "range": {
                "start": { "line": 10, "character": 4 },
                "end": { "line": 10, "character": 12 }
            }
        });
        let result = format_definition_result(&def);
        assert!(result.contains("home/user/project/src/main.rs:11:5"));
    }

    #[test]
    fn format_definition_multiple() {
        let def = json!([
            {
                "uri": "file:///src/a.rs",
                "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 5 } }
            },
            {
                "uri": "file:///src/b.rs",
                "range": { "start": { "line": 5, "character": 2 }, "end": { "line": 5, "character": 10 } }
            }
        ]);
        let result = format_definition_result(&def);
        assert!(result.contains("2 locations"));
        assert!(result.contains("src/a.rs:1:1"));
        assert!(result.contains("src/b.rs:6:3"));
    }

    #[test]
    fn validate_input_ok() {
        let input = json!({ "file_path": "/foo/bar.rs", "line": 10, "character": 5 });
        assert!(validate_lsp_input(&input).is_ok());
    }

    #[test]
    fn validate_input_missing_file() {
        let input = json!({ "line": 10, "character": 5 });
        assert!(!validate_lsp_input(&input).is_ok());
    }

    #[test]
    fn validate_input_unsupported_extension() {
        let input = json!({ "file_path": "/foo/bar.txt", "line": 0, "character": 0 });
        assert!(!validate_lsp_input(&input).is_ok());
    }

    #[test]
    fn sanitize_file_uri() {
        // On Windows, paths use backslashes — the LSP URI should use forward slashes.
        let path = Path::new("C:\\Users\\test\\project\\src\\main.rs");
        let uri = format!("file://{}", path.to_string_lossy().replace('\\', "/"));
        assert_eq!(uri, "file://C:/Users/test/project/src/main.rs");
    }
}
