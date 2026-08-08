//! `/usage` compatibility command.

use crate::app::actions::Action;
use crate::slash::command::{AppCtx, ArgItem, CommandExecCtx, CommandResult, SlashCommand};
use agent_client_protocol as acp;

pub struct UsageCommand;

/// Detect external-auth installs once at pager startup.
pub(crate) fn detect_external_auth_provider(auth_methods: &[acp::AuthMethod]) -> bool {
    auth_methods.iter().any(auth_method_is_external_provider)
        || auth_provider_env_set()
        || auth_provider_config_set()
}

fn auth_method_is_external_provider(method: &acp::AuthMethod) -> bool {
    method
        .meta()
        .as_ref()
        .and_then(|v| v.get("external_provider"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

fn auth_provider_env_set() -> bool {
    std::env::var("GROK_AUTH_PROVIDER_COMMAND")
        .ok()
        .is_some_and(|s| !s.trim().is_empty())
}

fn auth_provider_config_set() -> bool {
    let Ok(raw) = xai_grok_shell::config::load_effective_config() else {
        return false;
    };
    let Ok(cfg) = xai_grok_shell::agent::config::Config::new_from_toml_cfg(&raw) else {
        return false;
    };
    cfg.grok_com_config
        .auth_provider_command
        .as_deref()
        .is_some_and(|s| !s.trim().is_empty())
}

impl SlashCommand for UsageCommand {
    fn name(&self) -> &str {
        "usage"
    }

    fn aliases(&self) -> &[&str] {
        &["cost"]
    }

    fn description(&self) -> &str {
        "Show local session token and context usage"
    }

    fn usage(&self) -> &str {
        "/usage [show]"
    }

    fn takes_args(&self) -> bool {
        true
    }

    fn visible(&self, ctx: &AppCtx) -> bool {
        ctx.usage_command_visible
    }

    fn takes_args_now(&self, ctx: &AppCtx) -> bool {
        let _ = ctx;
        true
    }

    fn suggest_args(&self, ctx: &AppCtx, _args_query: &str) -> Option<Vec<ArgItem>> {
        let _ = ctx;
        Some(vec![ArgItem {
            display: "show".into(),
            match_text: "show".into(),
            insert_text: "show".into(),
            description: "View usage".into(),
        }])
    }

    fn run(&self, ctx: &mut CommandExecCtx, args: &str) -> CommandResult {
        let _ = ctx;
        let arg = args.trim();
        match arg {
            "" | "show" => CommandResult::Action(Action::ShowUsage),
            "manage" => CommandResult::Error(
                "Billing management is not available in the local build; /usage shows local session usage.".into(),
            ),
            _ => CommandResult::Error(format!(
                "Unknown argument: {arg}. Use /usage or /usage show"
            )),
        }
    }
}
