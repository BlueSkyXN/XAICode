use anyhow::Result;
use clap::Subcommand;
use xai_grok_shell::agent::config::Config as AgentConfig;
use xai_grok_shell::auth::AuthManager;
use xai_grok_shell::session::merge::MergedSession;
use xai_grok_shell::util::grok_home::grok_home;
#[derive(Debug, clap::Args, Clone)]
pub struct SessionsArgs {
    #[command(subcommand)]
    command: SessionsCommand,
}

#[derive(Debug, Subcommand, Clone)]
enum SessionsCommand {
    /// List recent sessions (same as search with no query)
    List {
        /// Maximum number of sessions to show
        #[arg(short = 'n', long, default_value = "20")]
        limit: usize,
    },
    /// Search sessions by keyword
    Search {
        /// Search query (searches summaries and first prompts).
        query: String,
        /// Maximum number of sessions to show
        #[arg(short = 'n', long, default_value = "20")]
        limit: usize,
    },
    /// Permanently delete a session from history
    Delete {
        /// Session id to delete.
        id: String,
    },
}

pub async fn run(args: SessionsArgs, agent_config: &AgentConfig) -> Result<()> {
    // Session history is deliberately local-only.  The upstream command
    // merged a hosted registry and attempted account authentication; the clean
    // build keeps the useful local index/delete behavior without that path.
    let auth_manager = std::sync::Arc::new(AuthManager::new_local(
        &grok_home(),
        agent_config.grok_com_config.clone(),
    ));

    let cwd = std::env::current_dir().unwrap_or_else(|_| ".".into());

    match args.command {
        SessionsCommand::List { limit } => {
            let sessions = xai_grok_shell::session::merge::fetch_merged(
                None,
                cwd.to_str(),
                xai_grok_shell::session::merge::CwdScope::WithSiblings,
                None,
                limit,
            )
            .await;
            print_sessions_grouped(&sessions);
        }
        SessionsCommand::Search { query, limit } => {
            use xai_grok_shell::session::storage::search::{SessionSearchRequest, execute_search};

            let req = SessionSearchRequest {
                query,
                cwd: Some(cwd.to_string_lossy().to_string()),
                limit,
                offset: 0,
                include_content: true,
            };
            let resp = execute_search(&grok_home(), &req).await?;

            for hit in &resp.results {
                let title = if hit.title.is_empty() {
                    "(untitled)"
                } else {
                    &hit.title
                };
                let time = chrono::DateTime::from_timestamp(hit.updated_at_unix, 0)
                    .map(|dt| {
                        dt.with_timezone(&chrono::Local)
                            .format("%b %d, %l:%M%P")
                            .to_string()
                    })
                    .unwrap_or_default();
                println!(
                    "{} (score: {:.2})  {}\n  {}\n  {}",
                    hit.session_id,
                    hit.score,
                    time,
                    title,
                    hit.snippet.as_deref().unwrap_or("")
                );
            }

            println!("\nTotal: {}", resp.results.len().min(limit));
        }
        SessionsCommand::Delete { id } => {
            let deletion = xai_grok_shell::session::persistence::delete_session_history(
                &id,
                None,
                false,
                auth_manager.clone(),
            )
            .await?;

            if deletion.any_removed() {
                println!("Deleted session {id}");
            } else {
                println!("No session found with id {id}.");
            }
        }
    }

    Ok(())
}

/// Print sessions grouped by worktree label, preserving the original table
/// format with a `Label: <label>` header before each group.
fn print_sessions_grouped(sessions: &[MergedSession]) {
    if sessions.is_empty() {
        println!("No sessions found.");
        return;
    }

    // Group by worktree_label, sort alphabetically, None last.
    let mut groups: std::collections::BTreeMap<Option<&str>, Vec<&MergedSession>> =
        std::collections::BTreeMap::new();
    for s in sessions {
        groups
            .entry(s.worktree_label.as_deref())
            .or_default()
            .push(s);
    }

    let header = format!(
        "{:<36}  {:<10}  {:<10}  {:<10}  {}",
        "SESSION ID", "CREATED", "UPDATED", "STATUS", "SUMMARY"
    );

    // Labeled groups first (alphabetical), then unlabeled last.
    let none_group = groups.remove(&None);
    let print_group = |label_line: &str, members: &[&MergedSession]| {
        println!("\n{label_line}");
        println!("{header}");
        for s in members {
            let first_line;
            let summary: &str = if !s.summary.is_empty() {
                &s.summary
            } else if let Some(ref fp) = s.first_prompt
                && let Some(line) = fp.lines().find(|l| !l.trim().is_empty())
            {
                first_line = line.trim().to_string();
                &first_line
            } else {
                "(no summary)"
            };
            let truncated: String = summary.chars().take(50).collect();
            let created = &s.created_at[..s.created_at.len().min(10)];
            let updated = &s.updated_at[..s.updated_at.len().min(10)];
            println!(
                "{}  {}  {}  {}  {}",
                s.session_id, created, updated, s.source, truncated
            );
        }
    };

    for (label, members) in &groups {
        let line = format!("Label: {}", label.unwrap_or(""));
        print_group(&line, members);
    }
    if let Some(members) = &none_group {
        print_group("(no label)", members);
    }
}
