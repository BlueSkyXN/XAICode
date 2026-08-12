//! Isolated binary so `grok_home()`'s process-wide OnceLock initializes from
//! our `GROK_HOME`. A lib-test EnvGuard is a no-op if another test already
//! resolved it, and then doctor reads the real ~/.grok.

use std::path::PathBuf;
use std::sync::OnceLock;

fn isolate_home() -> &'static PathBuf {
    static HOME: OnceLock<PathBuf> = OnceLock::new();
    HOME.get_or_init(|| {
        let dir = tempfile::TempDir::new().unwrap().keep();
        let grok = dir.join(".grok");
        std::fs::create_dir_all(&grok).unwrap();
        std::fs::write(grok.join("config.toml"), "").unwrap();
        // SAFETY: this binary's only test; set before any grok_home() call.
        unsafe {
            std::env::set_var("HOME", &dir);
            std::env::set_var("USERPROFILE", &dir);
            std::env::set_var("GROK_HOME", &grok);
        }
        dir
    })
}

#[tokio::test]
async fn run_doctor_isolated_home_does_not_probe_unconfigured_servers() {
    let _home = isolate_home();
    let cwd = tempfile::tempdir().unwrap();

    let report = xai_grok_shell::mcp_doctor::run_doctor(cwd.path(), None).await;
    assert!(
        !report.sources.iter().any(|s| s.path == "grok.com"),
        "doctor must not invent a first-party source: {:?}",
        report.sources
    );
    assert!(
        report.servers.is_empty(),
        "isolated cwd must not probe unconfigured HTTP servers: {:?}",
        report.servers
    );
}
