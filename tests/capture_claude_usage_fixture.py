#!/usr/bin/env python3
"""Capture a real `/api/oauth/usage` payload for the Claude usage-probe fixture.

Throwaway capture tool, not part of the build. It never prints, logs or writes
the access token: the token is read into a local variable, passed to the request
as a header, and dropped. Only the response *body* reaches stdout.

Usage:  python3 tests/capture_claude_usage_fixture.py > /dev/null 2>&1
        (the script writes the fixture itself; stdout carries only a summary)
"""

from __future__ import annotations

import json
import os
import pathlib
import subprocess
import sys
import urllib.error
import urllib.request

UA_VERSION = "2.1.69"
USAGE_URL = "https://api.anthropic.com/api/oauth/usage"


def _oauth_from_text(raw: str) -> dict | None:
    for candidate in (raw, None):
        if candidate is None:
            try:
                candidate = bytes.fromhex(raw.strip()).decode("utf-8")
            except ValueError:
                return None
        try:
            parsed = json.loads(candidate)
        except json.JSONDecodeError:
            continue
        oauth = parsed.get("claudeAiOauth")
        if isinstance(oauth, dict) and oauth.get("accessToken"):
            return oauth
    return None


def load_oauth() -> dict | None:
    base = "Claude Code-credentials"
    try:
        out = subprocess.run(
            ["/usr/bin/security", "find-generic-password", "-a", os.environ.get("USER", ""),
             "-s", base, "-w"],
            capture_output=True, text=True, timeout=10,
        )
        if out.returncode == 0:
            oauth = _oauth_from_text(out.stdout)
            if oauth:
                return oauth
    except (OSError, subprocess.SubprocessError):
        pass

    config_dir = os.environ.get("CLAUDE_CONFIG_DIR") or "~/.claude"
    path = pathlib.Path(config_dir).expanduser() / ".credentials.json"
    if path.is_file():
        return _oauth_from_text(path.read_text(encoding="utf-8", errors="replace"))
    return None


def main() -> int:
    oauth = load_oauth()
    if not oauth:
        print("no local Claude credential found", file=sys.stderr)
        return 1

    request = urllib.request.Request(
        USAGE_URL,
        headers={
            "Authorization": f"Bearer {oauth['accessToken'].strip()}",
            "Accept": "application/json",
            "Content-Type": "application/json",
            "anthropic-beta": "oauth-2025-04-20",
            "User-Agent": f"claude-code/{UA_VERSION}",
        },
    )
    try:
        with urllib.request.urlopen(request, timeout=10) as response:
            status = response.status
            body = response.read().decode("utf-8", errors="replace")
    except urllib.error.HTTPError as err:
        print(f"status={err.code}", file=sys.stderr)
        return 2
    except urllib.error.URLError:
        print("transport failure", file=sys.stderr)
        return 3

    destination = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else pathlib.Path("claude-usage.json")
    destination.write_text(json.dumps(json.loads(body), indent=2) + "\n", encoding="utf-8")
    print(f"status={status} keys={sorted(json.loads(body).keys())} -> {destination}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
