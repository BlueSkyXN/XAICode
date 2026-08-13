mod cursor;
mod envelope;
mod facets;
mod row;
use crate::agent::session_registry_client::SessionRegistryClient;
pub use crate::session::merge::CwdScope;
use agent_client_protocol as acp;
use cursor::{CompositeCursor, ConvLane, Paginated, merge_and_paginate};
pub use envelope::{FacetMap, FacetValue, SessionKind, SessionMetaEnvelope};
pub use facets::{
    BRANCH_FACET_KEY, BranchFacet, CWD_FACET_KEY, CwdFacet, FacetProvider, FacetRegistry,
    FacetSummary, FacetSummaryKey, FacetSummaryValue, GIT_ROOT_FACET_KEY, GitRootFacet,
    KIND_FACET_KEY, KindFacet, NormalizedItem, Pushdown, REPO_FACET_KEY, RepoFacet,
    SOURCE_WORKSPACE_FACET_KEY, STARRED_FACET_KEY, SourceQuery, SourceWorkspaceFacet, StarredFacet,
    WORKSPACE_FACET_KEY, WORKTREE_FACET_KEY, WorkspaceFacet, WorktreeFacet, build_facet_registry,
};
pub use row::{ExtSupersetRow, RowMeta, SessionInfo, UnifiedRow, merged_session_to_row};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::LazyLock;
pub const DEFAULT_LIMIT: usize = 30;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartialReason {
    Timeout,
    Error,
    NoOauth,
}
impl PartialReason {
    fn as_str(self) -> &'static str {
        match self {
            PartialReason::Timeout => "timeout",
            PartialReason::Error => "error",
            PartialReason::NoOauth => "no_oauth",
        }
    }
}
static FACET_REGISTRY: LazyLock<FacetRegistry> = LazyLock::new(build_facet_registry);
pub(crate) fn facet_registry() -> &'static FacetRegistry {
    &FACET_REGISTRY
}
/// Hard-off in release builds so they can't enable the
/// conversations lane via env.
pub(crate) fn conversations_lane_enabled() -> bool {
    false
}
/// Env lane (desktop `GROK_SESSION_LIST_CONVERSATIONS`) OR process-wide
/// `--chat` (`GROK_CHAT_MODE`); hard-off in release builds.
/// The compatibility predicate remains hard-off in this composition.
pub fn conversations_lane_active() -> bool {
    false
}
/// Parse `x.ai/session/list` params and, under process-wide chat mode, force
/// the conversations-only `kind` facet (see [`force_kind_chat`]).
///
/// Client-sent `kind` of `chat`/`build` is honored only behind
/// `feature = "local-workspace"` (pager welcome Local history). Chat-only
/// Desktop/ACP agents keep the force-rewrite so `kind: ["build"]` cannot
/// surface Build rows.
pub fn parse_list_req(raw: &str) -> Result<ListReq, serde_json::Error> {
    serde_json::from_str(raw)
}
fn cwd_scope_from_allow_relax<'de, D>(deserializer: D) -> Result<CwdScope, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(if bool::deserialize(deserializer)? {
        CwdScope::RelaxIfEmpty
    } else {
        CwdScope::WithSiblings
    })
}
fn client_sent_kind_filter(req: &ListReq) -> bool {
    let Some(kind) = req
        .meta
        .as_ref()
        .and_then(|m| m.get("x.ai/facetFilters"))
        .and_then(|f| f.get("kind"))
    else {
        return false;
    };
    match kind {
        serde_json::Value::Array(arr) if !arr.is_empty() => arr
            .iter()
            .any(|v| matches!(v.as_str(), Some("chat" | "build"))),
        serde_json::Value::String(s) if s == "chat" || s == "build" => true,
        _ => false,
    }
}
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListReq {
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub cursor: Option<String>,
    /// Which directories the listing draws from. The wire carries the original
    /// `allowRelax` boolean; `Only` is reachable only in code (ACP
    /// `session/list`), so "exact" and "relax" cannot be requested together.
    /// A relaxed response sets `_meta["x.ai/listScope"]`, re-evaluated per page.
    #[serde(
        default,
        rename = "allowRelax",
        deserialize_with = "cwd_scope_from_allow_relax"
    )]
    pub cwd_scope: CwdScope,
    #[serde(default, rename = "_meta")]
    pub meta: Option<serde_json::Value>,
}
/// Directory scope the returned sessions were drawn from. Wire form is the
/// `as_str` value (`x.ai/listScope`), so no serde derive is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ListScope {
    /// Scoped to the request cwd.
    #[default]
    Cwd,
    /// Relaxed to the cwd's repo when the cwd itself had no sessions.
    Repo,
    /// Relaxed to all directories when the cwd is not a git repo.
    All,
}
impl ListScope {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cwd => "cwd",
            Self::Repo => "repo",
            Self::All => "all",
        }
    }
    /// True when the scope relaxed past the cwd, to the repo or to all directories.
    pub const fn is_relaxed(self) -> bool {
        !matches!(self, Self::Cwd)
    }
}
pub struct UnifiedListResult {
    pub rows: Vec<UnifiedRow>,
    pub next_cursor: Option<String>,
    pub facets: FacetSummary,
    pub conversations_partial: Option<PartialReason>,
    /// Directory scope `rows` were drawn from; see [`ListReq::allow_relax`].
    pub scope: ListScope,
}
#[derive(Debug, Default)]
struct ParsedMeta {
    facet_filters: BTreeMap<String, Vec<serde_json::Value>>,
    query: Option<String>,
    limit: Option<usize>,
}
impl ParsedMeta {
    fn parse(meta: Option<&serde_json::Value>) -> Self {
        let Some(meta) = meta else {
            return Self::default();
        };
        let facet_filters = meta
            .get("x.ai/facetFilters")
            .and_then(|v| v.as_object())
            .map(|obj| {
                obj.iter()
                    .map(|(k, v)| (k.clone(), value_list(v)))
                    .collect()
            })
            .unwrap_or_default();
        let query = meta
            .get("x.ai/query")
            .and_then(|v| v.as_str())
            .map(str::to_owned);
        let limit = meta
            .get("x.ai/limit")
            .and_then(serde_json::Value::as_u64)
            .map(|n| n as usize);
        Self {
            facet_filters,
            query,
            limit,
        }
    }
}
fn value_list(v: &serde_json::Value) -> Vec<serde_json::Value> {
    match v {
        serde_json::Value::Array(arr) => arr.clone(),
        other => vec![other.clone()],
    }
}
/// Rewrite `req` so the `kind` facet filter is exactly `["chat"]`.
///
/// Used when process chat mode is on **and** the client omitted a recognized
/// `kind` facet (see [`parse_list_req`]). Welcome history sends an explicit
/// `kind` (`chat` / `build`) that must not be rewritten. Other facet filters
/// and `_meta` keys are left untouched.
pub(crate) fn force_kind_chat(req: &mut ListReq) {
    force_kind(req, SessionKind::Chat);
}
/// REPLACES any client-sent `kind` allow-list (a union would re-enable the
/// excluded lanes); every other facet filter and `_meta` key is untouched.
pub(crate) fn force_kind(req: &mut ListReq, kind: SessionKind) {
    let mut meta = match req.meta.take() {
        Some(serde_json::Value::Object(map)) => map,
        _ => serde_json::Map::new(),
    };
    let mut filters = match meta.remove("x.ai/facetFilters") {
        Some(serde_json::Value::Object(map)) => map,
        _ => serde_json::Map::new(),
    };
    filters.insert(
        KIND_FACET_KEY.to_owned(),
        serde_json::json!([kind.as_str()]),
    );
    meta.insert(
        "x.ai/facetFilters".to_owned(),
        serde_json::Value::Object(filters),
    );
    req.meta = Some(serde_json::Value::Object(meta));
}
pub async fn build_unified_list(
    registry_client: Option<&SessionRegistryClient>,
    mut req: ListReq,
) -> UnifiedListResult {
    let reg = facet_registry();
    let ParsedMeta {
        facet_filters,
        query: meta_query,
        limit: meta_limit,
    } = ParsedMeta::parse(req.meta.as_ref());
    let limit = req.limit.or(meta_limit).unwrap_or(DEFAULT_LIMIT);
    let query = req.query.or(meta_query);
    let cursor = CompositeCursor::decode(req.cursor.as_deref());
    let mut source_query = SourceQuery::default();
    reg.apply_pushdown(&facet_filters, &mut source_query);
    let exclude_conversations = excludes_conversations(&facet_filters);
    let exclude_build = excludes_build(&facet_filters);
    let over = crate::session::merge::over_fetch(limit);
    let cwd_scope = req.cwd_scope;
    let can_relax = relax_eligible(RelaxGate {
        opted_in: matches!(req.cwd_scope, CwdScope::RelaxIfEmpty),
        no_facet_filters: facet_filters.is_empty(),
        has_cwd: req.cwd.is_some(),
        is_search: query.is_some(),
    });
    let local_fut = async {
        if exclude_build {
            return LocalLane::default();
        }
        let cwd = req.cwd.as_deref();
        if can_relax {
            let lanes =
                crate::session::merge::fetch_lanes(registry_client, cwd, cwd_scope, None, over)
                    .await;
            let rows = to_rows(
                crate::session::merge::merge(
                    lanes.remote.clone(),
                    lanes.local,
                    None,
                    &lanes.repo_urls,
                    over,
                ),
                reg,
            );
            LocalLane {
                rows,
                relax: Some(RelaxInputs {
                    remote: lanes.remote,
                    repo_urls: lanes.repo_urls,
                }),
            }
        } else {
            let merged = crate::session::merge::fetch_merged(
                registry_client,
                cwd,
                cwd_scope,
                query.as_deref(),
                over,
            )
            .await;
            LocalLane {
                rows: to_rows(merged, reg),
                relax: None,
            }
        }
    };
    let (LocalLane {
        rows: local_rows,
        relax,
    },) = (local_fut.await,);
    let conv_lane = ConvLane::Skipped;
    let (local_rows, scope) = maybe_relax(local_rows, relax, over, reg).await;
    {
        let (conv_lane_status, conv_rows) = match &conv_lane {
            ConvLane::Skipped => ("skipped", 0),
            ConvLane::Degraded(reason) => (reason.as_str(), 0),
            ConvLane::Page { rows, .. } => ("ok", rows.len()),
        };
        tracing::debug!(
            local_lane_skipped = exclude_build,
            local_rows = local_rows.len(),
            conv_lane = conv_lane_status,
            conv_rows,
            "session list lanes"
        );
    }
    let local_rows = reg.apply_in_memory_filters(&facet_filters, local_rows);
    let conv_lane = match conv_lane {
        ConvLane::Page {
            rows,
            next_token,
            frontier,
        } => ConvLane::Page {
            rows: reg.apply_in_memory_filters(&facet_filters, rows),
            next_token,
            frontier,
        },
        other => other,
    };
    let Paginated {
        candidates,
        emit_count,
        next_cursor,
        partial,
    } = merge_and_paginate(local_rows, conv_lane, &cursor, limit);
    let mut rows = candidates;
    rows.truncate(emit_count);
    let facets = reg.summarize_window(&rows);
    UnifiedListResult {
        rows,
        next_cursor: next_cursor.map(|c| c.encode()),
        facets,
        conversations_partial: partial,
        scope,
    }
}
#[derive(Default)]
struct LocalLane {
    rows: Vec<UnifiedRow>,
    relax: Option<RelaxInputs>,
}
struct RelaxInputs {
    remote: Vec<crate::agent::session_registry_client::SessionRecord>,
    repo_urls: Vec<String>,
}
fn to_rows(
    merged: Vec<crate::session::merge::MergedSession>,
    reg: &FacetRegistry,
) -> Vec<UnifiedRow> {
    merged
        .into_iter()
        .map(|m| merged_session_to_row(m, reg))
        .collect()
}
#[derive(Clone, Copy)]
struct RelaxGate {
    opted_in: bool,
    no_facet_filters: bool,
    has_cwd: bool,
    is_search: bool,
}
fn relax_eligible(gate: RelaxGate) -> bool {
    gate.opted_in && gate.no_facet_filters && gate.has_cwd && !gate.is_search
}
/// True when no row has messages (a post-rebuild placeholder counts as empty).
fn lane_has_no_messages(rows: &[UnifiedRow]) -> bool {
    rows.iter().all(|r| r.legacy.num_messages == 0)
}
async fn maybe_relax(
    local_rows: Vec<UnifiedRow>,
    relax: Option<RelaxInputs>,
    over: usize,
    reg: &FacetRegistry,
) -> (Vec<UnifiedRow>, ListScope) {
    let Some(relax) = relax.filter(|_| lane_has_no_messages(&local_rows)) else {
        return (local_rows, ListScope::Cwd);
    };
    let scope = if relax.repo_urls.is_empty() {
        ListScope::All
    } else {
        ListScope::Repo
    };
    let all_local = crate::session::persistence::list_summaries(None)
        .await
        .unwrap_or_else(|e| {
            tracing::debug!("cwd scan failed: {e}");
            Vec::new()
        });
    match relax_rows(relax, all_local, over, reg) {
        Some(relaxed) => {
            tracing::debug!(
                rows = relaxed.len(),
                scope = scope.as_str(),
                "cwd empty; relaxing scope"
            );
            (relaxed, scope)
        }
        None => (local_rows, ListScope::Cwd),
    }
}
/// Re-merge the registry page with a repo-scoped local scan (all directories
/// when the cwd is not a repo); relax only when it reveals a messaged session.
fn relax_rows(
    relax: RelaxInputs,
    all_local: Vec<crate::session::persistence::Summary>,
    over: usize,
    reg: &FacetRegistry,
) -> Option<Vec<UnifiedRow>> {
    let scoped = crate::session::merge::filter_summaries_by_repo(all_local, &relax.repo_urls);
    let rows = to_rows(
        crate::session::merge::merge(relax.remote, scoped, None, &relax.repo_urls, over),
        reg,
    );
    (!lane_has_no_messages(&rows)).then_some(rows)
}
fn excludes_conversations(filters: &BTreeMap<String, Vec<serde_json::Value>>) -> bool {
    match filters.get(KIND_FACET_KEY) {
        Some(allowed) if !allowed.is_empty() => !allowed
            .iter()
            .any(|v| v.as_str() == Some(SessionKind::Chat.as_str())),
        _ => false,
    }
}
/// Mirror of [`excludes_conversations`]: `true` when a non-empty `kind`
/// allow-list does not include `"build"`, so the local lane can be skipped.
fn excludes_build(filters: &BTreeMap<String, Vec<serde_json::Value>>) -> bool {
    match filters.get(KIND_FACET_KEY) {
        Some(allowed) if !allowed.is_empty() => !allowed
            .iter()
            .any(|v| v.as_str() == Some(SessionKind::Build.as_str())),
        _ => false,
    }
}
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ExtListResponse {
    pub sessions: Vec<ExtSupersetRow>,
    #[serde(rename = "nextCursor", skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    #[serde(rename = "_meta")]
    pub meta: ExtListResponseMeta,
}
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ExtListResponseMeta {
    #[serde(rename = "x.ai/facets")]
    pub facets: FacetSummary,
    #[serde(rename = "x.ai/partial")]
    pub partial: PartialInfo,
    /// Present only when the listing relaxed beyond the cwd.
    #[serde(rename = "x.ai/listScope", skip_serializing_if = "Option::is_none")]
    pub list_scope: Option<&'static str>,
}
#[derive(Debug, Clone, Serialize)]
pub(crate) struct PartialInfo {
    pub conversations: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<&'static str>,
}
fn list_response_meta(result: &UnifiedListResult) -> ExtListResponseMeta {
    ExtListResponseMeta {
        facets: result.facets.clone(),
        partial: PartialInfo {
            conversations: result.conversations_partial.is_some(),
            reason: result.conversations_partial.map(PartialReason::as_str),
        },
        list_scope: result.scope.is_relaxed().then_some(result.scope.as_str()),
    }
}
pub(crate) fn ext_list_response(result: UnifiedListResult) -> ExtListResponse {
    let meta = list_response_meta(&result);
    ExtListResponse {
        sessions: result
            .rows
            .into_iter()
            .map(UnifiedRow::into_ext_superset)
            .collect(),
        next_cursor: result.next_cursor,
        meta,
    }
}
pub(crate) fn acp_response_meta(result: &UnifiedListResult) -> Option<acp::Meta> {
    to_meta(serde_json::to_value(list_response_meta(result)))
}
pub(super) fn to_meta<E: std::fmt::Display>(
    value: Result<serde_json::Value, E>,
) -> Option<acp::Meta> {
    match value {
        Ok(serde_json::Value::Object(map)) => Some(map),
        Ok(other) => {
            tracing::warn!(kind = ?other, "session list _meta was not an object");
            None
        }
        Err(e) => {
            tracing::warn!(error = %e, "session list _meta failed to serialize");
            None
        }
    }
}
