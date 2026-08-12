use super::*;
use serde::Deserialize;

/// Handle `x.ai/models/update` — model list changed (etag-triggered refresh).
pub(super) fn handle_models_update(notif: &acp::ExtNotification, app: &mut AppView) -> bool {
    let _ = (notif, app);
    // Model catalogs are local configuration in the clean build. Do not accept
    // a hosted push notification that could replace the user's provider/model
    // selection or turn the original x.ai endpoint back on.
    false
}

/// Apply the provider-neutral subset of an in-process settings refresh.
///
/// The shell and pager still use the upstream ACP method name for protocol
/// compatibility, but account, billing, hosted-gate, campaign, announcement,
/// and retired media fields are deliberately not part of [`PagerSettingsUpdate`].
pub(super) fn handle_settings_update(notif: &acp::ExtNotification, app: &mut AppView) -> bool {
    let Ok(update) = serde_json::from_str::<PagerSettingsUpdate>(notif.params.get()) else {
        tracing::warn!("Failed to parse x.ai/settings/update");
        return false;
    };

    if let Some(v) = update.auto_permission_mode_enabled {
        xai_grok_shell::util::config::cache_remote_auto_permission_mode_enabled(Some(v));
        app.auto_mode_gate = xai_grok_shell::util::config::auto_permission_mode_enabled_from_disk();
        if !app.auto_mode_gate {
            let leaving_auto: Vec<acp::SessionId> = app
                .agents
                .values()
                .filter(|agent| agent.session.is_auto())
                .filter_map(|agent| agent.session.session_id.clone())
                .collect();
            super::super::dispatch::downgrade_displayed_auto_if_gated(app);
            notify_sessions_leave_auto(app, &leaving_auto);
        }
        app.sync_permission_mode_slash_gate();
    }

    if let Some(remote) = update.permission_mode.as_ref()
        && app.permission_mode_from_soft_default
    {
        let root = xai_grok_shell::config::load_effective_config().ok();
        apply_soft_default_permission_mode(
            app,
            root.as_ref().and_then(|value| value.get("ui")),
            remote.as_deref(),
        );
    }

    if let Some(v) = update.show_resolved_model {
        app.show_resolved_model = v;
    }

    // Share is not a clean-build capability. A legacy local producer may
    // still include the field, but it can only drive the surface off.
    if update.sharing_enabled.is_some() {
        app.sharing_enabled = false;
        for agent in app.agents.values_mut() {
            agent.set_sharing_enabled(false);
        }
    }

    if let Some(remote) = update.session_picker_grouped {
        app.session_picker_grouped = std::env::var("GROK_SESSION_PICKER_GROUPED")
            .ok()
            .and_then(|value| match value.as_str() {
                "1" | "true" => Some(true),
                "0" | "false" => Some(false),
                _ => None,
            })
            .or_else(|| {
                xai_grok_shell::config::load_effective_config()
                    .ok()
                    .and_then(|cfg| cfg.get("cli")?.get("session_picker_grouped")?.as_bool())
            })
            .unwrap_or(remote);
    }

    let (requirements, user_config, managed_config) = (
        xai_grok_shell::config::load_merged_requirements(),
        xai_grok_shell::config::load_from_disk().ok(),
        xai_grok_shell::config::load_managed_config().ok(),
    );

    let remote = xai_grok_shell::util::config::RemoteSettings {
        group_tool_verbs: update.group_tool_verbs,
        ..Default::default()
    };
    let grouped = xai_grok_shell::util::config::resolve_group_tool_verbs(
        requirements.as_ref(),
        user_config.as_ref(),
        managed_config.as_ref(),
        Some(&remote),
    )
    .value;
    if grouped != crate::appearance::cache::load_group_tool_verbs() {
        crate::appearance::cache::set_group_tool_verbs(grouped);
        for agent in app.agents.values_mut() {
            agent.scrollback.clear_group_expansion();
            agent.scrollback.invalidate_heights();
            for child in agent.subagent_views.values_mut() {
                child.scrollback.clear_group_expansion();
                child.scrollback.invalidate_heights();
            }
        }
    }

    let remote = xai_grok_shell::util::config::RemoteSettings {
        collapsed_edit_blocks: update.collapsed_edit_blocks,
        ..Default::default()
    };
    let collapsed = xai_grok_shell::util::config::resolve_collapsed_edit_blocks(
        requirements.as_ref(),
        user_config.as_ref(),
        managed_config.as_ref(),
        Some(&remote),
    )
    .value;
    let previous = crate::appearance::cache::load_collapsed_edit_blocks();
    if collapsed != previous {
        crate::appearance::cache::set_collapsed_edit_blocks(collapsed);
        for agent in app.agents.values_mut() {
            agent
                .scrollback
                .apply_collapsed_edit_blocks_flip(previous, collapsed);
            for child in agent.subagent_views.values_mut() {
                child
                    .scrollback
                    .apply_collapsed_edit_blocks_flip(previous, collapsed);
            }
        }
    }

    if let Some(remote_tips) = update.tips {
        app.tips = xai_grok_shell::util::config::resolve_tips(
            requirements.as_ref(),
            user_config.as_ref(),
            managed_config.as_ref(),
            Some(&remote_tips),
        );
        app.tip = if app.tips.is_empty() {
            None
        } else {
            let grok_home = xai_grok_tools::util::grok_home::grok_home();
            xai_grok_shell::util::tips::pick_and_advance(&app.tips, &grok_home)
        };
    }

    if let Some(remote_tags) = update.slash_command_tags.as_ref() {
        let effective_config = xai_grok_shell::config::load_effective_config().ok();
        let empty = toml::Value::Table(Default::default());
        *app.command_tags.borrow_mut() = xai_grok_shell::util::config::resolve_slash_command_tags(
            effective_config.as_ref().unwrap_or(&empty),
            remote_tags.as_ref(),
        );
    }

    tracing::info!("local settings updated via ACP");
    true
}

/// Re-arm the soft-defaulted launch mode from a pushed `permission_mode`
/// (TOML `[ui]` > remote > Ask), for the next `/new` only — live sessions are
/// untouched and nothing is persisted. `effective_ui` is injected so the
/// resolve is deterministic under test. Enforcement gating reuses the app's
/// startup snapshots (`yolo_policy_block`, `auto_mode_gate`); the agent's
/// permission manager re-clamps authoritatively at decision time.
pub(super) fn apply_soft_default_permission_mode(
    app: &mut AppView,
    effective_ui: Option<&toml::Value>,
    remote: Option<&str>,
) {
    let mode = xai_grok_shell::util::config::resolve_permission_mode(effective_ui, remote);
    app.default_yolo = mode.is_always_approve() && app.yolo_policy_block.is_none();
    let auto = mode.is_auto() && app.auto_mode_gate && !app.default_yolo;
    app.current_ui.permission_mode = Some(if auto {
        "auto".to_string()
    } else if app.default_yolo {
        "always-approve".to_string()
    } else {
        xai_grok_shell::util::config::resolved_display_permission_mode(effective_ui, remote)
            .to_string()
    });
}

/// Tell live sessions to leave Auto on the mid-session kill-switch: fire the
/// `x.ai/yolo_mode_changed` notification the agent maps to
/// `SetAutoMode { enabled: false }`, fire-and-forget over the shared ACP channel.
/// The notification is CLIENT-scoped (the agent applies it to every session of
/// the sending client), so one send covers all affected sessions. `yolo_mode` is
/// deliberately OMITTED — the agent skips the yolo branch when the key is absent,
/// so a sibling tab's always-approve is preserved; only auto is cleared.
pub(super) fn notify_sessions_leave_auto(app: &AppView, session_ids: &[acp::SessionId]) {
    if session_ids.is_empty() {
        return;
    }
    let params = serde_json::json!({
        "auto_mode": false,
        "permission_mode": "ask",
    });
    let notification = acp::ExtNotification::new(
        "x.ai/yolo_mode_changed",
        serde_json::value::to_raw_value(&params)
            .expect("serialize yolo_mode_changed params")
            .into(),
    );
    let (response_tx, _response_rx) = tokio::sync::oneshot::channel();
    let args = xai_acp_lib::AcpArgs {
        request: notification,
        response_tx,
    };
    let _ = app.acp_tx.send(args.into());
}

/// Handle `x.ai/sessions/changed` — the leader broadcasts roster
/// upserts/removals to all clients (FleetView dashboard).
pub(super) fn handle_sessions_changed(notif: &acp::ExtNotification, app: &mut AppView) -> bool {
    let Ok(changed) = serde_json::from_str::<crate::app::roster::RosterChanged>(notif.params.get())
    else {
        tracing::warn!("Failed to parse x.ai/sessions/changed");
        return false;
    };
    let mut affected = false;
    for entry in changed.upserted {
        app.upsert_roster_entry(entry);
        affected = true;
    }
    for sid in changed.removed {
        app.remove_roster_entry(&sid);
        affected = true;
    }
    affected
}

/// Deserialization type for the `x.ai/settings/update` notification payload.
///
/// This is intentionally a separate struct from `SettingsUpdateNotification` in
/// `xai-grok-shell/src/agent/mvp_agent.rs`. The shell side derives `Serialize`
/// and owns the canonical field set from `RemoteSettings`; this pager side
/// derives `Deserialize` and selectively consumes only the fields relevant to
/// the TUI. Keeping them separate avoids coupling the pager to shell internals
/// and lets each side evolve independently (e.g. adding a shell-only field
/// doesn't require a pager change). All fields are `Option` with
/// `#[serde(default)]` so that partial updates and forward-compatible additions
/// are handled gracefully.
///
/// **Keep in sync** with field names/types in `SettingsUpdateNotification` at
/// `xai-grok-shell/src/agent/mvp_agent.rs` when adding fields that both sides
/// need.
#[derive(serde::Deserialize)]
pub(super) struct PagerSettingsUpdate {
    #[serde(default)]
    show_resolved_model: Option<bool>,
    #[serde(default)]
    sharing_enabled: Option<bool>,
    #[serde(default)]
    session_picker_grouped: Option<bool>,
    #[serde(default)]
    tips: Option<Vec<String>>,
    /// Free-form per-command slash-dropdown tags (canonical name → tag).
    /// Presence-aware and tolerant: omit = no update (older shell), `null` =
    /// remote cleared, map = set, malformed = warn + treat as absent so a
    /// bad value never fails the whole `PagerSettingsUpdate` parse.
    #[serde(default, deserialize_with = "deserialize_settings_update_tags")]
    slash_command_tags: Option<Option<std::collections::BTreeMap<String, String>>>,
    #[serde(default)]
    auto_permission_mode_enabled: Option<bool>,
    /// Soft-default permission mode. Presence-aware: omit = no update,
    /// `null` = recompute with remote=None, string = that soft-default.
    /// Omission happens with older shells that predate the field (they can
    /// never clear a mode they don't know about) — that version skew is why
    /// this is tri-state instead of a plain `Option`.
    #[serde(default, deserialize_with = "deserialize_presence_aware_string")]
    permission_mode: Option<Option<String>>,
    #[serde(default)]
    group_tool_verbs: Option<bool>,
    #[serde(default)]
    collapsed_edit_blocks: Option<bool>,
}

/// Presence-aware string: omit → `None` (`#[serde(default)]`), null →
/// `Some(None)`, string → `Some(Some(_))`.
fn deserialize_presence_aware_string<'de, D>(
    deserializer: D,
) -> Result<Option<Option<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Some(Option::<String>::deserialize(deserializer)?))
}

/// Presence-aware + tolerant tags map for live settings updates.
/// Only invoked when the field is present (`#[serde(default)]` covers omit).
/// - JSON null → `Some(None)` (explicit remote clear)
/// - valid object → `Some(Some(map))`
/// - malformed → warn + `Ok(None)` (leave tags alone; do not fail the struct)
fn deserialize_settings_update_tags<'de, D>(
    deserializer: D,
) -> Result<Option<Option<std::collections::BTreeMap<String, String>>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::Null => Ok(Some(None)),
        v => match serde_json::from_value::<std::collections::BTreeMap<String, String>>(v) {
            Ok(m) => Ok(Some(Some(m))),
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "malformed slash_command_tags in settings update; leaving tags unchanged"
                );
                Ok(None)
            }
        },
    }
}

#[cfg(test)]
mod presence_aware_dto_tests {
    use super::*;

    #[derive(Deserialize)]
    struct Probe {
        #[serde(default, deserialize_with = "deserialize_presence_aware_string")]
        permission_mode: Option<Option<String>>,
    }

    #[test]
    fn permission_mode_dto_distinguishes_omit_from_null() {
        let omit: Probe = serde_json::from_value(serde_json::json!({
            "show_resolved_model": true,
        }))
        .unwrap();
        assert_eq!(omit.permission_mode, None, "omit must be None (no update)");

        let null_v: Probe = serde_json::from_value(serde_json::json!({
            "permission_mode": null,
        }))
        .unwrap();
        assert_eq!(
            null_v.permission_mode,
            Some(None),
            "explicit null must be Some(None)"
        );

        let some_v: Probe = serde_json::from_value(serde_json::json!({
            "permission_mode": "always-approve",
        }))
        .unwrap();
        assert_eq!(
            some_v.permission_mode,
            Some(Some("always-approve".into())),
            "string must be Some(Some(_))"
        );
    }

    #[test]
    fn slash_command_tags_dto_absent_null_map_and_malformed() {
        // 1. field absent → outer None (leave tags alone)
        let absent: PagerSettingsUpdate = serde_json::from_value(serde_json::json!({
            "tips": ["hello"],
        }))
        .expect("absent slash_command_tags must not fail parse");
        assert_eq!(absent.slash_command_tags, None, "omit must be None");
        assert_eq!(absent.tips.as_deref(), Some(&["hello".to_string()][..]));

        // 2. explicit null → Some(None) (remote cleared)
        let null_v: PagerSettingsUpdate = serde_json::from_value(serde_json::json!({
            "slash_command_tags": null,
        }))
        .expect("null slash_command_tags must parse");
        assert_eq!(
            null_v.slash_command_tags,
            Some(None),
            "explicit null must be Some(None)"
        );

        // 3. valid map → Some(Some(map))
        let map_v: PagerSettingsUpdate = serde_json::from_value(serde_json::json!({
            "slash_command_tags": {"workflows": "new"},
        }))
        .expect("valid slash_command_tags map must parse");
        let tags = map_v
            .slash_command_tags
            .as_ref()
            .and_then(|inner| inner.as_ref())
            .expect("expected Some(Some(map))");
        assert_eq!(tags.get("workflows").map(String::as_str), Some("new"));
        assert_eq!(tags.len(), 1);

        // 4. malformed must NOT fail the whole struct; sibling fields still apply
        let bad: PagerSettingsUpdate = serde_json::from_value(serde_json::json!({
            "slash_command_tags": ["oops"],
            "tips": ["still-applied"],
            "permission_mode": "always-approve",
        }))
        .expect("malformed slash_command_tags must not fail PagerSettingsUpdate parse");
        assert_eq!(
            bad.slash_command_tags, None,
            "malformed tags treated as absent"
        );
        assert_eq!(
            bad.tips.as_deref(),
            Some(&["still-applied".to_string()][..]),
            "sibling tips must still parse"
        );
        assert_eq!(
            bad.permission_mode,
            Some(Some("always-approve".into())),
            "sibling permission_mode must still parse"
        );
    }
}
