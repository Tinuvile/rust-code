//! Full bootstrap sequence: config → auth → engine → dispatch.
//!
//! Ref: src/main.tsx, src/entrypoints/init.ts

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};

use code_api::client::{AnthropicClient, ClientConfig};
use code_auth::api_key::get_api_key;
use code_auth::secure_storage::create_secure_storage;
use code_commands::CommandRegistry;
use code_config::global::{load_global_config, GlobalConfig};
use code_config::project::{load_project_settings, load_project_local_settings};
use code_config::settings::{merge_settings, SettingsJson};
use code_query::engine::{QueryEngine, QueryEngineConfig};
use code_tools::registry::ToolRegistry;
use code_types::ids::SessionId;
use code_types::permissions::{PermissionMode, ToolPermissionContext};

use crate::args::Cli;

// ── Public entry point ────────────────────────────────────────────────────────

pub async fn run(cli: Cli) -> Result<()> {
    match &cli.subcommand {
        Some(crate::args::Subcommand::Mcp(_)) => {
            return crate::mcp_server::serve().await;
        }
        None => {}
    }

    // ── Phase 1: Load config + settings (parallel) ────────────────────────────
    let cwd = std::env::current_dir().context("cannot determine current directory")?;

    let (global_config, project_settings, local_settings) = tokio::join!(
        load_global_config(),
        load_project_settings(&cwd),
        load_project_local_settings(&cwd),
    );

    let global_config = Arc::new(global_config.unwrap_or_default());

    // Merge: project.json → project.local.json (local wins)
    let base_settings = project_settings.unwrap_or_default().unwrap_or_default();
    let local_s = local_settings.unwrap_or_default().unwrap_or_default();
    let mut merged_settings = merge_settings(base_settings, local_s);

    // CLI overrides take highest precedence.
    if let Some(model) = &cli.model {
        merged_settings.model = Some(model.clone());
    }

    let settings = Arc::new(merged_settings);

    // ── Phase 2: Authentication ───────────────────────────────────────────────
    let storage = create_secure_storage();
    let api_key_info = get_api_key(&global_config, &settings, storage.as_ref()).await;

    let client = match api_key_info {
        Some(info) => {
            tracing::debug!("API key resolved from {:?}", info.source);
            AnthropicClient::new(ClientConfig::from_api_key(info.key))
        }
        None => {
            // No key found — still build a client (will fail on first API call).
            // The user will see an auth error at query time.
            tracing::warn!("No API key found. Set ANTHROPIC_API_KEY or run /login.");
            AnthropicClient::new(ClientConfig::from_api_key(String::new()))
        }
    };

    // ── Phase 3: Session setup ────────────────────────────────────────────────
    let session_id = if let Some(sid_str) = &cli.session_id {
        if let Ok(uuid) = sid_str.parse() {
            code_types::ids::SessionId::from_uuid(uuid)
        } else {
            SessionId::new()
        }
    } else {
        SessionId::new()
    };

    let session_dir = session_dir_for(&session_id)?;
    tokio::fs::create_dir_all(&session_dir).await?;

    // ── Phase 4: Permission context ───────────────────────────────────────────
    let permission_ctx = build_permission_ctx(&cli)?;

    // ── Phase 5: Tool registry ────────────────────────────────────────────────
    let _tool_registry = ToolRegistry::with_default_tools(&cwd);

    // ── Phase 6: Command registry ─────────────────────────────────────────────
    let command_registry = CommandRegistry::with_all_commands();

    // ── Phase 7: Model resolution ─────────────────────────────────────────────
    let model = settings
        .model
        .clone()
        .unwrap_or_else(|| "claude-sonnet-4-6".to_owned());

    // ── Phase 8: Query engine ─────────────────────────────────────────────────
    let engine_config = QueryEngineConfig {
        model,
        cwd: cwd.clone(),
        session_id: session_id.to_string(),
        session_dir,
        permission_ctx,
        system_appendix: cli.append_system_prompt.clone(),
    };

    let engine = Arc::new(QueryEngine::new(client, engine_config));

    // ── Phase 9: Dispatch ─────────────────────────────────────────────────────
    if cli.print || cli.command.is_some() {
        crate::output::run_non_interactive(cli, engine, command_registry, settings).await
    } else {
        launch_tui(engine, global_config, Arc::clone(&settings), command_registry, session_id).await
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Build the session directory path (`~/.claude/sessions/<session_id>/`).
fn session_dir_for(session_id: &SessionId) -> Result<PathBuf> {
    let home = dirs_next::home_dir()
        .ok_or_else(|| anyhow::anyhow!("cannot determine home directory"))?;
    Ok(home.join(".claude").join("sessions").join(session_id.to_string()))
}

/// Convert CLI permission-mode string into a `ToolPermissionContext`.
fn build_permission_ctx(cli: &Cli) -> Result<ToolPermissionContext> {
    use code_types::permissions::PermissionRuleSource;

    let mode = match cli.permission_mode.as_str() {
        "auto" | "autoEdit" | "acceptEdits" => PermissionMode::AcceptEdits,
        "bypass-permissions" | "bypassPermissions" => PermissionMode::BypassPermissions,
        "plan" => PermissionMode::Plan,
        _ => PermissionMode::Default,
    };

    let mut ctx = ToolPermissionContext::default();
    ctx.mode = mode;

    // Parse allowed tools → always_allow_rules (CliArg source).
    if let Some(ref allowed) = cli.allowed_tools {
        let tools: Vec<String> = allowed
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect();
        if !tools.is_empty() {
            ctx.always_allow_rules.insert(PermissionRuleSource::CliArg, tools);
        }
    }

    // Parse disallowed tools → always_deny_rules (CliArg source).
    if let Some(ref disallowed) = cli.disallowed_tools {
        let tools: Vec<String> = disallowed
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect();
        if !tools.is_empty() {
            ctx.always_deny_rules.insert(PermissionRuleSource::CliArg, tools);
        }
    }

    Ok(ctx)
}

/// Launch the interactive TUI REPL.
async fn launch_tui(
    engine: Arc<QueryEngine>,
    config: Arc<GlobalConfig>,
    settings: Arc<SettingsJson>,
    registry: CommandRegistry,
    session_id: SessionId,
) -> Result<()> {
    code_tui::start(engine, config, settings, registry, session_id).await
}
