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

    required_markers: dict[str, tuple[str, ...]] = {
        "crates/codegen/xai-grok-pager-bin/src/main.rs": (
            "let remote_settings: Option<xai_grok_shell::util::config::RemoteSettings> = None;",
            "let use_leader = false;",
        ),
        "crates/codegen/xai-grok-pager/src/acp/spawn.rs": ("AuthManager::new_local(",),
        "crates/codegen/xai-grok-shell/src/agent/init.rs": (
            "cfg.storage_mode = StorageMode::Local;",
        ),
        "crates/codegen/xai-grok-shell/src/agent/mvp_agent/acp_agent.rs": (
            "fn clean_build_extension_disabled(method: &str) -> bool",
        ),
    }
    for relative_path, markers in required_markers.items():
        text = (repo / relative_path).read_text(encoding="utf-8")
        for marker in markers:
            expect(marker in text, f"missing clean-boundary marker in {relative_path}: {marker}")

    acp_agent = (
        repo / "crates/codegen/xai-grok-shell/src/agent/mvp_agent/acp_agent.rs"
    ).read_text(encoding="utf-8")
    for forbidden_call in (
        "force_reload_from_disk();",
        "first_party_env_key_allows_advertise(",
    ):
        expect(
            forbidden_call not in acp_agent,
            f"ACP initialization restored forbidden account-auth call: {forbidden_call}",
        )

    ci = (repo / ".github/workflows/ci.yml").read_text(encoding="utf-8")
    release = (repo / ".github/workflows/release.yml").read_text(encoding="utf-8")
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
