//! Markdown → ratatui `Line` renderer using pulldown-cmark + syntect.

use std::cell::RefCell;
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd, CodeBlockKind};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

use crate::theme::Theme;

thread_local! {
    static SYNTAX_SET: RefCell<SyntaxSet> = RefCell::new(SyntaxSet::load_defaults_newlines());
    static THEME_SET: RefCell<ThemeSet> = RefCell::new(ThemeSet::load_defaults());
}

fn syntect_color_to_ratatui(c: syntect::highlighting::Color) -> Color {
    Color::Rgb(c.r, c.g, c.b)
}

fn highlight_code_block(code: &str, lang: &str) -> Vec<Line<'static>> {
    SYNTAX_SET.with(|ss| {
        THEME_SET.with(|ts| {
            let ss = ss.borrow();
            let ts = ts.borrow();
            let syntax = ss
                .find_syntax_by_token(lang)
                .unwrap_or_else(|| ss.find_syntax_plain_text());
            let theme = &ts.themes["base16-ocean.dark"];
            let mut h = HighlightLines::new(syntax, theme);
            let mut lines = Vec::new();
            for line in LinesWithEndings::from(code) {
                let ranges = h.highlight_line(line, &ss).unwrap_or_default();
                let spans: Vec<Span<'static>> = ranges
                    .into_iter()
                    .map(|(style, text)| {
                        let fg = syntect_color_to_ratatui(style.foreground);
                        let rat_style = Style::default().fg(fg);
                        Span::styled(text.trim_end_matches('\n').to_owned(), rat_style)
                    })
                    .collect();
                lines.push(Line::from(spans));
            }
            lines
        })
    })
}

pub fn render_markdown(text: &str, width: u16, theme: &Theme) -> Vec<Line<'static>> {
    let mut output: Vec<Line<'static>> = Vec::new();
    let mut current_spans: Vec<Span<'static>> = Vec::new();
    let mut style_stack: Vec<Style> = vec![Style::default().fg(theme.text)];
    let mut blockquote_depth: usize = 0;
    let mut in_code_block = false;
    let mut code_block_lang = String::new();
    let mut code_block_buf = String::new();

    let opts = Options::all();
    let parser = Parser::new_ext(text, opts);

    for event in parser {
        match event {
            Event::Start(Tag::Heading { .. }) => {
                let top = style_stack.last().copied().unwrap_or_default();
                style_stack.push(top.add_modifier(Modifier::BOLD));
            }
            Event::End(TagEnd::Heading(_)) => {
                if style_stack.len() > 1 {
                    style_stack.pop();
                }
                let line_spans: Vec<Span<'static>> = current_spans.drain(..).collect();
                output.push(Line::from(line_spans));
                output.push(Line::default());
            }
            Event::Start(Tag::Strong) => {
                let top = style_stack.last().copied().unwrap_or_default();
                style_stack.push(top.add_modifier(Modifier::BOLD));
            }
            Event::End(TagEnd::Strong) => {
                if style_stack.len() > 1 {
                    style_stack.pop();
                }
            }
            Event::Start(Tag::Emphasis) => {
                let top = style_stack.last().copied().unwrap_or_default();
                style_stack.push(top.add_modifier(Modifier::ITALIC));
            }
            Event::End(TagEnd::Emphasis) => {
                if style_stack.len() > 1 {
                    style_stack.pop();
                }
            }
            Event::Start(Tag::BlockQuote(_)) => {
                blockquote_depth += 1;
                let top = style_stack.last().copied().unwrap_or_default();
                style_stack.push(top.fg(theme.subtle));
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                if blockquote_depth > 0 {
                    blockquote_depth -= 1;
                }
                if style_stack.len() > 1 {
                    style_stack.pop();
                }
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                in_code_block = true;
                code_block_lang = match kind {
                    CodeBlockKind::Fenced(lang) => lang.to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                code_block_buf.clear();
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                let highlighted = highlight_code_block(&code_block_buf, &code_block_lang);
                output.extend(highlighted);
                output.push(Line::default());
                code_block_buf.clear();
                code_block_lang.clear();
            }
            Event::Text(t) => {
                if in_code_block {
                    code_block_buf.push_str(&t);
                } else {
                    let style = style_stack.last().copied().unwrap_or_default();
                    current_spans.push(Span::styled(t.to_string(), style));
                }
            }
            Event::Code(t) => {
                let style = Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD);
                current_spans.push(Span::styled(format!("`{}`", t), style));
            }
            Event::End(TagEnd::Paragraph) => {
                let line_spans: Vec<Span<'static>> = current_spans.drain(..).collect();
                output.push(Line::from(line_spans));
                output.push(Line::default());
            }
            Event::SoftBreak | Event::HardBreak => {
                let line_spans: Vec<Span<'static>> = current_spans.drain(..).collect();
                output.push(Line::from(line_spans));
            }
            Event::Rule => {
                let rule = "─".repeat(width as usize);
                output.push(Line::from(Span::styled(
                    rule,
                    Style::default().fg(theme.subtle),
                )));
            }
            Event::Start(Tag::List(_)) | Event::End(TagEnd::List(_)) => {}
            Event::Start(Tag::Item) => {
                let style = style_stack.last().copied().unwrap_or_default();
                current_spans.push(Span::styled("• ".to_owned(), style));
            }
            Event::End(TagEnd::Item) => {
                let line_spans: Vec<Span<'static>> = current_spans.drain(..).collect();
                output.push(Line::from(line_spans));
            }
            Event::Start(Tag::Link { .. }) => {
                let top = style_stack.last().copied().unwrap_or_default();
                style_stack.push(top.fg(Color::Cyan));
            }
            Event::End(TagEnd::Link) => {
                if style_stack.len() > 1 {
                    style_stack.pop();
                }
            }
            _ => {}
        }
    }

    // flush remaining spans
    if !current_spans.is_empty() {
        output.push(Line::from(current_spans.drain(..).collect::<Vec<_>>()));
    }

    let _ = blockquote_depth;
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::dark_theme;

    #[test]
    fn plain_text_renders_as_line() {
        let theme = dark_theme();
        let lines = render_markdown("Hello world", 80, &theme);
        assert!(!lines.is_empty());
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref().to_owned()))
            .collect();
        assert!(
            all_text.contains("Hello world"),
            "expected 'Hello world' in output, got: {:?}",
            all_text
        );
    }

    #[test]
    fn rule_fills_width_with_dashes() {
        let theme = dark_theme();
        let lines = render_markdown("---", 20, &theme);
        let found = lines.iter().any(|l| {
            let text: String = l.spans.iter().map(|s| s.content.as_ref().to_owned()).collect();
            text.chars().all(|c| c == '─') && text.chars().count() == 20
        });
        assert!(
            found,
            "expected a rule line of 20 '─' chars, got: {:?}",
            lines
        );
    }

    #[test]
    fn code_block_renders_at_least_one_line() {
        let theme = dark_theme();
        let md = "```rust\nfn main() {}\n```";
        let lines = render_markdown(md, 80, &theme);
        assert!(!lines.is_empty(), "code block should produce at least one line");
    }
}
