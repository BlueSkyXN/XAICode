#!/usr/bin/env python3
"""Read-only provenance and clean-contract checks for XAICode maintenance."""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import tomllib
from collections import Counter
from pathlib import Path
from typing import Any


SCHEMA_VERSION = 1
REPO_ROOT = Path(__file__).resolve().parents[1]


class MaintenanceError(RuntimeError):
    """A user-actionable maintenance command failure."""


def run_git(repo: Path, *args: str) -> str:
    command = ["git", "-C", str(repo), *args]
    result = subprocess.run(command, capture_output=True, text=True, check=False)
    if result.returncode != 0:
        detail = result.stderr.strip() or result.stdout.strip() or "unknown git error"
        raise MaintenanceError(f"{' '.join(command)} failed: {detail}")
    return result.stdout


def load_manifest(repo: Path) -> dict[str, Any]:
    path = repo / "UPSTREAM.toml"
    try:
        with path.open("rb") as handle:
            manifest = tomllib.load(handle)
    except FileNotFoundError as error:
        raise MaintenanceError(f"missing maintenance manifest: {path}") from error
    if manifest.get("schema_version") != SCHEMA_VERSION:
        raise MaintenanceError(
            f"unsupported UPSTREAM.toml schema_version={manifest.get('schema_version')!r}"
        )
    return manifest


def resolve_commit(repo: Path, ref: str) -> str:
    return run_git(repo, "rev-parse", "--verify", f"{ref}^{{commit}}").strip()


def read_tree(repo: Path, ref: str) -> dict[str, tuple[str, str]]:
    commit = resolve_commit(repo, ref)
    raw = subprocess.run(
        ["git", "-C", str(repo), "ls-tree", "-r", "-z", commit],
        capture_output=True,
        check=False,
    )
    if raw.returncode != 0:
        detail = raw.stderr.decode(errors="replace").strip() or "unknown git error"
        raise MaintenanceError(f"git ls-tree failed for {repo}@{ref}: {detail}")

    entries: dict[str, tuple[str, str]] = {}
    for record in raw.stdout.split(b"\0"):
        if not record:
            continue
        metadata, path_bytes = record.split(b"\t", 1)
        mode, _object_type, object_id = metadata.decode("ascii").split()
        path = path_bytes.decode("utf-8", errors="surrogateescape")
        entries[path] = (mode, object_id)
    return entries


def changed_paths(
    before: dict[str, tuple[str, str]], after: dict[str, tuple[str, str]]
) -> set[str]:
    return {path for path in before.keys() | after.keys() if before.get(path) != after.get(path)}


def change_state(
    before: dict[str, tuple[str, str]],
    after: dict[str, tuple[str, str]],
    path: str,
) -> str:
    if path not in before:
        return "added"
    if path not in after:
        return "deleted"
    return "modified"


def state_counts(
    before: dict[str, tuple[str, str]],
    after: dict[str, tuple[str, str]],
    paths: set[str],
) -> dict[str, int]:
    counts = Counter(change_state(before, after, path) for path in paths)
    return {state: counts.get(state, 0) for state in ("added", "modified", "deleted")}


def area_for(path: str) -> str:
    parts = path.split("/")
    return "/".join(parts[:4]) if len(parts) >= 4 else path


def audit_upstream(args: argparse.Namespace) -> int:
    repo = args.repo.resolve()
    upstream_repo = (args.upstream_repo or repo.parent / "grok-build").resolve()
    manifest = load_manifest(repo)
    configured_base = manifest["upstream"]["git_commit"]
    configured_target = manifest.get("migration", {}).get("target_git_commit")
    if not configured_target:
        configured_target = manifest.get("latest_observed", {}).get("git_commit")
    base_ref = args.base or configured_base
    target_ref = args.target or configured_target
    if not target_ref:
        raise MaintenanceError(
            "no --target supplied and UPSTREAM.toml has no migration or latest-observed target"
        )

    base_commit = resolve_commit(upstream_repo, base_ref)
    target_commit = resolve_commit(upstream_repo, target_ref)
    xaicode_commit = resolve_commit(repo, args.xaicode_ref)

    base_tree = read_tree(upstream_repo, base_commit)
    target_tree = read_tree(upstream_repo, target_commit)
    xaicode_tree = read_tree(repo, xaicode_commit)

    overlay = changed_paths(base_tree, xaicode_tree)
    upstream_delta = changed_paths(base_tree, target_tree)
    overlap = overlay & upstream_delta
    overlay_only = overlay - upstream_delta
    upstream_only = upstream_delta - overlay
    areas = Counter(area_for(path) for path in overlap)

    report: dict[str, Any] = {
        "schema_version": SCHEMA_VERSION,
        "repositories": {
            "xaicode": str(repo),
            "upstream": str(upstream_repo),
        },
        "refs": {
            "base": base_commit,
            "target": target_commit,
            "xaicode": xaicode_commit,
        },
        "file_counts": {
            "base": len(base_tree),
            "target": len(target_tree),
            "xaicode": len(xaicode_tree),
        },
        "changed_path_counts": {
            "xaicode_overlay": len(overlay),
            "upstream_delta": len(upstream_delta),
            "overlap": len(overlap),
            "xaicode_only": len(overlay_only),
            "upstream_only": len(upstream_only),
        },
        "change_states": {
            "xaicode_overlay": state_counts(base_tree, xaicode_tree, overlay),
            "upstream_delta": state_counts(base_tree, target_tree, upstream_delta),
        },
        "overlap_by_area": [
            {"area": area, "count": count}
            for area, count in sorted(areas.items(), key=lambda item: (-item[1], item[0]))
        ],
        "xaicode_worktree_dirty": bool(run_git(repo, "status", "--porcelain")),
    }
    if args.list_paths:
        report["overlap_paths"] = sorted(overlap)

    if args.format == "json":
        print(json.dumps(report, indent=2, sort_keys=True))
        return 0

    counts = report["changed_path_counts"]
    print("XAICode upstream delta audit")
    print(f"base:     {base_commit} ({len(base_tree)} files)")
    print(f"target:   {target_commit} ({len(target_tree)} files)")
    print(f"xaicode:  {xaicode_commit} ({len(xaicode_tree)} files)")
    print(f"overlay:  {counts['xaicode_overlay']} changed paths")
    print(f"upstream: {counts['upstream_delta']} changed paths")
    print(f"overlap:  {counts['overlap']} changed paths")
    print(f"only:     xaicode={counts['xaicode_only']} upstream={counts['upstream_only']}")
    print(f"dirty:    {str(report['xaicode_worktree_dirty']).lower()} (audit uses committed refs)")
    print("overlap by area:")
    for item in report["overlap_by_area"]:
        print(f"  {item['count']:>4}  {item['area']}")
    if args.list_paths:
        print("overlap paths:")
        for path in report["overlap_paths"]:
            print(f"  {path}")
    return 0


def read_toml(path: Path) -> dict[str, Any]:
    try:
        with path.open("rb") as handle:
            return tomllib.load(handle)
    except FileNotFoundError as error:
        raise MaintenanceError(f"missing required file: {path}") from error


def check_contract(args: argparse.Namespace) -> int:
    repo = args.repo.resolve()
    manifest = load_manifest(repo)
    upstream = manifest["upstream"]
    latest_observed = manifest.get("latest_observed", {})
    migration = manifest.get("migration", {})
    product = manifest["product"]
    failures: list[str] = []
    checks = 0

    def expect(condition: bool, message: str) -> None:
        nonlocal checks
        checks += 1
        if not condition:
            failures.append(message)

    source_rev = (repo / "SOURCE_REV").read_text(encoding="utf-8").strip()
    expect(source_rev == upstream["source_rev"], "SOURCE_REV differs from UPSTREAM.toml")

    observed_fields = (
        "checked_at",
        "git_commit",
        "commit_time",
        "source_rev",
        "crate_version",
        "npm_version",
        "npm_published_at",
        "npm_git_head",
        "npm_source_mapping",
    )
    expect(
        isinstance(latest_observed, dict)
        and all(
            isinstance(latest_observed.get(field), str)
            and bool(latest_observed[field].strip())
            for field in observed_fields
        ),
        "latest_observed metadata is incomplete",
    )
    observed_shas = (
        latest_observed.get("git_commit", ""),
        latest_observed.get("source_rev", ""),
        latest_observed.get("npm_git_head", ""),
    )
    expect(
        all(
            len(value) == 40 and all(char in "0123456789abcdef" for char in value)
            for value in observed_shas
        ),
        "latest_observed commit identifiers must be full lowercase SHA-1 values",
    )
    expect(
        latest_observed.get("npm_source_mapping") in {"exact", "version-only", "unmapped"},
        "latest_observed npm_source_mapping is unsupported",
    )

    migration_fields = (
        "status",
        "target_git_commit",
        "product_version",
        "branch",
        "strategy",
    )
    expect(
        isinstance(migration, dict)
        and all(
            isinstance(migration.get(field), str) and bool(migration[field].strip())
            for field in migration_fields
        ),
        "migration metadata is incomplete",
    )
    expect(
        migration.get("status")
        in {"awaiting-authorization", "in-progress", "candidate", "complete", "deferred"},
        "migration status is unsupported",
    )
    migration_target = migration.get("target_git_commit", "")
    expect(
        len(migration_target) == 40
        and all(char in "0123456789abcdef" for char in migration_target),
        "migration target_git_commit must be a full lowercase SHA-1 value",
    )
    expect(
        migration.get("branch", "").startswith("codex/"),
        "migration branch must use the codex/ prefix",
    )
    expect(
        migration.get("product_version") == product.get("version"),
        "migration product_version differs from product version",
    )
    if migration.get("status") in {"candidate", "complete"}:
        expect(
            migration_target == upstream.get("git_commit"),
            "candidate/complete migration target differs from integrated upstream commit",
        )

    migration_plan = (repo / "UPSTREAM_MIGRATION.md").read_text(encoding="utf-8")
    expect(upstream["git_commit"] in migration_plan, "migration plan does not record the base")
    expect(migration_target in migration_plan, "migration plan does not record the target")
    expect(
        migration.get("status", "") in migration_plan,
        "migration plan status differs from UPSTREAM.toml",
    )

    version_toml = read_toml(repo / "crates/codegen/xai-grok-version/Cargo.toml")
    expect(
        version_toml.get("package", {}).get("version") == upstream["crate_version"],
        "xai-grok-version does not match UPSTREAM.toml crate_version",
    )

    for document in ("README.md", "CLEAN_BUILD.md"):
        text = (repo / document).read_text(encoding="utf-8")
        expect(upstream["git_commit"] in text, f"{document} does not record upstream git_commit")
        expect("`coding-agent`" not in text, f"{document} still documents coding-agent")

    product_docs = [
        repo / "crates/codegen/xai-grok-pager/README.md",
        repo / "crates/codegen/xai-grok-shell/README.md",
        repo / "crates/codegen/xai-grok-pager/src/app/cli.rs",
        *(repo / "crates/codegen/xai-grok-pager/docs").rglob("*.md"),
    ]
    forbidden_command = re.compile(r"(?m)^\s*(?:\$\s+)?grok(?:\s|$)|`grok(?:\s|`)")
    for path in product_docs:
        text = path.read_text(encoding="utf-8")
        relative_path = path.relative_to(repo)
        expect(
            "`coding-agent`" not in text,
            f"{relative_path} still documents coding-agent as a command",
        )
        expect(
            forbidden_command.search(text) is None,
            f"{relative_path} still documents the forbidden grok binary",
        )

    package_toml = read_toml(repo / "crates/codegen/xai-grok-pager-bin/Cargo.toml")
    package = package_toml.get("package", {})
    actual_bins = [entry.get("name") for entry in package_toml.get("bin", [])]
    expected_bins = [product["primary_binary"], *product["compatibility_binaries"]]
    expect(package.get("name") == product["package"], "composition package name drifted")
    expect(package.get("version") == product["version"], "composition package version drifted")
    expect(package.get("default-run") == product["primary_binary"], "default-run drifted")
    expect(actual_bins == expected_bins, f"declared bins {actual_bins!r} != {expected_bins!r}")
    expect(
        not set(actual_bins) & set(product["forbidden_binaries"]),
        "a forbidden binary is declared",
    )

    lock_toml = read_toml(repo / "Cargo.lock")
    locked_product = [
        entry for entry in lock_toml.get("package", []) if entry.get("name") == product["package"]
    ]
    expect(len(locked_product) == 1, "Cargo.lock must contain exactly one xaicode package")
    if len(locked_product) == 1:
        expect(locked_product[0].get("version") == product["version"], "Cargo.lock version drifted")

    root_toml = read_toml(repo / "Cargo.toml")
    members = set(root_toml.get("workspace", {}).get("members", []))
    expect(
        "crates/codegen/xai-grok-update" not in members,
        "upstream xai-grok-update is present in workspace members",
    )

    tracked = set(filter(None, run_git(repo, "ls-files", "-z").split("\0")))
    forbidden_prefixes = (
        "crates/codegen/xai-grok-update/",
        "crates/codegen/xai-grok-pager/npm/",
    )
    forbidden_tracked = sorted(
        path for path in tracked if any(path.startswith(prefix) for prefix in forbidden_prefixes)
    )
    expect(not forbidden_tracked, f"vendor updater/npm files are tracked: {forbidden_tracked[:5]}")

    # Telemetry boundary: the clean tree retains local diagnostics, W3C trace
    # context, and a generic customer OTLP compatibility/test surface, but
    # production activation remains inert until separately authorized.
    telemetry_root = repo / "crates/codegen/xai-grok-telemetry"
    for removed in (
        "src/client.rs",
        "src/http.rs",
        "src/sentry.rs",
        "src/otel_layer",
    ):
        expect(
            not (telemetry_root / removed).exists(),
            f"removed hosted telemetry path is present: {removed}",
        )
    telemetry_manifest = (telemetry_root / "Cargo.toml").read_text(encoding="utf-8")
    expect("xai-mixpanel" not in telemetry_manifest, "telemetry still depends on xai-mixpanel")
    expect(
        "xai-mixpanel" not in root_toml.get("workspace", {}).get("dependencies", {}),
        "workspace dependencies still expose xai-mixpanel",
    )
    expect(
        not any(
            str(entry.get("name", "")).startswith("sentry")
            for entry in lock_toml.get("package", [])
        ),
        "Cargo.lock still carries the deleted Sentry vendor graph",
    )
    telemetry_sources = "\n".join(
        path.read_text(encoding="utf-8")
        for path in telemetry_root.rglob("*.rs")
    )
    for forbidden_marker in (
        "xai_grok_telemetry::client",
        "xai_grok_telemetry::http",
        "xai_grok_telemetry::otel_layer",
        "xai_grok_telemetry::sentry",
        "X-XAI-Token-Auth",
        "x-grok-agent-id",
        "x-userid",
        "x-teamid",
        "user_id",
        "team_id",
        "organization_id",
        "principal",
    ):
        expect(
            forbidden_marker not in telemetry_sources,
            f"telemetry source still contains hosted identity/transport marker: {forbidden_marker}",
        )
    shell_sources = "\n".join(
        path.read_text(encoding="utf-8")
        for path in (repo / "crates/codegen/xai-grok-shell/src").rglob("*.rs")
    )
    for forbidden_marker in (
        "xai_grok_telemetry::id::agent_id",
        "xai_grok_telemetry::id::agent_instance_id",
        "xai_grok_telemetry::external::set_identity",
        "xai_grok_telemetry::external::IdentityAttrs",
        "apply_external_otel_remote_policy",
    ):
        expect(
            forbidden_marker not in shell_sources,
            f"shell still injects hosted telemetry identity/policy marker: {forbidden_marker}",
        )

    # The remote/client module is now only a provider-neutral model-catalog
    # seam plus fail-closed compatibility entries.  Keep the old public
    # carrier names and parser behavior visible, while preventing a hosted
    # session client, identity header builder, or session URL constructor
    # from returning in a later source refresh.
    remote_client = (
        repo / "crates/codegen/xai-grok-shell/src/remote/client.rs"
    ).read_text(encoding="utf-8")
    for forbidden_marker in (
        "BackendClient",
        "ShareResponse",
        "LoadDataResponse",
        "LoadedMessage",
        "SessionInfo",
        "SaveDataRequest",
        "UpsertSessionRequest",
        "SessionUpdate",
        "auth_header_map",
        "send_with_auth",
        "with_base_url",
        "ClientWithMiddleware",
        "RequestBuilder",
        "reqwest::",
        "GROK_CODE_BACKEND_URL",
        "GROK_CODE_WEB_URL",
        "X-XAI-Token-Auth",
        "x-userid",
        "x-email",
        "x-grok-client-identifier",
        "x-grok-client-version",
        "BackendError::Auth",
        "/sessions",
        "/sessions/",
    ):
        expect(
            forbidden_marker not in remote_client,
            f"hosted remote-client residue returned: {forbidden_marker}",
        )
    for preserved_marker in (
        "pub enum BackendError",
        "Disabled,",
        "pub fn fetch_settings_blocking(",
        "SettingsFetch::Retry",
        "pub(crate) fn fetch_models_blocking(",
        "pub struct FetchModelsResult",
        "pub(crate) fn models_list_url(",
        "pub(crate) fn parse_remote_model_value(",
        "ListModelsEndpoint::from_endpoints",
    ):
        expect(
            preserved_marker in remote_client,
            f"provider-neutral remote model seam is missing: {preserved_marker}",
        )
    shell_manifest = (
        repo / "crates/codegen/xai-grok-shell/Cargo.toml"
    ).read_text(encoding="utf-8")
    for removed_dependency in ("reqwest-middleware", "xai-grok-extra-ca"):
        expect(
            removed_dependency not in shell_manifest,
            f"retired remote-client dependency remains in shell manifest: {removed_dependency}",
        )
    shell_locked = [
        entry for entry in lock_toml.get("package", []) if entry.get("name") == "xai-grok-shell"
    ]
    expect(len(shell_locked) == 1, "Cargo.lock must contain exactly one xai-grok-shell package")
    if len(shell_locked) == 1:
        shell_dependencies = set(shell_locked[0].get("dependencies", []))
        for removed_dependency in ("reqwest-middleware 0.4.2", "xai-grok-extra-ca"):
            expect(
                removed_dependency not in shell_dependencies,
                f"Cargo.lock still gives xai-grok-shell a retired remote-client edge: {removed_dependency}",
            )

    # Generic model providers remain the source of connection/auth behavior;
    # their TOML carriers and merge parser must not be mistaken for hosted
    # identity plumbing removed above.
    model_providers = (
        repo / "crates/codegen/xai-grok-shell/src/agent/model_providers.rs"
    ).read_text(encoding="utf-8")
    for preserved_marker in (
        "pub struct ModelProviderConfig",
        "pub base_url: Option<String>",
        "pub api_base_url: Option<String>",
        "pub env_key: Option<EnvKeys>",
        "pub api_key: Option<String>",
        "pub api_backend: Option<ApiBackend>",
        "pub extra_headers: IndexMap<String, String>",
        "pub query_params: IndexMap<String, String>",
        "pub env_http_headers: IndexMap<String, String>",
        "pub auth_provider: Option<String>",
        "pub auth: Option<crate::auth::AuthProviderConfig>",
        "pub(crate) fn parse_model_providers(",
        "with_provider_defaults(",
    ):
        expect(
            preserved_marker in model_providers,
            f"generic model-provider field/parser is missing: {preserved_marker}",
        )
    external_config = (
        telemetry_root / "src/external/config.rs"
    ).read_text(encoding="utf-8")
    external_mod = (telemetry_root / "src/external/mod.rs").read_text(encoding="utf-8")
    telemetry_config = (telemetry_root / "src/config.rs").read_text(encoding="utf-8")
    expect(
        'pub const ENV_MASTER_SWITCH: &str = "GROK_EXTERNAL_OTEL";' in external_config,
        "stable generic external OTLP master switch is missing",
    )
    expect(
        'pub const ENV_MASTER_SWITCH_ALIAS: &str = "XAICODE_EXTERNAL_OTEL";' in external_config,
        "XAICode external OTLP compatibility alias is missing",
    )
    for preserved_marker in (
        "pub struct ExternalOtelFileConfig",
        "pub enabled: Option<bool>",
        "pub metrics_exporter: Option<String>",
        "pub logs_exporter: Option<String>",
        "pub endpoint: Option<String>",
        "pub protocol: Option<String>",
    ):
        expect(
            preserved_marker in external_config,
            f"external OTLP config compatibility marker is missing: {preserved_marker}",
        )
    expect(
        '#[serde(alias = "otel_transport")]' in telemetry_config,
        "telemetry config transport alias compatibility marker is missing",
    )
    expect(
        "if !cfg!(test)" in external_config and "return None;" in external_config,
        "external OTLP config resolution lost its production-inert guard",
    )
    expect(
        "if !cfg!(test)" in external_mod and "let _ = cfg;" in external_mod,
        "external OTLP init lost its production-inert embedder guard",
    )
    shell_config = (
        repo / "crates/codegen/xai-grok-shell/src/agent/config.rs"
    ).read_text(encoding="utf-8")
    expect(
        "ExternalOtelFileConfig" in shell_config
        and "get(\"otel_enabled\")" in shell_config
        and "load_effective_config" in shell_config,
        "local telemetry table layering/roundtrip path is missing",
    )
    expect(
        "apply_external_otel_remote_policy" not in shell_config,
        "remote OTLP policy consumer has returned",
    )
    expect(
        "pub fn resolve_external_otel_config(" in shell_config
        and "if !cfg!(test)" in shell_config
        and "let _ = client;" in shell_config,
        "shell external OTLP resolver lost its production-inert guard",
    )
    expect(
        "external_otel_disabled" not in shell_config
        and "external_otel_content_gates_locked" not in shell_config,
        "remote OTLP policy fields are consumed by the shell runtime",
    )
    pager_bin = (
        repo / "crates/codegen/xai-grok-pager-bin/src/main.rs"
    ).read_text(encoding="utf-8")
    expect(
        "xai_grok_telemetry::external::init" in pager_bin
        and "xai_grok_telemetry::external::shutdown" in pager_bin,
        "pager-bin headless/CLI composition lost generic OTLP init/shutdown",
    )
    pager_app = (
        repo / "crates/codegen/xai-grok-pager/src/app/mod.rs"
    ).read_text(encoding="utf-8")
    expect(
        "xai_grok_telemetry::external::init" in pager_app
        and "xai_grok_telemetry::external::shutdown" in pager_app,
        "pager TUI composition lost generic OTLP init/shutdown",
    )
    expect(
        (repo / "crates/codegen/xai-file-utils/src/trace_context.rs").exists(),
        "local W3C trace-context implementation is missing",
    )
    trace_context = (
        repo / "crates/codegen/xai-file-utils/src/trace_context.rs"
    ).read_text(encoding="utf-8")
    expect("traceparent" in trace_context, "W3C traceparent context support is missing")
    usage_source = (
        repo / "crates/codegen/xai-grok-shell/src/extensions/usage.rs"
    ).read_text(encoding="utf-8")
    expect(
        "modelUsage" in usage_source and "record_main_loop_call" in usage_source,
        "local usage/token accounting preservation marker is missing",
    )

    # Hosted uploads, voice/STT, and first-party image/video generation are
    # source-absent. Their config carriers stay parseable so existing
    # config.toml files round-trip without regrowing a network consumer.
    removed_media_paths = (
        "crates/codegen/xai-file-utils/src/circuit_breaker_observer.rs",
        "crates/codegen/xai-file-utils/src/gcs.rs",
        "crates/codegen/xai-file-utils/src/queue.rs",
        "crates/codegen/xai-file-utils/src/s3.rs",
        "crates/codegen/xai-file-utils/src/storage_client.rs",
        "crates/codegen/xai-grok-shell/src/agent/feedback_client.rs",
        "crates/codegen/xai-grok-shell/src/remote/agent.rs",
        "crates/codegen/xai-grok-shell/src/session/acp_session_tests/media_gen_auth_retry_tests.rs",
        "crates/codegen/xai-grok-shell/src/upload",
        "crates/codegen/xai-grok-workspace/src/recovery.rs",
        "crates/codegen/xai-grok-workspace/src/upload",
        "crates/codegen/xai-grok-voice/src/audio",
        "crates/codegen/xai-grok-voice/src/auth.rs",
        "crates/codegen/xai-grok-voice/src/pipeline.rs",
        "crates/codegen/xai-grok-voice/src/probe.rs",
        "crates/codegen/xai-grok-voice/src/stt",
        "crates/codegen/xai-grok-pager/src/slash/commands/imagine.rs",
        "crates/codegen/xai-grok-pager/src/slash/commands/imagine_video.rs",
        "crates/codegen/xai-grok-pager/src/slash/commands/voice.rs",
        "crates/codegen/xai-grok-tools/src/implementations/grok_build/image_edit",
    )
    for relative_path in removed_media_paths:
        expect(
            not (repo / relative_path).exists(),
            f"removed hosted upload/media source still exists: {relative_path}",
        )
    http_source = (repo / "crates/codegen/xai-grok-http/src/lib.rs").read_text(
        encoding="utf-8"
    )
    expect(
        "shared_upload_client" not in http_source,
        "hosted upload-specific HTTP client remains in the generic HTTP crate",
    )
    locked_names = {str(entry.get("name", "")) for entry in lock_toml.get("package", [])}
    for removed_package in (
        "aws-config",
        "aws-sdk-s3",
        "gcloud-storage",
        "cpal",
        "alsa",
        "alsa-sys",
        "coreaudio-rs",
        "coreaudio-sys",
    ):
        expect(
            removed_package not in locked_names,
            f"Cargo.lock still carries removed upload/audio package: {removed_package}",
        )
    voice_config = (
        repo / "crates/codegen/xai-grok-voice/src/config.rs"
    ).read_text(encoding="utf-8")
    expect(
        "#[serde(default)]" in voice_config
        and "pub struct VoiceConfig" in voice_config
        and 'root.get("voice")' in voice_config,
        "[voice] config compatibility carrier is missing",
    )
    voice_manifest = (
        repo / "crates/codegen/xai-grok-voice/Cargo.toml"
    ).read_text(encoding="utf-8")
    for removed_dependency in ("cpal", "tokio-tungstenite", "xai-tty-utils"):
        expect(
            removed_dependency not in voice_manifest,
            f"voice compatibility crate still links runtime dependency: {removed_dependency}",
        )

    # The pager no longer contains a voice command, dispatch path, capture
    # loop, or overlay. Keep this contract at both the path and symbol level:
    # a future source refresh must not silently reconnect the UI to the
    # compatibility-only xai-grok-voice crate.
    pager_src = repo / "crates/codegen/xai-grok-pager/src"
    pager_sources = "\n".join(
        path.read_text(encoding="utf-8") for path in pager_src.rglob("*.rs")
    )
    expect(
        re.search(r"\bvoice\b", pager_sources, flags=re.IGNORECASE) is None,
        "pager source still contains a voice UI/runtime reference",
    )
    for forbidden_marker in (
        "SharedVoiceAuth",
        "VoiceCommand",
        "VoiceEvent",
        "run_voice_pipeline",
        "AUDIO_SUPPORTED",
        "VoicePromptOverlay",
        "voice_cmd_tx",
        "voice_state",
        "voice_mode_enabled",
        "voice_ui_active",
    ):
        expect(
            forbidden_marker not in pager_sources,
            f"pager voice runtime/UI symbol returned: {forbidden_marker}",
        )
    for relative_path in (
        "crates/codegen/xai-grok-pager/src/voice",
        "crates/codegen/xai-grok-pager/src/app/dispatch/voice.rs",
        "crates/codegen/xai-grok-pager/src/app/dispatch/tests/voice.rs",
        "crates/codegen/xai-grok-pager/src/slash/commands/voice.rs",
        "crates/codegen/xai-grok-pager/tests/voice.rs",
    ):
        expect(
            not (repo / relative_path).exists(),
            f"removed pager voice source still exists: {relative_path}",
        )
    pager_test_root = repo / "crates/codegen/xai-grok-pager/tests"
    pager_test_sources = "\n".join(
        path.read_text(encoding="utf-8") for path in pager_test_root.rglob("*.rs")
    )
    expect(
        re.search(r"\bvoice\b", pager_test_sources, flags=re.IGNORECASE) is None,
        "pager voice-specific integration tests returned",
    )
    voice_lib = (repo / "crates/codegen/xai-grok-voice/src/lib.rs").read_text(
        encoding="utf-8"
    )
    for forbidden_marker in (
        "SharedVoiceAuth",
        "VoiceCommand",
        "VoiceEvent",
        "run_voice_pipeline",
        "AUDIO_SUPPORTED",
    ):
        expect(
            forbidden_marker not in voice_lib,
            f"voice compatibility crate still exports runtime API: {forbidden_marker}",
        )

    # Preserve every config carrier while pruning its consumer. These checks
    # intentionally pin the field names, serde defaults, env key, feature
    # pinning, remote-settings field, and writeback helpers that make old
    # config.toml files parse and round-trip without exposing a voice UI.
    config_mod = (repo / "crates/codegen/xai-grok-shell/src/config/mod.rs").read_text(
        encoding="utf-8"
    )
    config_types = (
        repo / "crates/codegen/xai-grok-config-types/src/lib.rs"
    ).read_text(encoding="utf-8")
    ui_config = (repo / "crates/codegen/xai-grok-shared/src/ui_config.rs").read_text(
        encoding="utf-8"
    )
    settings_writes = (
        repo / "crates/codegen/xai-grok-shell/src/util/config/settings_writes.rs"
    ).read_text(encoding="utf-8")
    for marker in (
        "pub voice_mode: Constrained<bool>",
        "pub voice_mode: Option<bool>",
        "resolve_voice_mode",
        'GROK_VOICE_MODE',
        "load_effective_config",
    ):
        expect(marker in shell_config, f"voice config carrier marker is missing: {marker}")
    expect(
        "pin_feature!(voice_mode)" in config_mod,
        "[features].voice_mode requirement merge carrier is missing",
    )
    expect(
        "pub voice_mode_enabled: Option<bool>" in config_types
        and "#[serde(default)]" in config_types,
        "RemoteSettings voice_mode_enabled serde carrier is missing",
    )
    for marker in (
        "pub voice_capture_mode: Option<String>",
        "pub voice_stt_language: Option<String>",
        "pub voice_keybind_enabled: Option<bool>",
        "voice_capture_mode: None",
        "voice_stt_language: None",
        "voice_keybind_enabled: None",
    ):
        expect(marker in ui_config, f"[ui] voice roundtrip carrier is missing: {marker}")
    for marker in (
        "set_voice_capture_mode",
        "set_voice_stt_language",
        "set_voice_keybind_enabled",
        "cfg.ui.voice_capture_mode",
        "cfg.ui.voice_stt_language",
        "cfg.ui.voice_keybind_enabled",
    ):
        expect(marker in settings_writes, f"voice config writeback carrier is missing: {marker}")
    for marker in (
        "pub stt_ws_path: String",
        "pub language: String",
        "pub sample_rate: u32",
        "pub stt_endpointing_ms: u32",
        "pub stt_interim_results: bool",
        "pub client_identifier: String",
        "pub user_agent: String",
        "#[serde(skip)]",
        'root.get("voice")',
    ):
        expect(marker in voice_config, f"[voice] compatibility field is missing: {marker}")
    for relative_path, marker in (
        (
            "crates/codegen/xai-grok-tools/src/implementations/grok_build/image_gen/mod.rs",
            "pub enum ImageGenConfig",
        ),
        (
            "crates/codegen/xai-grok-tools/src/implementations/grok_build/video_gen/mod.rs",
            "pub enum VideoGenConfig",
        ),
    ):
        text = (repo / relative_path).read_text(encoding="utf-8")
        expect(marker in text, f"media config compatibility carrier is missing: {marker}")
        for runtime_marker in ("reqwest::", ".send().await", "impl Tool for"):
            expect(
                runtime_marker not in text,
                f"hosted media runtime returned in {relative_path}: {runtime_marker}",
            )
    for relative_path in (
        "crates/codegen/xai-file-utils/src/events/mod.rs",
        "crates/codegen/xai-file-utils/src/trace_context.rs",
        "crates/codegen/xai-file-utils/src/workspace_classifier.rs",
        "crates/codegen/xai-grok-pager-render/src/prompt_images.rs",
    ):
        expect(
            (repo / relative_path).is_file(),
            f"local media/event utility was removed with hosted runtime: {relative_path}",
        )
    file_utils_lib = (
        repo / "crates/codegen/xai-file-utils/src/lib.rs"
    ).read_text(encoding="utf-8")
    expect(
        "pub fn sha256_hex(" in file_utils_lib
        and "pub fn sha256_hex_from_file(" in file_utils_lib,
        "local content hashing utilities were removed with hosted runtime",
    )
    for marker in (
        "pub image_gen: Constrained<bool>",
        "pub image_edit: Constrained<bool>",
        "pub video_gen: Constrained<bool>",
        "pub voice_mode: Constrained<bool>",
    ):
        expect(marker in shell_config, f"config.toml compatibility field is missing: {marker}")

    # Streaming trace captures and the heap-profile hosted uploader are
    # source-absent.  The ordinary ACP streaming path, doom-loop accounting,
    # and low-level jemalloc hooks remain load-bearing local behavior.
    trace_capture_paths = (
        "crates/codegen/xai-grok-shell/src/session/streaming_capture.rs",
        "crates/codegen/xai-grok-shell/tests/test_heap_profile_monitor.rs",
        "crates/codegen/xai-grok-shell/src/heap_profile/monitor.rs",
        "crates/codegen/xai-grok-shell/src/agent/mvp_agent/heap_profile.rs",
    )
    for relative_path in trace_capture_paths:
        expect(
            not (repo / relative_path).exists(),
            f"removed trace/heap hosted source still exists: {relative_path}",
        )
    shell_session_sources = "\n".join(
        path.read_text(encoding="utf-8")
        for path in (repo / "crates/codegen/xai-grok-shell/src/session").rglob("*.rs")
    )
    for forbidden_marker in (
        "StreamingTurnCapture",
        "TakeStreamingCapture",
        "STREAMING_CAPTURE_MAX_BYTES",
        "DoomLoopSegmentStamp",
        "stamp_doom_loop",
        "finalize_for_upload",
    ):
        expect(
            forbidden_marker not in shell_session_sources,
            f"streaming trace-capture seam remains in shell session sources: {forbidden_marker}",
        )
    tool_calls = (
        repo / "crates/codegen/xai-grok-shell/src/session/acp_session_impl/tool_calls.rs"
    ).read_text(encoding="utf-8")
    for preserved_marker in (
        "self.send_update(",
        "self.send_thought_chunk(",
        "turn_stream_drained",
        "doom_loop_turn_tally",
        "record_doom_loop_recovery_attempt",
        "record_doom_loop_accepted_after_budget",
    ):
        expect(
            preserved_marker in tool_calls,
            f"local streaming/doom-loop behavior is missing: {preserved_marker}",
        )
    heap_profile = (
        repo / "crates/codegen/xai-grok-shell/src/heap_profile/mod.rs"
    ).read_text(encoding="utf-8")
    for forbidden_marker in (
        "HeapProfileMonitor",
        "HeapProfileUploadHandles",
        "build_upload_handles",
        "UPLOAD_TIMEOUT",
        "upload_pair",
        "SCOPED_KILL_SWITCH_INTERVAL",
    ):
        expect(
            forbidden_marker not in heap_profile,
            f"hosted heap-profile uploader seam returned: {forbidden_marker}",
        )
    for preserved_marker in (
        "pub struct HeapProfileHooks",
        "pub fn stats()",
        "pub fn set_prof_active(",
        "pub fn dump_to_path(",
        "pub fn prof_available()",
        "pub const LG_PROF_SAMPLE",
    ):
        expect(
            preserved_marker in heap_profile,
            f"low-level jemalloc hook is missing: {preserved_marker}",
        )
    mvp_sources = "\n".join(
        path.read_text(encoding="utf-8")
        for path in (repo / "crates/codegen/xai-grok-shell/src/agent/mvp_agent").rglob("*.rs")
    )
    for forbidden_marker in (
        "HeapProfileMonitor",
        "heap_profile_monitor",
        "heap_profile_started",
        "reconfigure_heap_profile_monitor",
    ):
        expect(
            forbidden_marker not in mvp_sources,
            f"removed heap-profile monitor consumer remains: {forbidden_marker}",
        )
    config_types = (
        repo / "crates/codegen/xai-grok-config-types/src/lib.rs"
    ).read_text(encoding="utf-8")
    for preserved_marker in (
        "pub jemalloc_heap_profile_enabled: Option<bool>",
        "pub jemalloc_heap_profile_thresholds_bytes: Option<Vec<u64>>",
        "pub jemalloc_heap_profile_poll_interval_secs: Option<u64>",
    ):
        expect(
            preserved_marker in config_types,
            f"heap-profile config carrier is missing: {preserved_marker}",
        )

    # Test support no longer impersonates the hosted `/v1/storage` upload
    # service.  Inference, settings, and required-auth mocks remain available.
    mock_server = (
        repo / "crates/codegen/xai-grok-test-support/src/mock_server.rs"
    ).read_text(encoding="utf-8")
    for forbidden_marker in (
        "StorageUpload",
        "StorageState",
        "storage_upload_handler",
        "set_storage_unauthorized",
        "storage_request_count",
        "storage_uploads",
        '"/v1/storage"',
    ):
        expect(
            forbidden_marker not in mock_server,
            f"test-support storage mock residue remains: {forbidden_marker}",
        )
    for preserved_marker in (
        '"/v1/chat/completions"',
        '"/v1/responses"',
        '"/v1/messages"',
        '"/v1/settings"',
        "start_with_required_auth",
    ):
        expect(
            preserved_marker in mock_server,
            f"ordinary test-support mock endpoint is missing: {preserved_marker}",
        )
    pty_harness = (
        repo / "crates/codegen/xai-grok-pager-pty-harness/src/content.rs"
    ).read_text(encoding="utf-8")
    for forbidden_marker in (
        "StorageUpload",
        "set_storage_unauthorized",
        "storage_request_count",
        "storage_uploads",
        "/v1/storage",
    ):
        expect(
            forbidden_marker not in pty_harness,
            f"PTY storage mock control remains: {forbidden_marker}",
        )

    required_markers: dict[str, tuple[str, ...]] = {
        "crates/codegen/xai-grok-pager-bin/src/main.rs": (
            "let remote_settings: Option<xai_grok_shell::util::config::RemoteSettings> = None;",
            "let use_leader = false;",
        ),
        "crates/codegen/xai-grok-pager/src/acp/spawn.rs": ("AuthManager::new_local(",),
        "crates/codegen/xai-grok-shell/src/agent/init.rs": (
            "cfg.storage_mode = StorageMode::Local;",
        ),
    }
    for relative_path, markers in required_markers.items():
        text = (repo / relative_path).read_text(encoding="utf-8")
        for marker in markers:
            expect(marker in text, f"missing clean-boundary marker in {relative_path}: {marker}")

    acp_agent = (
        repo / "crates/codegen/xai-grok-shell/src/agent/mvp_agent/acp_agent.rs"
    ).read_text(encoding="utf-8")
    expect(
        "clean_build_extension_disabled" not in acp_agent,
        "hosted ACP compatibility gate remains instead of source-level dispatch removal",
    )
    for forbidden_call in (
        "force_reload_from_disk();",
        "first_party_env_key_allows_advertise(",
    ):
        expect(
            forbidden_call not in acp_agent,
            f"ACP initialization restored forbidden account-auth call: {forbidden_call}",
        )

    auth_manager = (
        repo / "crates/codegen/xai-grok-shell/src/auth/manager.rs"
    ).read_text(encoding="utf-8")
    for preserved_marker in (
        "auth_file_access: bool",
        "grok_home.join(\"auth.json\"),\n            false,",
        "if !self.auth_file_access",
        "refresh_chain_in_memory",
    ):
        expect(
            preserved_marker in auth_manager,
            f"production auth-file no-access marker is missing: {preserved_marker}",
        )
    config_reloader = (
        repo / "crates/codegen/xai-grok-shell/src/config/reloader.rs"
    ).read_text(encoding="utf-8")
    expect(
        "pub(crate) fn reload_auth" in config_reloader
        and "if !cfg!(test)" in config_reloader
        and "return Ok(());" in config_reloader,
        "config reloader lost its production auth-file no-access guard",
    )

    # The clean build intentionally removes the hosted account and extension
    # implementations.  Keep this as a path-level absence contract so a
    # future upstream refresh cannot silently resurrect a WebLogin/OIDC,
    # hosted billing, or hosted dispatch surface.  Generic provider auth and
    # third-party MCP OAuth remain required below.
    removed_source_paths = (
        "crates/codegen/xai-grok-shell/src/auth/device_code.rs",
        "crates/codegen/xai-grok-shell/src/auth/devbox_login_stub.rs",
        "crates/codegen/xai-grok-shell/src/auth/manager/enrichment.rs",
        "crates/codegen/xai-grok-shell/src/auth/manager_tests.rs",
        "crates/codegen/xai-grok-shell/src/auth/oidc/login.rs",
        "crates/codegen/xai-grok-shell/src/auth/oidc/mod.rs",
        "crates/codegen/xai-grok-shell/src/auth/oidc/protocol.rs",
        "crates/codegen/xai-grok-shell/src/auth/oidc/refresh.rs",
        "crates/codegen/xai-grok-shell/src/auth/oidc/test_helpers.rs",
        "crates/codegen/xai-grok-shell/src/auth/refresh/auth_backend_contract_tests.rs",
        "crates/codegen/xai-grok-shell/src/auth/refresh/oidc_refresher.rs",
        "crates/codegen/xai-grok-shell/src/auth/refresh/oidc_refresher_tests.rs",
        "crates/codegen/xai-grok-shell/src/auth/single_flight.rs",
        "crates/codegen/xai-grok-shell/src/agent/subscription_check.rs",
        "crates/codegen/xai-grok-shell/src/extensions/auth.rs",
        "crates/codegen/xai-grok-shell/src/extensions/auth_gate.rs",
        "crates/codegen/xai-grok-shell/src/extensions/billing.rs",
        "crates/codegen/xai-grok-shell/src/extensions/bundle.rs",
        "crates/codegen/xai-grok-shell/src/extensions/feedback.rs",
        "crates/codegen/xai-grok-shell/src/extensions/privacy.rs",
        "crates/codegen/xai-grok-shell/src/extensions/rollout.rs",
        "crates/codegen/xai-grok-shell/src/extensions/share.rs",
        "crates/codegen/xai-grok-shell/src/agent/handlers/workspaces.rs",
        "crates/codegen/xai-grok-pager/src/app/dispatch/billing.rs",
        "crates/codegen/xai-grok-pager/src/app/dispatch/tests/billing.rs",
        "crates/codegen/xai-grok-pager/src/app/subscription.rs",
        "crates/codegen/xai-grok-pager/src/share_cmd.rs",
        "crates/codegen/xai-grok-pager/src/slash/commands/announcements.rs",
        "crates/codegen/xai-grok-pager/src/slash/commands/feedback.rs",
        "crates/codegen/xai-grok-pager/src/slash/commands/share.rs",
    )
    for relative_path in removed_source_paths:
        expect(
            not (repo / relative_path).exists(),
            f"removed hosted/account source still exists: {relative_path}",
        )

    # Announcement/subscription/billing consumers are deliberately removed
    # from the production event graph.  RemoteSettings and the TOML carrier
    # remain below for compatibility, but a future upstream refresh must not
    # silently restore the pager notification route, billing timers, or
    # hosted persistence effects.  Keep this scoped to production roots so
    # retained protocol/config names and historical tests do not weaken the
    # runtime absence contract.
    hosted_runtime_roots = (
        "crates/codegen/xai-grok-pager/src/app/actions.rs",
        "crates/codegen/xai-grok-pager/src/app/effects/mod.rs",
        "crates/codegen/xai-grok-pager/src/app/event_loop.rs",
        "crates/codegen/xai-grok-pager/src/app/acp_handler/mod.rs",
        "crates/codegen/xai-grok-pager/src/app/dispatch/router.rs",
        "crates/codegen/xai-grok-pager/src/app/dispatch/task_result.rs",
        "crates/codegen/xai-grok-pager/src/app/agent_view/input.rs",
        "crates/codegen/xai-grok-pager/src/app/mouse.rs",
        "crates/codegen/xai-grok-shell/src/agent/mvp_agent/agent_ops.rs",
        "crates/codegen/xai-grok-shell/src/leader/server.rs",
    )
    hosted_runtime_forbidden = (
        "FetchBilling",
        "FetchAppBilling",
        "RefreshGate",
        "CheckSubscription",
        "ManageBilling",
        "SchedulePaywallCheck",
        "ScheduleGateVerifyTimeout",
        "CreditLimitRecheck",
        "PersistAnnouncementsHidden",
        "AnnouncementsHiddenPersisted",
        "AnnouncementsOpenCta",
        "AnnouncementsHide",
        "AnnouncementsShow",
        "x.ai/announcements/update",
        "handle_announcements_update",
        "subscription_watch_interval_secs",
    )
    for relative_path in hosted_runtime_roots:
        text = (repo / relative_path).read_text(encoding="utf-8")
        for marker in hosted_runtime_forbidden:
            expect(
                marker not in text,
                f"hosted subscription/announcement runtime marker returned in {relative_path}: {marker}",
            )

    # Remote settings are a wire/config compatibility carrier, not a reason
    # to reintroduce the deleted consumer.  Keep their tolerant serde fields,
    # plus the local TOML merge/env precedence contract, byte-for-byte in the
    # implementation even when no UI reads the hosted values.
    remote_settings = (
        repo / "crates/codegen/xai-grok-config-types/src/lib.rs"
    ).read_text(encoding="utf-8")
    for marker in (
        "pub struct RemoteSettings",
        "pub announcements: Option<Vec<RemoteAnnouncement>>",
        "deserialize_tolerant_announcements",
        "pub subscription_tier: Option<String>",
        "pub usage_billing_redirect_url: Option<String>",
        "#[serde(default)]",
    ):
        expect(marker in remote_settings, f"RemoteSettings compatibility marker is missing: {marker}")
    announcement_config = (
        repo / "crates/codegen/xai-grok-shell/src/util/config/announcements.rs"
    ).read_text(encoding="utf-8")
    for marker in (
        "announcements_from_toml",
        "merge_announcements",
        "announcements_override",
        "resolve_announcements",
        "GROK_ANNOUNCEMENTS_OVERRIDE",
        "Priority: requirements > remote > user config > managed config.",
    ):
        expect(
            marker in announcement_config,
            f"announcements TOML parser/merge compatibility marker is missing: {marker}",
        )

    # Local `/usage` and session-info remain intentionally live.  These
    # markers guard the local token/cost ledger and modal against a later
    # hosted-billing cleanup accidentally removing the preserved data plane.
    usage_sources = {
        relative_path: (repo / relative_path).read_text(encoding="utf-8")
        for relative_path in (
            "crates/codegen/xai-grok-pager/src/app/actions.rs",
            "crates/codegen/xai-grok-pager/src/app/dispatch/status.rs",
            "crates/codegen/xai-grok-pager/src/app/status_blocks.rs",
            "crates/codegen/xai-grok-pager/src/views/usage_modal.rs",
            "crates/codegen/xai-grok-shell/src/extensions/usage.rs",
        )
    }
    expect(
        "FetchSessionUsage" in usage_sources["crates/codegen/xai-grok-pager/src/app/actions.rs"],
        "local session usage action was removed",
    )
    expect(
        "FetchSessionUsage" in usage_sources["crates/codegen/xai-grok-pager/src/app/dispatch/status.rs"]
        and "session_usage_text" in usage_sources["crates/codegen/xai-grok-pager/src/app/dispatch/status.rs"],
        "local session usage dispatch/persistence path was removed",
    )
    usage_modal = usage_sources["crates/codegen/xai-grok-pager/src/views/usage_modal.rs"]
    for marker in ("SessionInfo", "session_usage_text"):
        expect(marker in usage_modal, f"local usage rendering marker is missing: {marker}")
    usage_status = usage_sources["crates/codegen/xai-grok-pager/src/app/status_blocks.rs"]
    for marker in ("Input tokens", "Output tokens", "Total tokens"):
        expect(marker in usage_status, f"local token-count rendering marker is missing: {marker}")
    usage_extension = usage_sources["crates/codegen/xai-grok-shell/src/extensions/usage.rs"]
    for marker in ("x.ai/session/usage", "modelUsage", "record_main_loop_call"):
        expect(marker in usage_extension, f"local usage ledger marker is missing: {marker}")

    # Test coverage is part of the clean contract too.  Hosted-only
    # announcement/CTA assertions may disappear, but a future cleanup must
    # not delete an entire generic test module (links, settings DTO/config,
    # local queue/permission behavior, or shell turn accounting) to silence
    # those stale cases.
    preserved_test_markers: dict[str, tuple[str, ...]] = {
        "crates/codegen/xai-grok-pager/src/app/agent_view/links.rs": (
            "fn drag_clears_pending_link_click()",
            "fn modifier_click_on_file_link_opens_via_our_handler()",
            "fn cycle_forward_from_none_selects_first()",
            "fn vim_slash_opens_scrollback_search()",
        ),
        "crates/codegen/xai-grok-pager/src/app/acp_handler/tests/settings.rs": (
            "fn settings_update_ignores_account_tier()",
            "fn settings_update_clearing_group_tool_verbs_reverts_to_default()",
            "fn auto_gate_killswitch_notifies_agents_to_leave_auto()",
            "fn permission_mode_soft_default_applies_remote_always_approve()",
        ),
        "crates/codegen/xai-grok-pager/src/app/acp_handler/tests/mod.rs": (
            "mod settings;",
            "group_tool_verbs_settings_update",
            "collapsed_edit_blocks_settings_update",
        ),
        "crates/codegen/xai-grok-pager/src/app/dispatch/tests/router.rs": (
            "#[test]\nfn switch_model_dispatch_produces_effect_and_sets_pending()",
        ),
        "crates/codegen/xai-grok-pager/src/app/dispatch/tests/dashboard.rs": (
            "fn dashboard_rename_esc_keystroke_routes_to_cancel()",
        ),
        "crates/codegen/xai-grok-shell/src/agent/mvp_agent/tests.rs": (
            "#[test]\nfn allocate_turn_number_advances_counter()",
            "mod hunk_tracking_mode",
        ),
    }
    for (relative_path, markers) in preserved_test_markers.items():
        path = repo / relative_path
        expect(path.is_file(), f"preserved generic test source is missing: {relative_path}")
        if path.is_file():
            text = path.read_text(encoding="utf-8")
            for marker in markers:
                expect(
                    marker in text,
                    f"preserved generic/local test marker is missing in {relative_path}: {marker}",
                )

    settings_handler = (
        repo / "crates/codegen/xai-grok-pager/src/app/acp_handler/settings.rs"
    ).read_text(encoding="utf-8")
    for marker in (
        "serde_json::from_str::<PagerSettingsUpdate>",
        "sync_permission_mode_slash_gate",
        "resolve_group_tool_verbs",
        "resolve_collapsed_edit_blocks",
        "resolve_tips",
        "resolve_slash_command_tags",
    ):
        expect(marker in settings_handler, f"local settings update path is missing: {marker}")
    for marker in (
        "subscription_tier_display:",
        "gate_message:",
        "campaigns:",
        "voice_mode_enabled:",
        "privacy_notice_rollout:",
    ):
        expect(
            marker not in settings_handler,
            f"hosted settings field returned to pager DTO: {marker}",
        )

    pager_main = (
        repo / "crates/codegen/xai-grok-pager-bin/src/main.rs"
    ).read_text(encoding="utf-8")
    expect(
        "warn_leader_disabled_by_sandbox(profile);\n    }\n    match agent_args.mode" in pager_main,
        "pager-bin local agent dispatch has stale braces after leader removal",
    )
    mvp_tests = (
        repo / "crates/codegen/xai-grok-shell/src/agent/mvp_agent/tests.rs"
    ).read_text(encoding="utf-8")
    expect(
        "    });\n}\nmod direct_hub_cloud_removed" in mvp_tests,
        "interactive trust test is not closed before the local hub guard module",
    )

    for relative_path in (
        "crates/codegen/xai-grok-shell/src/auth/auth_provider.rs",
        "crates/codegen/xai-grok-shell/src/auth/token_output.rs",
        "crates/codegen/xai-grok-shell/src/auth/external_auth.rs",
    ):
        expect(
            (repo / relative_path).is_file(),
            f"generic auth/provider source is missing: {relative_path}",
        )

    # Hosted MCP gateway runtime is intentionally absent. The config/wire
    # carriers below remain present for compatibility, but no production
    # source may instantiate the old catalog/client/support crate.
    removed_mcp_paths = (
        repo / "crates/codegen/xai-grok-shell-session-support/Cargo.toml",
        repo / "crates/codegen/xai-grok-shell-session-support/src/lib.rs",
        repo / "crates/codegen/xai-grok-shell-session-support/src/managed_mcp.rs",
        repo
        / "crates/codegen/xai-grok-shell-session-support/tests/production_gateway_fail_closed.rs",
        repo / "crates/codegen/xai-grok-shell/src/remote/skills_client.rs",
    )
    for path in removed_mcp_paths:
        expect(not path.exists(), f"hosted MCP runtime path still exists: {path.relative_to(repo)}")

    hosted_runtime_paths = (
        repo / "crates/codegen/xai-grok-shell/src/extensions/mcp.rs",
        repo / "crates/codegen/xai-grok-shell/src/session/mcp_sources.rs",
        repo / "crates/codegen/xai-grok-shell/src/session/acp_session_impl/mcp_snapshot.rs",
        repo / "crates/codegen/xai-grok-shell/src/session/mcp_descriptors.rs",
        repo / "crates/codegen/xai-grok-tools/src/implementations/use_tool/mod.rs",
        repo / "crates/codegen/xai-grok-tools/src/types/resources.rs",
    )
    hosted_forbidden = (
        "ManagedGatewayTool",
        "gateway_catalog",
        "managed_mcp_cache",
        "managed_mcp_proxy_base_url",
        "fetch_gateway_catalog",
        "WorkspaceOps::Proxy",
        "SkillsClient",
        "ProductSkills",
    )
    for path in hosted_runtime_paths:
        text = path.read_text(encoding="utf-8")
        for symbol in hosted_forbidden:
            expect(symbol not in text, f"hosted MCP symbol remains in {path.relative_to(repo)}: {symbol}")

    # Computer-hub/workspace hosted runtime is also consumer-first absent. The
    # config and wire carriers remain, but no workspace client, socket runtime,
    # preview proxy, or donation implementation may be source-present.
    hub_removed_paths = (
        repo / "crates/codegen/xai-grok-workspace-client/Cargo.toml",
        repo / "crates/codegen/xai-grok-workspace-client/src/lib.rs",
        repo / "crates/common/xai-computer-hub-mcp-adapter/Cargo.toml",
        repo / "crates/common/xai-computer-hub-mcp-adapter/src/bridge.rs",
        repo / "crates/common/xai-computer-hub-mcp-adapter/src/lib.rs",
        repo / "crates/common/xai-computer-hub-mcp-adapter/src/metrics.rs",
        repo / "crates/common/xai-computer-hub-mcp-adapter/src/transport.rs",
        repo / "crates/common/xai-computer-hub-mcp-adapter/src/types.rs",
        repo / "crates/codegen/xai-grok-workspace/src/hub_auth.rs",
        repo / "crates/codegen/xai-grok-workspace/src/hub_channel.rs",
        repo / "crates/codegen/xai-grok-workspace/src/hub_ids.rs",
        repo / "crates/codegen/xai-grok-workspace/src/hub_server.rs",
        repo / "crates/codegen/xai-grok-workspace/src/mcp.rs",
        repo / "crates/codegen/xai-grok-workspace/src/preview_supervisor.rs",
        repo / "crates/codegen/xai-grok-workspace/src/bin/workspace_server.rs",
        repo / "crates/codegen/xai-grok-workspace/src/bin/workspace_server_probe.rs",
        repo / "crates/common/xai-computer-hub-core/src/remote.rs",
    )
    for path in hub_removed_paths:
        expect(not path.exists(), f"hosted hub runtime path still exists: {path.relative_to(repo)}")

    sdk_removed_paths = (
        "admission.rs",
        "cancel.rs",
        "connection.rs",
        "connection_borrow.rs",
        "demux.rs",
        "donate_pump.rs",
        "handshake.rs",
        "log_donate.rs",
        "metric_donate.rs",
        "metrics.rs",
        "notification.rs",
        "oidc_provider.rs",
        "pool.rs",
        "refcount.rs",
        "server.rs",
        "trace_donate.rs",
    )
    for name in sdk_removed_paths:
        path = repo / "crates/common/xai-computer-hub-sdk/src" / name
        expect(not path.exists(), f"hosted SDK runtime path still exists: {path.relative_to(repo)}")

    sdk_lib = (repo / "crates/common/xai-computer-hub-sdk/src/lib.rs").read_text(encoding="utf-8")
    for marker in ("pub mod protocol;", "LocalRegistry", "ToolHarness", "ToolServerHandler"):
        expect(marker in sdk_lib, f"local computer-hub SDK marker is missing: {marker}")
    protocol = (repo / "crates/common/xai-computer-hub-sdk/src/protocol.rs").read_text(
        encoding="utf-8"
    )
    expect("pub trait ToolServerHandler" in protocol, "local ToolServerHandler seam is missing")
    workspace_ops = (
        repo / "crates/codegen/xai-grok-workspace/src/workspace_ops.rs"
    ).read_text(encoding="utf-8")
    expect("pub enum WorkspaceOps" in workspace_ops, "workspace operations enum is missing")
    expect("Local { handle: WorkspaceHandle }" in workspace_ops, "local workspace ops are missing")
    workspace_source = (
        repo / "crates/codegen/xai-grok-workspace/src/workspace_ops.rs"
    ).read_text(encoding="utf-8")
    for forbidden_symbol in ("WorkspaceOps::Proxy", "WorkspaceClient", "ToolServerBuilder"):
        expect(
            forbidden_symbol not in workspace_source,
            f"hosted workspace symbol remains in workspace_ops.rs: {forbidden_symbol}",
        )

    mcp_sources = (
        repo / "crates/codegen/xai-grok-shell/src/session/mcp_sources.rs"
    ).read_text(encoding="utf-8")
    for marker in (
        "pub(crate) fn merge_mcp_servers(",
        "pub(crate) fn merge_mcp_servers_sourced(",
        "pub(crate) fn collect_plugin_oauth_configs(",
        "pub(crate) fn merge_plugin_oauth_into(",
    ):
        expect(marker in mcp_sources, f"local MCP merge marker is missing: {marker}")

    mcp_extension = (repo / "crates/codegen/xai-grok-shell/src/extensions/mcp.rs").read_text(
        encoding="utf-8"
    )
    expect(
        "pub fn build_mcp_catalog(local_servers: &[acp::McpServer])" in mcp_extension,
        "local MCP catalog builder is missing",
    )
    expect(
        "McpServerSource::Local" in mcp_extension,
        "local MCP source classification is missing",
    )
    pager_mcps = (repo / "crates/codegen/xai-grok-pager/src/views/mcps_modal.rs").read_text(
        encoding="utf-8"
    )
    for marker in (
        "fn section_for_grok_com_local_name_is_local()",
        "fn is_removable_allows_grok_com_local_name()",
    ):
        expect(marker in pager_mcps, f"explicit local grok_com_* coverage is missing: {marker}")
    dispatcher = (
        repo / "crates/codegen/xai-grok-shell/src/session/mcp_dispatcher.rs"
    ).read_text(encoding="utf-8")
    expect(
        "McpServerSource::Local" in dispatcher,
        "MCP status dispatcher no longer classifies configured servers as local",
    )

    # Generic config carriers are deliberately retained even though the
    # hosted consumer is gone; this freezes config.toml parsing/roundtrip.
    for relative_path, markers in {
        "crates/codegen/xai-grok-shell/src/config/mod.rs": (
            "pub struct ManagedMcpsConfig",
            "managed_mcp_gateway_tools_enabled",
        ),
        "crates/codegen/xai-grok-config-types/src/lib.rs": (
            "pub managed_mcps_enabled: Option<bool>",
            "pub managed_mcp_gateway_tools_enabled: Option<bool>",
        ),
    }.items():
        text = (repo / relative_path).read_text(encoding="utf-8")
        for marker in markers:
            expect(marker in text, f"compatibility config carrier missing in {relative_path}: {marker}")

    # Local/third-party MCP transports and OAuth remain first-class.
    for relative_path in (
        "crates/codegen/xai-grok-mcp/src/servers.rs",
        "crates/codegen/xai-grok-mcp/src/oauth.rs",
        "crates/codegen/xai-grok-mcp/src/oauth_config.rs",
    ):
        expect((repo / relative_path).exists(), f"local MCP transport/OAuth module missing: {relative_path}")
    mcp_servers = (repo / "crates/codegen/xai-grok-mcp/src/servers.rs").read_text(encoding="utf-8")
    for marker in ("pub fn new_http(", "McpClient::new_http", "OAuth"):
        expect(marker in mcp_servers, f"local HTTP/OAuth MCP marker missing: {marker}")
    slash_commands = (repo / "crates/codegen/xai-grok-shell/src/session/slash_commands.rs").read_text(
        encoding="utf-8"
    )
    expect(
        "list_skills_with_plugins" in slash_commands,
        "local/plugin skills discovery was removed with hosted skills",
    )

    ci = (repo / ".github/workflows/ci.yml").read_text(encoding="utf-8")
    release = (repo / ".github/workflows/release.yml").read_text(encoding="utf-8")
    dependabot = (repo / ".github/dependabot.yml").read_text(encoding="utf-8")
    toolchain = read_toml(repo / "rust-toolchain.toml")["toolchain"]["channel"]
    for workflow_name, workflow in (("CI", ci), ("release", release)):
        expect(
            f'toolchain: "{toolchain}"' in workflow,
            f"{workflow_name} workflow toolchain differs from rust-toolchain.toml",
        )
        for boundary_test in ("clean_custom_provider", "request_query_and_headers"):
            expect(
                boundary_test in workflow,
                f"{workflow_name} workflow does not run {boundary_test}",
            )
    for obsolete_action in (
        "actions/checkout@v4",
        "actions/setup-python@v5",
        "actions/upload-artifact@v4",
        "actions/download-artifact@v4",
    ):
        expect(
            obsolete_action not in ci and obsolete_action not in release,
            f"workflow still uses obsolete action runtime: {obsolete_action}",
        )
    expect(
        'package-ecosystem: "github-actions"' in dependabot,
        "Dependabot does not maintain GitHub Actions",
    )
    expect(
        'interval: "weekly"' in dependabot,
        "GitHub Actions dependency maintenance is not weekly",
    )
    expect(
        "name: Strip distribution binary" in release,
        "release workflow does not strip the shipped binary",
    )
    expect(
        'strip --strip-all "$BIN"' in release,
        "release workflow does not strip Linux symbols",
    )
    expect(
        'strip -S -x "$BIN"' in release,
        "release workflow does not strip macOS symbols",
    )
    expect(
        'test "$AFTER_SIZE" -lt "$BEFORE_SIZE"' in release,
        "release workflow does not verify that stripping reduced the binary",
    )
    expect("BIN_NAME: xaicode" in release, "release primary binary drifted")
    expect("COMPAT_BIN_NAME: xai-grok-pager" in release, "release does not name compatibility bin")
    expect(
        "--bin ${{ env.COMPAT_BIN_NAME }}" in release,
        "release does not build the compatibility bin",
    )
    expect(
        'COMPAT_BIN="target/release-dist/${{ env.COMPAT_BIN_NAME }}"' in release,
        "release does not smoke the compatibility bin",
    )
    expect(
        "contents: read" in release and release.count("contents: write") == 1,
        "release write permission must be limited to the publishing job",
    )

    if os.environ.get("GITHUB_REF_TYPE") == "tag":
        expected_tag = f"v{product['version']}"
        expect(
            os.environ.get("GITHUB_REF_NAME") == expected_tag,
            f"release tag must be {expected_tag}",
        )

    result = {
        "schema_version": SCHEMA_VERSION,
        "ok": not failures,
        "checks": checks,
        "failures": failures,
    }
    if args.format == "json":
        print(json.dumps(result, indent=2, sort_keys=True))
    elif failures:
        print(f"FAIL clean contract ({len(failures)}/{checks} checks failed)", file=sys.stderr)
        for failure in failures:
            print(f"  - {failure}", file=sys.stderr)
    else:
        print(f"PASS clean contract ({checks} checks)")
    return 1 if failures else 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Audit XAICode upstream provenance and clean-build invariants."
    )
    parser.add_argument(
        "--repo",
        type=Path,
        default=REPO_ROOT,
        help="XAICode repository (default: script parent repository)",
    )
    subparsers = parser.add_subparsers(dest="command", required=True)

    check = subparsers.add_parser("check-contract", help="validate tracked clean-build invariants")
    check.add_argument("--format", choices=("text", "json"), default="text")
    check.set_defaults(handler=check_contract)

    audit = subparsers.add_parser(
        "audit-upstream", help="compare base, XAICode and a local upstream target tree"
    )
    audit.add_argument(
        "--upstream-repo",
        type=Path,
        help="local grok-build repository (default: sibling of --repo)",
    )
    audit.add_argument(
        "--target",
        help="target upstream commit or ref (default: UPSTREAM.toml migration target)",
    )
    audit.add_argument("--base", help="base upstream ref (default: UPSTREAM.toml git_commit)")
    audit.add_argument("--xaicode-ref", default="HEAD", help="committed XAICode ref to compare")
    audit.add_argument("--format", choices=("text", "json"), default="text")
    audit.add_argument("--list-paths", action="store_true", help="include every overlap path")
    audit.set_defaults(handler=audit_upstream)
    return parser


def main() -> int:
    parser = build_parser()
    args = parser.parse_args()
    try:
        return args.handler(args)
    except (MaintenanceError, OSError, KeyError, tomllib.TOMLDecodeError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
