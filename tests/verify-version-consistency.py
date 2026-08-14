#!/usr/bin/env python3
"""Release gate: one version number, six manifests.

`docs/dev/04-ci-cd.md` § Manual release process tells a human to bump the version
by hand in several files. Nothing enforces that they all got bumped, and a
mismatch is invisible until a release is already tagged: `tauri build` stamps the
bundle from `tauri.conf.json`, cargo stamps the binary from the workspace
`Cargo.toml`, and the two silently disagree.

This script reads every manifest that carries the product version and fails when
they are not identical. It also accepts an expected version (or a `v`-prefixed
git tag) so `release.yml` can assert that the tag being built matches what is
committed.

Usage:

    python3 tests/verify-version-consistency.py            # manifests agree with each other
    python3 tests/verify-version-consistency.py v0.2.0     # ...and with this tag
    python3 tests/verify-version-consistency.py 0.2.0      # bare version also accepted

Exit codes: 0 = consistent, 1 = mismatch, 2 = a manifest is missing or unreadable.

Hermetic by construction: it only reads files under the repo root, never the
network, never $HOME.
"""

from __future__ import annotations

import json
import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# Every file that carries the product version, in the order a human bumps them.
CARGO_WORKSPACE = Path("Cargo.toml")
TAURI_CONF = Path("apps/autostand-app/src-tauri/tauri.conf.json")
# The design system (MAECLY/autostand-ui) and the marketing site
# (MAECLY/autostand-landing-page) version independently in their own repos; the
# design system is pinned here by commit in pnpm-lock.yaml, not by semver.
PACKAGE_JSONS = [
    Path("package.json"),
    Path("apps/autostand-app/package.json"),
]

# Workspace members are expected to inherit the version rather than restate it;
# a member that hardcodes one is a second place to forget to bump.
MEMBER_CARGO_TOMLS = [
    Path("crates/autostand-core/Cargo.toml"),
    Path("crates/autostand-adapters/Cargo.toml"),
    Path("crates/autostand-scheduler/Cargo.toml"),
    Path("crates/autostand-runlog/Cargo.toml"),
    Path("crates/autostand-local-llm/Cargo.toml"),
    Path("apps/autostand-app/src-tauri/Cargo.toml"),
]

SEMVER = re.compile(
    r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)"
    r"(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?"
    r"(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$"
)


def die(message: str, code: int = 2) -> None:
    print(f"FAIL: {message}", file=sys.stderr)
    raise SystemExit(code)


def read_text(rel: Path) -> str:
    path = ROOT / rel
    if not path.is_file():
        die(f"{rel} does not exist (expected relative to {ROOT})")
    try:
        return path.read_text(encoding="utf-8")
    except OSError as exc:  # pragma: no cover - unreadable file
        die(f"{rel} could not be read: {exc}")
    raise AssertionError("unreachable")


def cargo_workspace_version() -> str:
    data = tomllib.loads(read_text(CARGO_WORKSPACE))
    try:
        version = data["workspace"]["package"]["version"]
    except (KeyError, TypeError):
        die(f"{CARGO_WORKSPACE} has no [workspace.package] version")
    if not isinstance(version, str):
        die(f"{CARGO_WORKSPACE}: [workspace.package] version is not a string")
    return version


def package_json_version(rel: Path) -> str:
    try:
        data = json.loads(read_text(rel))
    except json.JSONDecodeError as exc:
        die(f"{rel} is not valid JSON: {exc}")
    version = data.get("version")
    if not isinstance(version, str):
        die(f'{rel} has no string "version" field')
    return version


def tauri_conf_version() -> str:
    return package_json_version(TAURI_CONF)


def member_version(rel: Path, workspace_version: str) -> tuple[str, bool]:
    """Return (version, inherited) for a workspace member crate."""
    data = tomllib.loads(read_text(rel))
    package = data.get("package")
    if not isinstance(package, dict):
        die(f"{rel} has no [package] table")
    version = package.get("version")
    if isinstance(version, dict) and version.get("workspace") is True:
        return workspace_version, True
    if isinstance(version, str):
        return version, False
    die(f"{rel}: [package] version is neither a literal nor version.workspace = true")
    raise AssertionError("unreachable")


def normalize_expected(raw: str) -> str:
    expected = raw[1:] if raw.startswith("v") else raw
    if not SEMVER.match(expected):
        die(f"{raw!r} is not a semver version or a v-prefixed tag", code=2)
    return expected


def main(argv: list[str]) -> int:
    if len(argv) > 1:
        die(f"usage: {Path(sys.argv[0]).name} [vX.Y.Z]", code=2)
    expected = normalize_expected(argv[0]) if argv else None

    workspace_version = cargo_workspace_version()

    # label -> (version, source note)
    found: dict[str, tuple[str, str]] = {
        str(CARGO_WORKSPACE): (workspace_version, "[workspace.package] version"),
        str(TAURI_CONF): (tauri_conf_version(), '"version"'),
    }
    for rel in PACKAGE_JSONS:
        found[str(rel)] = (package_json_version(rel), '"version"')

    inherited: list[str] = []
    for rel in MEMBER_CARGO_TOMLS:
        version, is_inherited = member_version(rel, workspace_version)
        if is_inherited:
            inherited.append(str(rel))
        else:
            found[str(rel)] = (version, "[package] version (hardcoded, should inherit)")

    width = max(len(name) for name in found)
    for name, (version, note) in found.items():
        print(f"  {name.ljust(width)}  {version}   ({note})")
    for name in inherited:
        print(f"  {name.ljust(width)}  {workspace_version}   (inherits version.workspace)")
    # stdout is block-buffered under CI; without this the table lands after the
    # stderr diagnostic and the log reads backwards.
    sys.stdout.flush()

    versions = {version for version, _ in found.values()}
    if len(versions) != 1:
        by_version: dict[str, list[str]] = {}
        for name, (version, _) in found.items():
            by_version.setdefault(version, []).append(name)
        lines = [
            "version mismatch across manifests — a release built from this tree "
            "would ship a binary and a bundle stamped with different versions.",
            "",
            "  found:",
        ]
        for version in sorted(by_version):
            for name in sorted(by_version[version]):
                lines.append(f"    {version}  {name}")
        lines += [
            "",
            "  fix: set the same version in every file listed above "
            "(see docs/dev/04-ci-cd.md § Manual release process).",
        ]
        print("FAIL: " + "\n".join(lines), file=sys.stderr)
        return 1

    version = versions.pop()
    if not SEMVER.match(version):
        print(
            f"FAIL: {version!r} is not a valid semver version "
            "(tauri and cargo both reject non-semver versions at build time).",
            file=sys.stderr,
        )
        return 1

    if expected is not None and version != expected:
        print(
            f"FAIL: the manifests say {version}, but the release was asked for "
            f"{expected}.\n"
            "  fix: bump every manifest to the tagged version, or re-tag to match "
            "the committed version (see docs/dev/04-ci-cd.md § Manual release process).",
            file=sys.stderr,
        )
        return 1

    suffix = f" and matches the requested {expected}" if expected else ""
    print(f"OK: every manifest is at {version}{suffix}.")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
