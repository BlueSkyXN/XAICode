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

/// Handle `x.ai/settings/update` — remote settings refreshed on `/new`.
pub(super) fn handle_settings_update(notif: &acp::ExtNotification, app: &mut AppView) -> bool {
    // The clean build has no hosted settings channel. Keep the legacy parser
    // below for source/test compatibility, but never apply a pushed account or
    // feature payload at runtime.
    return false;
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

pub(super) fn handle_announcements_update(notif: &acp::ExtNotification, app: &mut AppView) -> bool {
    let _ = (notif, app);
    // Hosted announcement pushes are deliberately ignored.  Keeping the
    // handler in the dispatch table preserves ACP compatibility without
    // allowing marketing or account prompts into the local UI.
    false
}

/// Apply half of [`handle_announcements_update`], with config layers injected
/// so the merge/prune behavior is unit-testable without disk state.
/// `resolve_announcements` honors `GROK_ANNOUNCEMENTS_OVERRIDE` first, so a
/// backend push can't reintroduce announcements when the override is set.
pub(super) fn apply_announcements_update(
    app: &mut AppView,
    next_gen: u64,
    remote: &[xai_grok_announcements::RemoteAnnouncement],
    requirements: Option<&toml::Value>,
    user_config: Option<&toml::Value>,
    managed_config: Option<&toml::Value>,
) {
    let _ = (requirements, user_config, managed_config, remote);
    app.announcement = None;
    app.active_announcements.clear();
    app.hidden_announcement_ids.clear();
    app.announcements_last_gen = next_gen;
    app.tips.clear();
    app.tip = None;
}

pub(super) fn pick_random_announcement(
    announcements: &[xai_grok_announcements::RemoteAnnouncement],
) -> Option<xai_grok_announcements::RemoteAnnouncement> {
    if announcements.is_empty() {
        return None;
    }
    use rand::Rng;
    let idx = rand::rng().random_range(0..announcements.len());
    announcements.get(idx).cloned()
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
    privacy_notice_rollout: Option<bool>,
    #[serde(default)]
    privacy_banner_reshow_days: Option<u64>,
    #[serde(default)]
    voice_mode_enabled: Option<bool>,
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
    // `announcements` is deliberately NOT consumed here: every shell writer of
    // remote_settings also emits gen-ordered `x.ai/announcements/update`
    // (emit_announcements_if_changed), and a gen-less apply on this path could
    // clobber a newer push. Single ingest path: handle_announcements_update.
    /// Remote campaigns snapshot. `Some` whenever the shell has settings
    /// (empty = campaigns withdrawn); `None`/omitted (settings-less push,
    /// older shell) must leave this process's campaign cache untouched.
    #[serde(default)]
    campaigns: Option<Vec<xai_grok_shell::util::config::CampaignOverride>>,
    #[serde(default)]
    gate_message: Option<String>,
    #[serde(default)]
    gate_url: Option<String>,
    #[serde(default)]
    gate_label: Option<String>,
    #[serde(default)]
    allow_access: Option<bool>,
    #[serde(default)]
    subscription_tier_display: Option<String>,
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
    #[serde(default)]
    subscription_watch_interval_secs: Option<u64>,
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
