//! WebFetchTool — fetch a URL and return its content as Markdown.
//!
//! HTML pages are converted to readable Markdown via regex-based
//! transformations.  Other content types are returned as-is.
//! Content is truncated to `MAX_CONTENT_CHARS`.
//!
//! Ref: src/tools/WebFetchTool/WebFetchTool.ts

use std::path::Path;

use async_trait::async_trait;
use code_permissions::evaluator::ToolCallContext;
use code_types::tool::{ToolInputSchema, ToolResult, ToolResultPayload, ValidationResult};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{error_result, ProgressSender, Tool, ToolContext};

const MAX_CONTENT_CHARS: usize = 100_000;

#[derive(Deserialize)]
struct WebFetchInput {
    url: String,
    #[allow(dead_code)]
    prompt: Option<String>,
    max_length: Option<usize>,
}

pub struct WebFetchTool;

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str { "WebFetch" }

    fn description(&self) -> &str {
        "Fetches content from a URL and returns it as Markdown. \
        HTML pages are converted to readable Markdown. \
        Use this to read documentation, web pages, or any URL-accessible content."
    }

    fn input_schema(&self) -> ToolInputSchema {
        json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to fetch"
                },
                "prompt": {
                    "type": "string",
                    "description": "Optional description of what you're looking for"
                },
                "max_length": {
                    "type": "number",
                    "description": "Maximum characters to return (default: 100000)"
                }
            },
            "required": ["url"]
        })
    }

    fn is_read_only(&self, _input: &Value) -> bool { true }

    async fn validate_input(&self, input: &Value, _ctx: &ToolContext) -> ValidationResult {
        let url = match input.get("url").and_then(|v| v.as_str()) {
            Some(u) if !u.is_empty() => u,
            _ => return ValidationResult::err("url is required and must not be empty", 1),
        };
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return ValidationResult::err("url must start with http:// or https://", 1);
        }
        ValidationResult::ok()
    }

    fn permission_context<'a>(&'a self, input: &'a Value, cwd: &'a Path) -> ToolCallContext<'a> {
        ToolCallContext {
            tool_name: self.name(),
            content: input.get("url").and_then(|v| v.as_str()),
            input: Some(input),
            is_read_only: true,
            cwd,
        }
    }

    async fn call(
        &self,
        tool_use_id: &str,
        input: Value,
        _ctx: &ToolContext,
        _progress: Option<&ProgressSender>,
    ) -> ToolResult {
        let parsed: WebFetchInput = match serde_json::from_value(input) {
            Ok(v) => v,
            Err(e) => return error_result(tool_use_id, format!("Invalid input: {e}")),
        };
        let max_len = parsed.max_length.unwrap_or(MAX_CONTENT_CHARS);

        let client = match reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (compatible; ClaudeCode/1.0)")
            .timeout(std::time::Duration::from_secs(30))
            .build()
        {
            Ok(c) => c,
            Err(e) => return error_result(tool_use_id, format!("HTTP client error: {e}")),
        };

        let response = match client.get(&parsed.url).send().await {
            Ok(r) => r,
            Err(e) => return error_result(tool_use_id, format!("Failed to fetch {}: {e}", parsed.url)),
        };

        let status = response.status();
        if !status.is_success() {
            return error_result(tool_use_id, format!("HTTP {} for {}", status.as_u16(), parsed.url));
        }

        let is_html = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|ct| ct.contains("text/html"))
            .unwrap_or(false);

        let body = match response.text().await {
            Ok(b) => b,
            Err(e) => return error_result(tool_use_id, format!("Failed to read response: {e}")),
        };

        let text = if is_html { html_to_markdown(&body) } else { body };

        let truncated = text.chars().count() > max_len;
        let mut output: String = text.chars().take(max_len).collect();
        if truncated {
            output.push_str(&format!("\n\n[Content truncated at {max_len} characters]"));
        }

        ToolResult {
            tool_use_id: tool_use_id.to_owned(),
            content: ToolResultPayload::Text(output),
            is_error: false,
            was_truncated: truncated,
        }
    }
}

// ── HTML → Markdown conversion ───────────────────────────────────────────────

/// Compile a regex once and cache it forever.
fn re(pat: &str) -> regex::Regex {
    regex::Regex::new(pat).unwrap()
}

/// Convert HTML to readable Markdown.
///
/// Handles the most common structural elements: headings, paragraphs, links,
/// emphasis, code blocks, lists, images, blockquotes, and tables.
fn html_to_markdown(html: &str) -> String {
    // ── Phase 1: Remove invisible content ────────────────────────────────────
    let s = re(r"(?is)<(script|style|noscript|svg|head)[^>]*>.*?</(script|style|noscript|svg|head)>")
        .replace_all(html, "");
    let s = re(r"(?s)<!--.*?-->").replace_all(&s, "");

    // ── Phase 2: Structural elements → Markdown ──────────────────────────────

    // Headings.
    let s = re(r"(?is)<h1[^>]*>(.*?)</h1>").replace_all(&s, "\n\n# $1\n\n");
    let s = re(r"(?is)<h2[^>]*>(.*?)</h2>").replace_all(&s, "\n\n## $1\n\n");
    let s = re(r"(?is)<h3[^>]*>(.*?)</h3>").replace_all(&s, "\n\n### $1\n\n");
    let s = re(r"(?is)<h4[^>]*>(.*?)</h4>").replace_all(&s, "\n\n#### $1\n\n");
    let s = re(r"(?is)<h5[^>]*>(.*?)</h5>").replace_all(&s, "\n\n##### $1\n\n");
    let s = re(r"(?is)<h6[^>]*>(.*?)</h6>").replace_all(&s, "\n\n###### $1\n\n");

    // Code blocks: <pre><code>...</code></pre> → fenced code blocks.
    let s = re(r"(?is)<pre[^>]*>\s*<code[^>]*>(.*?)</code>\s*</pre>")
        .replace_all(&s, "\n\n```\n$1\n```\n\n");
    let s = re(r"(?is)<pre[^>]*>(.*?)</pre>")
        .replace_all(&s, "\n\n```\n$1\n```\n\n");

    // Inline code: <code>...</code> → `...`.
    let s = re(r"(?is)<code[^>]*>(.*?)</code>").replace_all(&s, "`$1`");

    // Links: <a href="url">text</a> → [text](url).
    let s = re(r#"(?is)<a\s[^>]*href\s*=\s*["']([^"']+)["'][^>]*>(.*?)</a>"#)
        .replace_all(&s, "[$2]($1)");

    // Images: <img src="url" alt="text"> → ![text](url).
    let s = re(r#"(?is)<img\s[^>]*src\s*=\s*["']([^"']+)["'][^>]*?alt\s*=\s*["']([^"']*)["'][^>]*/?\s*>"#)
        .replace_all(&s, "![$2]($1)");
    let s = re(r#"(?is)<img\s[^>]*src\s*=\s*["']([^"']+)["'][^>]*/?\s*>"#)
        .replace_all(&s, "![]($1)");

    // Bold: <strong>, <b> → **text**.
    let s = re(r"(?is)<(strong|b)\b[^>]*>(.*?)</(strong|b)>").replace_all(&s, "**$2**");

    // Italic: <em>, <i> → *text*.
    let s = re(r"(?is)<(em|i)\b[^>]*>(.*?)</(em|i)>").replace_all(&s, "*$2*");

    // Strikethrough: <del>, <s> → ~~text~~.
    let s = re(r"(?is)<(del|s)\b[^>]*>(.*?)</(del|s)>").replace_all(&s, "~~$2~~");

    // Blockquote: <blockquote>...</blockquote>.
    let s = re(r"(?is)<blockquote[^>]*>(.*?)</blockquote>").replace_all(&s, |caps: &regex::Captures| {
        let inner = caps[1].trim();
        let lines: Vec<String> = inner.lines().map(|l| format!("> {}", l.trim())).collect();
        format!("\n\n{}\n\n", lines.join("\n"))
    });

    // Unordered list items: <li> inside context → "- ".
    let s = re(r"(?is)<li[^>]*>(.*?)</li>").replace_all(&s, "\n- $1");

    // List wrappers: just insert newlines around them.
    let s = re(r"(?is)<(ul|ol)\b[^>]*>(.*?)</(ul|ol)>").replace_all(&s, "\n$2\n");

    // Horizontal rules: <hr>.
    let s = re(r"(?i)<hr\s*/?\s*>").replace_all(&s, "\n\n---\n\n");

    // Line breaks: <br>.
    let s = re(r"(?i)<br\s*/?\s*>").replace_all(&s, "\n");

    // Paragraphs: add double newlines.
    let s = re(r"(?i)<p\b[^>]*>").replace_all(&s, "\n\n");
    let s = re(r"(?i)</p>").replace_all(&s, "\n\n");

    // Divs and sections: add newlines.
    let s = re(r"(?i)<(div|section|article|header|footer|main|nav)\b[^>]*>").replace_all(&s, "\n");
    let s = re(r"(?i)</(div|section|article|header|footer|main|nav)>").replace_all(&s, "\n");

    // Table cells: separate with " | ", rows with newlines.
    let s = re(r"(?is)<th[^>]*>(.*?)</th>").replace_all(&s, " **$1** |");
    let s = re(r"(?is)<td[^>]*>(.*?)</td>").replace_all(&s, " $1 |");
    let s = re(r"(?is)<tr[^>]*>(.*?)</tr>").replace_all(&s, "|$1\n");

    // Table wrapper.
    let s = re(r"(?is)<(table|thead|tbody|tfoot)\b[^>]*>").replace_all(&s, "\n");
    let s = re(r"(?is)</(table|thead|tbody|tfoot)>").replace_all(&s, "\n");

    // ── Phase 3: Strip remaining tags ────────────────────────────────────────
    let s = re(r"<[^>]+>").replace_all(&s, "");

    // ── Phase 4: Decode entities ─────────────────────────────────────────────
    let s = s
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ")
        .replace("&#x27;", "'")
        .replace("&#x2F;", "/")
        .replace("&mdash;", "\u{2014}")
        .replace("&ndash;", "\u{2013}")
        .replace("&hellip;", "\u{2026}")
        .replace("&laquo;", "\u{00AB}")
        .replace("&raquo;", "\u{00BB}");

    // Numeric entities.
    let s = re(r"&#(\d+);").replace_all(&s, |caps: &regex::Captures| {
        caps[1]
            .parse::<u32>()
            .ok()
            .and_then(char::from_u32)
            .map(|c| c.to_string())
            .unwrap_or_default()
    });
    let s = re(r"&#x([0-9a-fA-F]+);").replace_all(&s, |caps: &regex::Captures| {
        u32::from_str_radix(&caps[1], 16)
            .ok()
            .and_then(char::from_u32)
            .map(|c| c.to_string())
            .unwrap_or_default()
    });

    // ── Phase 5: Cleanup whitespace ──────────────────────────────────────────
    let s = re(r"\n{3,}").replace_all(&s, "\n\n");
    let s = re(r"[ \t]+").replace_all(&s, " ");
    // Trim leading spaces on each line.
    let s = re(r"(?m)^[ \t]+").replace_all(&s, "");

    s.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings_converted() {
        let html = "<h1>Title</h1><h2>Subtitle</h2><p>Body text</p>";
        let md = html_to_markdown(html);
        assert!(md.contains("# Title"));
        assert!(md.contains("## Subtitle"));
        assert!(md.contains("Body text"));
    }

    #[test]
    fn links_converted() {
        let html = r#"<a href="https://example.com">Click here</a>"#;
        let md = html_to_markdown(html);
        assert!(md.contains("[Click here](https://example.com)"));
    }

    #[test]
    fn code_blocks_preserved() {
        let html = "<pre><code>fn main() {}</code></pre>";
        let md = html_to_markdown(html);
        assert!(md.contains("```\nfn main() {}\n```"));
    }

    #[test]
    fn inline_code_converted() {
        let html = "Use <code>cargo build</code> to compile.";
        let md = html_to_markdown(html);
        assert!(md.contains("`cargo build`"));
    }

    #[test]
    fn bold_italic_converted() {
        let html = "<strong>Bold</strong> and <em>italic</em>";
        let md = html_to_markdown(html);
        assert!(md.contains("**Bold**"));
        assert!(md.contains("*italic*"));
    }

    #[test]
    fn list_items_converted() {
        let html = "<ul><li>Item 1</li><li>Item 2</li></ul>";
        let md = html_to_markdown(html);
        assert!(md.contains("- Item 1"));
        assert!(md.contains("- Item 2"));
    }

    #[test]
    fn scripts_removed() {
        let html = "<p>Hello</p><script>alert('x')</script><p>World</p>";
        let md = html_to_markdown(html);
        assert!(md.contains("Hello"));
        assert!(md.contains("World"));
        assert!(!md.contains("alert"));
    }

    #[test]
    fn entities_decoded() {
        let html = "Fish &amp; Chips &mdash; Good";
        let md = html_to_markdown(html);
        assert!(md.contains("Fish & Chips \u{2014} Good"));
    }

    #[test]
    fn numeric_entity_decoded() {
        let html = "&#169; 2024";
        let md = html_to_markdown(html);
        assert!(md.contains("\u{00A9} 2024"));
    }

    #[test]
    fn images_converted() {
        let html = r#"<img src="logo.png" alt="Logo">"#;
        let md = html_to_markdown(html);
        assert!(md.contains("![Logo](logo.png)"));
    }

    #[test]
    fn horizontal_rule_converted() {
        let html = "<hr />";
        let md = html_to_markdown(html);
        assert!(md.contains("---"));
    }
}
