//! `/usage` compatibility command.

use crate::app::actions::Action;
use crate::slash::command::{AppCtx, ArgItem, CommandExecCtx, CommandResult, SlashCommand};

pub struct UsageCommand;

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
