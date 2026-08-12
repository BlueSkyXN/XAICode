use std::collections::BTreeMap;

use serde::Serialize;

use super::envelope::{FacetMap, FacetValue, SessionKind};
use super::row::UnifiedRow;
use crate::session::merge::MergedSession;

pub const KIND_FACET_KEY: &str = "kind";
pub const CWD_FACET_KEY: &str = "cwd";
pub const WORKSPACE_FACET_KEY: &str = "workspace";
pub const STARRED_FACET_KEY: &str = "starred";
pub const REPO_FACET_KEY: &str = "repo";
pub const BRANCH_FACET_KEY: &str = "branch";
pub const WORKTREE_FACET_KEY: &str = "worktree";
pub const GIT_ROOT_FACET_KEY: &str = "gitRoot";
pub const SOURCE_WORKSPACE_FACET_KEY: &str = "sourceWorkspace";

#[derive(Debug, Clone)]
pub struct NormalizedItem {
    pub kind: SessionKind,
    pub cwd: String,
    pub repo_name: Option<String>,
    pub branch: Option<String>,
    pub worktree_label: Option<String>,
    pub git_root_dir: Option<String>,
    pub source_workspace_dir: Option<String>,
    pub workspace_ids: Vec<String>,
    pub starred: bool,
}

impl NormalizedItem {
    pub(crate) fn from_merged(m: &MergedSession) -> Self {
        Self {
            kind: SessionKind::Build,
            cwd: m.cwd.clone(),
            repo_name: m.repo_name.clone(),
            branch: m.branch.clone(),
            worktree_label: m.worktree_label.clone(),
            git_root_dir: m.git_root_dir.clone(),
            source_workspace_dir: m.source_workspace_dir.clone(),
            workspace_ids: Vec::new(),
            starred: false,
        }
    }
}

#[derive(Debug, Default)]
pub struct SourceQuery {
    pub workspace_id: Option<String>,
}

pub enum Pushdown {
    Applied,
    NotSupported,
}

pub trait FacetProvider: Send + Sync {
    fn key(&self) -> &'static str;

    fn applies_to(&self) -> &'static [SessionKind];

    fn extract(&self, item: &NormalizedItem) -> Option<FacetValue>;

    fn pushdown(&self, _filter: &[serde_json::Value], _query: &mut SourceQuery) -> Pushdown {
        Pushdown::NotSupported
    }
}

pub struct KindFacet;

impl FacetProvider for KindFacet {
    fn key(&self) -> &'static str {
        KIND_FACET_KEY
    }
    fn applies_to(&self) -> &'static [SessionKind] {
        &[SessionKind::Build, SessionKind::Chat]
    }
    fn extract(&self, item: &NormalizedItem) -> Option<FacetValue> {
        Some(FacetValue::One(serde_json::Value::String(
            item.kind.as_str().to_owned(),
        )))
    }
}

pub struct CwdFacet;

impl FacetProvider for CwdFacet {
    fn key(&self) -> &'static str {
        CWD_FACET_KEY
    }
    fn applies_to(&self) -> &'static [SessionKind] {
        &[SessionKind::Build]
    }
    fn extract(&self, item: &NormalizedItem) -> Option<FacetValue> {
        if item.cwd.is_empty() {
            None
        } else {
            Some(FacetValue::One(serde_json::Value::String(item.cwd.clone())))
        }
    }
}

pub struct WorkspaceFacet;

impl FacetProvider for WorkspaceFacet {
    fn key(&self) -> &'static str {
        WORKSPACE_FACET_KEY
    }
    fn applies_to(&self) -> &'static [SessionKind] {
        &[SessionKind::Chat]
    }
    fn extract(&self, item: &NormalizedItem) -> Option<FacetValue> {
        if item.workspace_ids.is_empty() {
            None
        } else {
            Some(FacetValue::Many(
                item.workspace_ids
                    .iter()
                    .cloned()
                    .map(serde_json::Value::String)
                    .collect(),
            ))
        }
    }
    fn pushdown(&self, filter: &[serde_json::Value], query: &mut SourceQuery) -> Pushdown {
        if let [only] = filter
            && let Some(workspace_id) = only.as_str()
        {
            query.workspace_id = Some(workspace_id.to_owned());
            return Pushdown::Applied;
        }
        Pushdown::NotSupported
    }
}

pub struct StarredFacet;

impl FacetProvider for StarredFacet {
    fn key(&self) -> &'static str {
        STARRED_FACET_KEY
    }
    fn applies_to(&self) -> &'static [SessionKind] {
        &[SessionKind::Chat]
    }
    fn extract(&self, item: &NormalizedItem) -> Option<FacetValue> {
        item.starred
            .then(|| FacetValue::One(serde_json::Value::Bool(true)))
    }
}

pub struct RepoFacet;

impl FacetProvider for RepoFacet {
    fn key(&self) -> &'static str {
        REPO_FACET_KEY
    }
    fn applies_to(&self) -> &'static [SessionKind] {
        &[SessionKind::Build]
    }
    fn extract(&self, item: &NormalizedItem) -> Option<FacetValue> {
        string_facet(item.repo_name.as_deref())
    }
}

pub struct BranchFacet;

impl FacetProvider for BranchFacet {
    fn key(&self) -> &'static str {
        BRANCH_FACET_KEY
    }
    fn applies_to(&self) -> &'static [SessionKind] {
        &[SessionKind::Build]
    }
    fn extract(&self, item: &NormalizedItem) -> Option<FacetValue> {
        string_facet(item.branch.as_deref())
    }
}

pub struct WorktreeFacet;

impl FacetProvider for WorktreeFacet {
    fn key(&self) -> &'static str {
        WORKTREE_FACET_KEY
    }
    fn applies_to(&self) -> &'static [SessionKind] {
        &[SessionKind::Build]
    }
    fn extract(&self, item: &NormalizedItem) -> Option<FacetValue> {
        string_facet(item.worktree_label.as_deref())
    }
}

pub struct GitRootFacet;

impl FacetProvider for GitRootFacet {
    fn key(&self) -> &'static str {
        GIT_ROOT_FACET_KEY
    }
    fn applies_to(&self) -> &'static [SessionKind] {
        &[SessionKind::Build]
    }
    fn extract(&self, item: &NormalizedItem) -> Option<FacetValue> {
        string_facet(item.git_root_dir.as_deref())
    }
}

pub struct SourceWorkspaceFacet;

impl FacetProvider for SourceWorkspaceFacet {
    fn key(&self) -> &'static str {
        SOURCE_WORKSPACE_FACET_KEY
    }
    fn applies_to(&self) -> &'static [SessionKind] {
        &[SessionKind::Build]
    }
    fn extract(&self, item: &NormalizedItem) -> Option<FacetValue> {
        string_facet(item.source_workspace_dir.as_deref())
    }
}

fn string_facet(value: Option<&str>) -> Option<FacetValue> {
    value
        .filter(|s| !s.is_empty())
        .map(|s| FacetValue::One(serde_json::Value::String(s.to_owned())))
}

#[derive(Default)]
pub struct FacetRegistry {
    providers: Vec<Box<dyn FacetProvider>>,
}

impl FacetRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(mut self, provider: impl FacetProvider + 'static) -> Self {
        self.providers.push(Box::new(provider));
        self
    }

    pub fn provider(&self, key: &str) -> Option<&dyn FacetProvider> {
        self.providers
            .iter()
            .map(|p| p.as_ref())
            .find(|p| p.key() == key)
    }

    pub(crate) fn extract_all(&self, item: &NormalizedItem) -> FacetMap {
        let mut facets = FacetMap::new();
        for provider in &self.providers {
            if provider.applies_to().contains(&item.kind)
                && let Some(value) = provider.extract(item)
            {
                facets.insert(provider.key().to_owned(), value);
            }
        }
        facets
    }

    pub(crate) fn apply_pushdown(
        &self,
        filters: &BTreeMap<String, Vec<serde_json::Value>>,
        query: &mut SourceQuery,
    ) {
        for (key, allowed) in filters {
            if allowed.is_empty() {
                continue;
            }
            if let Some(provider) = self.provider(key) {
                let _ = provider.pushdown(allowed, query);
            }
        }
    }

    pub fn apply_in_memory_filters(
        &self,
        filters: &BTreeMap<String, Vec<serde_json::Value>>,
        rows: Vec<UnifiedRow>,
    ) -> Vec<UnifiedRow> {
        let active: Vec<(&dyn FacetProvider, &Vec<serde_json::Value>)> = filters
            .iter()
            .filter(|(key, _)| key.as_str() != CWD_FACET_KEY)
            .filter(|(_, allowed)| !allowed.is_empty())
            .filter_map(|(key, allowed)| self.provider(key).map(|p| (p, allowed)))
            .collect();
        if active.is_empty() {
            return rows;
        }
        rows.into_iter()
            .filter(|row| {
                active.iter().all(|(provider, allowed)| {
                    if !provider.applies_to().contains(&row.kind) {
                        return true;
                    }
                    row.facets
                        .get(provider.key())
                        .is_some_and(|value| value.intersects(allowed))
                })
            })
            .collect()
    }

    pub(crate) fn summarize_window(&self, rows: &[UnifiedRow]) -> FacetSummary {
        let mut acc: BTreeMap<String, BTreeMap<String, (serde_json::Value, usize)>> =
            BTreeMap::new();
        for row in rows {
            for (key, value) in &row.facets {
                let bucket = acc.entry(key.clone()).or_default();
                for v in value.values() {
                    let entry = bucket
                        .entry(v.to_string())
                        .or_insert_with(|| (v.clone(), 0));
                    entry.1 += 1;
                }
            }
        }
        let keys = acc
            .into_iter()
            .map(|(key, values)| FacetSummaryKey {
                key,
                values: values
                    .into_values()
                    .map(|(value, count)| FacetSummaryValue {
                        value,
                        label: None,
                        count,
                    })
                    .collect(),
            })
            .collect();
        FacetSummary {
            scope: "window",
            keys,
        }
    }
}

pub fn build_facet_registry() -> FacetRegistry {
    FacetRegistry::new()
        .with(KindFacet)
        .with(CwdFacet)
        .with(WorkspaceFacet)
        .with(StarredFacet)
        .with(RepoFacet)
        .with(BranchFacet)
        .with(WorktreeFacet)
        .with(GitRootFacet)
        .with(SourceWorkspaceFacet)
}

#[derive(Debug, Clone, Serialize)]
pub struct FacetSummary {
    pub scope: &'static str,
    pub keys: Vec<FacetSummaryKey>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FacetSummaryKey {
    pub key: String,
    pub values: Vec<FacetSummaryValue>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FacetSummaryValue {
    pub value: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub count: usize,
}
