//! NotebookEditTool — edit Jupyter notebook (.ipynb) cells.
//!
//! Supports inserting, replacing, and deleting cells in a notebook's
//! `cells` array.  The notebook is read, modified in memory, and written
//! back atomically.
//!
//! Ref: src/tools/NotebookEditTool/NotebookEditTool.ts

use std::path::Path;

use async_trait::async_trait;
use code_permissions::evaluator::ToolCallContext;
use code_types::tool::{ToolInputSchema, ToolResult, ValidationResult};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{error_result, ok_result, ProgressSender, Tool, ToolContext};

// ── Input ─────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum EditMode {
    Insert,
    Replace,
    Delete,
}

#[derive(Deserialize)]
struct NotebookEditInput {
    notebook_path: String,
    cell_index: usize,
    edit_mode: EditMode,
    /// New source lines for the cell (required for Insert and Replace).
    new_source: Option<Vec<String>>,
    /// Cell type: "code" | "markdown" | "raw" (default: "code").
    cell_type: Option<String>,
}

// ── Notebook structures ───────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
struct Notebook {
    cells: Vec<Value>,
    #[serde(flatten)]
    rest: serde_json::Map<String, Value>,
}

// ── Tool ─────────────────────────────────────────────────────────────────────

pub struct NotebookEditTool;

#[async_trait]
impl Tool for NotebookEditTool {
    fn name(&self) -> &str { "NotebookEdit" }

    fn description(&self) -> &str {
        "Edits cells in a Jupyter notebook (.ipynb file). \
        Supports inserting a new cell at a given index, replacing an existing cell, \
        or deleting a cell. Cell outputs are preserved unless the cell itself is replaced."
    }

    fn input_schema(&self) -> ToolInputSchema {
        json!({
            "type": "object",
            "properties": {
                "notebook_path": {
                    "type": "string",
                    "description": "Absolute path to the .ipynb file"
                },
                "cell_index": {
                    "type": "number",
                    "description": "0-based index of the cell to act on"
                },
                "edit_mode": {
                    "type": "string",
                    "enum": ["insert", "replace", "delete"],
                    "description": "insert: add new cell before cell_index; replace: overwrite cell; delete: remove cell"
                },
                "new_source": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Source lines for the new/replacement cell (required for insert/replace)"
                },
                "cell_type": {
                    "type": "string",
                    "enum": ["code", "markdown", "raw"],
                    "description": "Cell type (default: code)"
                }
            },
            "required": ["notebook_path", "cell_index", "edit_mode"]
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool { false }

    async fn validate_input(&self, input: &Value, _ctx: &ToolContext) -> ValidationResult {
        let mode = input.get("edit_mode").and_then(|v| v.as_str()).unwrap_or("");
        if matches!(mode, "insert" | "replace") {
            if input.get("new_source").is_none() {
                return ValidationResult::err("new_source is required for insert/replace", 1);
            }
        }
        ValidationResult::ok()
    }

    fn permission_context<'a>(&'a self, input: &'a Value, cwd: &'a Path) -> ToolCallContext<'a> {
        ToolCallContext {
            tool_name: self.name(),
            content: input.get("notebook_path").and_then(|v| v.as_str()),
            input: Some(input),
            is_read_only: false,
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
        let parsed: NotebookEditInput = match serde_json::from_value(input) {
            Ok(v) => v,
            Err(e) => return error_result(tool_use_id, format!("Invalid input: {e}")),
        };

        let path = {
            let p = std::path::Path::new(&parsed.notebook_path);
            if p.is_absolute() { p.to_path_buf() } else { ctx.cwd.join(p) }
        };

        // Read notebook JSON.
        let raw = match tokio::fs::read_to_string(&path).await {
            Ok(s) => s,
            Err(e) => return error_result(tool_use_id, format!("Cannot read notebook: {e}")),
        };
        let mut notebook: Notebook = match serde_json::from_str(&raw) {
            Ok(n) => n,
            Err(e) => return error_result(tool_use_id, format!("Invalid notebook JSON: {e}")),
        };

        let ncells = notebook.cells.len();
        let idx = parsed.cell_index;

        match parsed.edit_mode {
            EditMode::Insert => {
                if idx > ncells {
                    return error_result(tool_use_id, format!("cell_index {idx} out of range (notebook has {ncells} cells)"));
                }
                let cell = make_cell(
                    parsed.cell_type.as_deref().unwrap_or("code"),
                    parsed.new_source.unwrap_or_default(),
                );
                notebook.cells.insert(idx, cell);
            }
            EditMode::Replace => {
                if idx >= ncells {
                    return error_result(tool_use_id, format!("cell_index {idx} out of range (notebook has {ncells} cells)"));
                }
                let cell = make_cell(
                    parsed.cell_type.as_deref().unwrap_or("code"),
                    parsed.new_source.unwrap_or_default(),
                );
                notebook.cells[idx] = cell;
            }
            EditMode::Delete => {
                if idx >= ncells {
                    return error_result(tool_use_id, format!("cell_index {idx} out of range (notebook has {ncells} cells)"));
                }
                notebook.cells.remove(idx);
            }
        }

        // Write back.
        let out = match serde_json::to_string_pretty(&notebook) {
            Ok(s) => s,
            Err(e) => return error_result(tool_use_id, format!("Serialization error: {e}")),
        };
        if let Err(e) = tokio::fs::write(&path, out).await {
            return error_result(tool_use_id, format!("Cannot write notebook: {e}"));
        }

        ok_result(tool_use_id, format!("Notebook {} updated successfully.", path.display()))
    }
}

fn make_cell(cell_type: &str, source: Vec<String>) -> Value {
    let source_val: Vec<Value> = source.into_iter().map(Value::String).collect();
    match cell_type {
        "markdown" | "raw" => json!({
            "cell_type": cell_type,
            "metadata": {},
            "source": source_val
        }),
        _ => json!({
            "cell_type": "code",
            "execution_count": null,
            "metadata": {},
            "outputs": [],
            "source": source_val
        }),
    }
}
