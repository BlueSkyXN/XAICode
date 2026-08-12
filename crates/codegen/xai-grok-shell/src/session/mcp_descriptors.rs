//! On-disk descriptors for explicitly configured local MCP servers.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::session::mcp_servers::{McpClient, sanitize_descriptor_segment};

/// Per-server descriptor folder: `<mcps_root>/<sanitized server name>`.
pub(crate) fn server_descriptor_dir(mcps_root: &Path, server_name: &str) -> PathBuf {
    mcps_root.join(sanitize_descriptor_segment(server_name))
}

/// Materialize descriptors for connected local MCP clients. Errors are logged
/// and do not affect the live MCP session.
pub(crate) async fn materialize_descriptors_for_clients(
    mcps_root: &Path,
    clients: Vec<(String, Arc<McpClient>)>,
) {
    for (name, client) in clients {
        let server_dir = server_descriptor_dir(mcps_root, &name);
        if let Err(error) = client.materialize_descriptors(&server_dir).await {
            tracing::warn!(
                server = %name,
                path = %server_dir.display(),
                %error,
                "failed to materialize MCP descriptors",
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_replaces_unsafe_chars_and_never_empty() {
        assert_eq!(
            sanitize_descriptor_segment("grok_com_linear"),
            "grok_com_linear"
        );
        assert_eq!(sanitize_descriptor_segment("a/b:c d"), "a_b_c_d");
        assert_eq!(sanitize_descriptor_segment(""), "_");
        assert_eq!(sanitize_descriptor_segment("keep-1.2_x"), "keep-1.2_x");
    }

    #[test]
    fn server_dir_is_joined_under_root() {
        let root = Path::new("/home/u/.grok/projects/enc/mcps");
        assert_eq!(server_descriptor_dir(root, "vercel"), root.join("vercel"));
    }
}
