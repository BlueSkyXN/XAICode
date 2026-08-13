#![allow(
    unused_imports,
    unused_variables,
    unused_mut,
    unreachable_code,
    dead_code
)]
//! Local data collection and data-plane helpers.
//!
//! This crate deliberately contains no upload queue or cloud storage client.
//! The retained modules write local event logs, classify local workspaces, and
//! carry W3C trace context for explicitly configured local/generic transports.
pub mod events;
pub mod trace_context;
pub mod upload_config;
pub mod workspace_classifier;
pub use upload_config::*;
/// Compute SHA256 hash of content as a hex string.
pub fn sha256_hex(content: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(content);
    format!("{:x}", hasher.finalize())
}
/// Compute SHA256 hash of a file by streaming, without loading entire file into memory.
/// If `max_bytes` is set (> 0), only hash up to that many bytes.
pub fn sha256_hex_from_file(
    path: &std::path::Path,
    max_bytes: Option<u64>,
) -> std::io::Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let file = std::fs::File::open(path)?;
    let mut reader: Box<dyn Read> = if let Some(limit) = max_bytes {
        Box::new(file.take(limit))
    } else {
        Box::new(file)
    };
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}
