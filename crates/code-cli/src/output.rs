//! Non-interactive output mode: run a single query, print result, exit.
//!
//! Ref: src/main.tsx print/command handling

use std::sync::Arc;

use anyhow::Result;
use uuid::Uuid;

use code_commands::{CommandOutput, CommandRegistry};
use code_config::settings::SettingsJson;
use code_query::engine::QueryEngine;
use code_types::message::{ContentBlock, Message, TextBlock, UserMessage};

use crate::args::Cli;

/// Non-interactive mode: run a single query, print result to stdout, then exit.
///
/// When `--print` is given with a positional prompt, or `--command` is given,
/// we run exactly one query (or slash command) and print the output.
pub async fn run_non_interactive(
    cli: Cli,
    engine: Arc<QueryEngine>,
    registry: CommandRegistry,
    _settings: Arc<SettingsJson>,
) -> Result<()> {
    // Determine the input text (command takes precedence over prompt).
    let input = cli
        .command
        .or(cli.prompt)
        .unwrap_or_default();

    let input = input.trim().to_owned();
    if input.is_empty() {
        // Nothing to do.
        return Ok(());
    }

    // Slash commands are dispatched via the registry.
    if input.starts_with('/') {
        return run_slash_command(&input, registry, engine, &cli.output_format).await;
    }

    run_query(&input, engine, &cli.output_format).await
}

// ── Slash command ─────────────────────────────────────────────────────────────

async fn run_slash_command(
    raw: &str,
    registry: CommandRegistry,
    engine: Arc<QueryEngine>,
    output_format: &str,
) -> Result<()> {
    use std::sync::Arc;
    use tokio::sync::RwLock;

    let (name, args) = parse_slash(raw);

    let cmd_arc = registry
        .all()
        .iter()
        .find(|c| c.name() == name || c.aliases().contains(&name))
        .cloned();

    let Some(cmd) = cmd_arc else {
        eprintln!("Unknown command: /{name}");
        std::process::exit(1);
    };

    let session_id = code_types::ids::SessionId::new();
    let cwd = std::env::current_dir().unwrap_or_default();
    let config = Arc::new(code_config::global::GlobalConfig::default());
    let settings = Arc::new(code_config::settings::SettingsJson::default());
    let conversation_lock = Arc::new(RwLock::new(Vec::new()));

    let mut ctx = code_commands::CommandContext::new(
        session_id,
        cwd,
        config,
        settings,
        vec![],
        conversation_lock,
        false,
    );

    match cmd.execute(args, &mut ctx).await? {
        CommandOutput::Text(t) | CommandOutput::Markdown(t) => {
            print_text(&t, output_format);
        }
        CommandOutput::Query(q) => {
            run_query(&q, engine, output_format).await?;
        }
        CommandOutput::Exit => {}
        CommandOutput::None | CommandOutput::Compact { .. } => {}
    }

    Ok(())
}

// ── Query execution ───────────────────────────────────────────────────────────

async fn run_query(
    prompt: &str,
    engine: Arc<QueryEngine>,
    output_format: &str,
) -> Result<()> {
    let user_msg = UserMessage {
        uuid: Uuid::new_v4(),
        content: vec![ContentBlock::Text(TextBlock {
            text: prompt.to_owned(),
            cache_control: None,
        })],
        is_api_error_message: false,
        agent_id: None,
    };

    let mut conversation = Vec::new();

    // Subscribe before querying so we don't miss any messages.
    let mut rx = engine.subscribe();

    // Spawn the query in a local task because QueryEngine::query is !Send.
    let engine2 = Arc::clone(&engine);
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            tokio::task::spawn_local(async move {
                let _ = engine2.query(user_msg, &mut conversation).await;
            })
            .await
            .ok()
        })
        .await;

    // Drain whatever was published to the channel before the task completed.
    let mut output_parts: Vec<String> = Vec::new();
    while let Some(msg) = rx.try_recv() {
        collect_message(msg, output_format, &mut output_parts);
    }

    // Wait a short time for any lingering messages then drain again.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    while let Some(msg) = rx.try_recv() {
        collect_message(msg, output_format, &mut output_parts);
    }

    if output_format == "json" {
        let arr: Vec<serde_json::Value> = output_parts
            .iter()
            .map(|s| serde_json::json!({ "type": "text", "text": s }))
            .collect();
        println!("{}", serde_json::to_string_pretty(&arr)?);
    } else {
        for part in &output_parts {
            print!("{part}");
        }
        if !output_parts.is_empty() {
            println!();
        }
    }

    Ok(())
}

fn collect_message(msg: Message, output_format: &str, parts: &mut Vec<String>) {
    match msg {
        Message::Assistant(a) => {
            for block in &a.content {
                match block {
                    ContentBlock::Text(t) => {
                        if output_format == "stream-json" {
                            println!(
                                "{}",
                                serde_json::json!({ "type": "assistant", "text": t.text })
                            );
                        } else {
                            parts.push(t.text.clone());
                        }
                    }
                    _ => {}
                }
            }
        }
        Message::SystemApiError(e) => {
            eprintln!("API error: {}", e.error);
        }
        Message::SystemTurnDuration(d) => {
            if output_format == "stream-json" {
                println!(
                    "{}",
                    serde_json::json!({
                        "type": "system",
                        "subtype": "result",
                        "duration_ms": d.duration_ms,
                        "cost_usd": d.cost_usd,
                    })
                );
            }
        }
        _ => {}
    }
}

fn print_text(text: &str, output_format: &str) {
    if output_format == "json" {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!([{"type": "text", "text": text}]))
                .unwrap_or_default()
        );
    } else {
        println!("{text}");
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn parse_slash(raw: &str) -> (&str, &str) {
    let stripped = raw.trim_start_matches('/');
    if let Some(space) = stripped.find(' ') {
        (&stripped[..space], stripped[space + 1..].trim_start())
    } else {
        (stripped, "")
    }
}
