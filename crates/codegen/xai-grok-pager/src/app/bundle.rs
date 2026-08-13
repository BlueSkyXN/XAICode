//! Local catalog state shared by the pager welcome and agent views.
//!
//! Hosted bundle status and entry response DTOs were removed with the hosted
//! account surface.  These small types remain because local/project persona
//! and role editing still uses the same view model.

use serde::Deserialize;

/// Pager-local snapshot of catalog entries available to local views.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BundleState {
    pub has_cache: bool,
    pub version: String,
    pub personas: Vec<String>,
    pub roles: Vec<String>,
    pub agents: Vec<String>,
    pub skills: Vec<String>,
    pub persona_details: Vec<PersonaDetail>,
    pub role_details: Vec<RoleDetail>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PersonaDetail {
    pub name: String,
    pub description: Option<String>,
    pub has_inputs: bool,
    pub has_outputs: bool,
    /// Absolute path when the persona was loaded from disk (user/project).
    #[serde(default)]
    pub source_path: Option<String>,
    /// `user` or `project` for local personas.
    #[serde(default)]
    pub scope_label: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RoleDetail {
    pub name: String,
    pub description: String,
}
