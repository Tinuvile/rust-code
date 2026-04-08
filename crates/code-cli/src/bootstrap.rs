//! Full bootstrap sequence: config → auth → provider → engine → dispatch.
//!
//! Ref: src/main.tsx, src/entrypoints/init.ts

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};

use code_api::providers::registry::{create_provider, ProviderConfig};
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
use code_types::provider::{detect_provider_from_env, resolve_api_key, LlmProvider, ProviderKind};

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
    if let Some(provider) = &cli.provider {
        merged_settings.provider = Some(provider.clone());
    }
    if let Some(base_url) = &cli.provider_base_url {
        merged_settings.provider_base_url = Some(base_url.clone());
    }

    let settings = Arc::new(merged_settings);

    // ── Phase 2: Resolve provider ─────────────────────────────────────────────
    let provider_kind = resolve_provider_kind(&settings);

    // ── Phase 3: Resolve API key ──────────────────────────────────────────────
    let storage = create_secure_storage();
    let api_key = resolve_provider_api_key(
        provider_kind,
        &global_config,
        &settings,
        storage.as_ref(),
    )
    .await;

    if api_key.is_empty() {
        tracing::warn!(
            "No API key found for provider {}. Set {} or run /login.",
            provider_kind,
            provider_kind.api_key_env_vars().first().unwrap_or(&"LLM_API_KEY"),
        );
    }

    let provider: Arc<dyn LlmProvider> = create_provider(ProviderConfig {
        kind: provider_kind,
        api_key,
        base_url: settings.provider_base_url.clone(),
        extra_headers: HashMap::new(),
        timeout: Duration::from_secs(600),
    });

    // ── Phase 4: Session setup ────────────────────────────────────────────────
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

    // ── Phase 5: Permission context ───────────────────────────────────────────
    let permission_ctx = build_permission_ctx(&cli)?;

    // ── Phase 6: Tool registry ────────────────────────────────────────────────
    let _tool_registry = ToolRegistry::with_default_tools(&cwd);

    // ── Phase 7: Command registry ─────────────────────────────────────────────
    let command_registry = CommandRegistry::with_all_commands();

    // ── Phase 8: Model resolution ─────────────────────────────────────────────
    let model = settings.model.clone().unwrap_or_else(|| {
        default_model_for_provider(provider_kind)
    });

    // ── Phase 9: Query engine ─────────────────────────────────────────────────
    let engine_config = QueryEngineConfig {
        model,
        cwd: cwd.clone(),
        session_id: session_id.to_string(),
        session_dir,
        permission_ctx,
        system_appendix: cli.append_system_prompt.clone(),
        provider_kind,
    };

    let engine = Arc::new(QueryEngine::new(provider, engine_config));

    // ── Phase 10: Background update check ──────────────────────────────────────
    crate::update_check::spawn_update_check();

    // ── Phase 11: Dispatch ────────────────────────────────────────────────────
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

/// Resolve which provider to use.
///
/// Priority: settings.provider > LLM_PROVIDER env > auto-detect > default (Anthropic).
fn resolve_provider_kind(settings: &SettingsJson) -> ProviderKind {
    // 1. From settings (which includes CLI overrides already merged in).
    if let Some(ref s) = settings.provider {
        if let Some(kind) = ProviderKind::from_str_loose(s) {
            return kind;
        }
    }
    // 2. From LLM_PROVIDER env var.
    if let Ok(val) = std::env::var("LLM_PROVIDER") {
        if let Some(kind) = ProviderKind::from_str_loose(&val) {
            return kind;
        }
    }
    // 3. Auto-detect from available API key env vars.
    if let Some(kind) = detect_provider_from_env() {
        return kind;
    }
    // 4. Default.
    ProviderKind::Anthropic
}

/// Resolve the API key for the selected provider.
///
/// For Anthropic, falls through to the existing `get_api_key()` which also
/// checks keychain/OAuth. For other providers, checks env vars.
async fn resolve_provider_api_key(
    provider: ProviderKind,
    global_config: &GlobalConfig,
    settings: &SettingsJson,
    storage: &dyn code_auth::secure_storage::SecureStorage,
) -> String {
    // For Anthropic family, use the existing auth system.
    if provider.is_anthropic_family() {
        return match get_api_key(global_config, settings, storage).await {
            Some(info) => {
                tracing::debug!("API key resolved from {:?}", info.source);
                info.key
            }
            None => String::new(),
        };
    }

    // For other providers, resolve from env vars.
    let custom_env = settings.provider_api_key_env.as_deref();
    resolve_api_key(provider, custom_env).unwrap_or_default()
}

/// Return a sensible default model for a given provider.
fn default_model_for_provider(provider: ProviderKind) -> String {
    match provider {
        ProviderKind::Anthropic | ProviderKind::Bedrock | ProviderKind::Vertex | ProviderKind::Azure => {
            "claude-sonnet-4-6".to_owned()
        }
        ProviderKind::OpenAi => "gpt-4o".to_owned(),
        ProviderKind::Gemini => "gemini-2.5-flash".to_owned(),
        ProviderKind::DeepSeek => "deepseek-chat".to_owned(),
        ProviderKind::Kimi => "moonshot-v1-128k".to_owned(),
        ProviderKind::Minimax => "abab6.5s-chat".to_owned(),
        ProviderKind::OpenAiCompatible => "default".to_owned(),
    }
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
