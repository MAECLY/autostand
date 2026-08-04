#!/usr/bin/env python3
"""F5 gate: WCAG contrast for every foreground/background pair the landing page
actually paints, in both themes.

Token values are read from design-system/tokens/tokens.css, so this measures the
shipped tokens rather than a copy of them. Threshold is 4.5:1 (WCAG AA, normal
text) — nothing on the page is large enough to earn the 3:1 bar.

Two lists, because they have two different fix sites:

  PAGE_OWNED  — the landing page picks these utilities, so a regression here is
                fixable in apps/landing/src and fails the run.
  TOKEN_DEBT  — the pair is forced by a design-system token or a frozen base
                component. Printed with its real number every run so it cannot
                be quietly forgotten, but it does not fail this gate: the fix is
                a token change in design-system/tokens/tokens.css, which restyles
                the Tauri app too and is out of scope for F5.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TOKENS = ROOT / "design-system/tokens/tokens.css"

AA_NORMAL = 4.5
AA_LARGE = 3.0


def parse_blocks(css: str) -> tuple[dict[str, str], dict[str, str]]:
    """Return (:root vars, .dark overrides)."""

    def block(selector: str) -> dict[str, str]:
        match = re.search(rf"{re.escape(selector)}\s*\{{(.*?)\n\}}", css, re.S)
        if not match:
            raise SystemExit(f"{selector} block not found in {TOKENS}")
        return dict(re.findall(r"(--[\w-]+)\s*:\s*([^;]+);", match.group(1)))

    return block(":root"), block(".dark")


def resolve(name: str, light: dict[str, str], dark: dict[str, str], is_dark: bool) -> str:
    seen = set()
    value = (dark if is_dark else light).get(name) or light.get(name)
    while value is not None:
        value = value.strip()
        ref = re.fullmatch(r"var\((--[\w-]+)\)", value)
        if not ref:
            return value
        name = ref.group(1)
        if name in seen:
            raise SystemExit(f"cycle resolving {name}")
        seen.add(name)
        value = (dark if is_dark else light).get(name) or light.get(name)
    raise SystemExit(f"unresolved token {name}")


def luminance(hex_color: str) -> float:
    hex_color = hex_color.lstrip("#")
    channels = [int(hex_color[i : i + 2], 16) / 255 for i in (0, 2, 4)]
    linear = [c / 12.92 if c <= 0.03928 else ((c + 0.055) / 1.055) ** 2.4 for c in channels]
    return 0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2]


def ratio(fg: str, bg: str) -> float:
    a, b = luminance(fg), luminance(bg)
    lighter, darker = max(a, b), min(a, b)
    return (lighter + 0.05) / (darker + 0.05)


# (foreground token, background token, where it appears)
PAGE_OWNED: list[tuple[str, str, str]] = [
    ("--fg-base", "--bg-base", "body copy on the page background"),
    ("--fg-base", "--bg-surface", "body copy on cards, the tonal band and the footer"),
    ("--fg-base", "--bg-muted", "the phantom row in the audit table"),
    ("--fg-muted", "--bg-base", "subheads, feature bodies, nav links"),
    ("--fg-muted", "--bg-surface", "card body copy, footer links, mockup labels"),
]

# (foreground token, background token, where it appears, why markup cannot fix it)
TOKEN_DEBT: list[tuple[str, str, str, str]] = [
    (
        "--fg-muted",
        "--bg-elevated",
        "the mockup window-chrome title",
        "--fg-muted is slate-500 in light; only tokens.css can darken it",
    ),
    (
        # Was --fg-inverse, which dark mode maps to slate-950 and which therefore
        # painted 3.90:1 labels on the brand fill. --fg-on-brand is the token
        # --color-primary-foreground now resolves to, so this is the pair the
        # page actually paints; measuring --fg-inverse here would gate a pair
        # nothing renders any more.
        "--fg-on-brand",
        "--brand-primary",
        "every primary button label and the pipeline step numbers",
        "on-brand text has its own token; only tokens.css can change it",
    ),
    (
        "--fg-on-brand",
        "--brand-primary-hover",
        "primary button label while hovered",
        "same --fg-on-brand mapping",
    ),
    (
        "--brand-primary",
        "--bg-base",
        "FAQ links",
        "blue-600 is the brand colour; dark mode needs a lighter --brand-primary",
    ),
    (
        "--brand-primary",
        "--bg-surface",
        "the pipeline icon on the tonal band",
        "decorative glyph, but same token pair as the links above",
    ),
    (
        "--audit-commit",
        "--bg-surface",
        "audit badge: commit",
        "the six audit colours are the section's semantics; they live in tokens.css",
    ),
    ("--audit-github", "--bg-surface", "audit badge: github", "same"),
    ("--audit-review", "--bg-surface", "audit badge: review", "same"),
    ("--audit-note", "--bg-surface", "audit badge: note", "same"),
    ("--audit-phantom", "--bg-surface", "audit badge: phantom", "same"),
    ("--audit-unverified", "--bg-surface", "audit badge: unverified", "same"),
    (
        "--status-warning",
        "--status-warning-bg",
        "Badge variant=warning in the mockup",
        "badge.tsx is a frozen base component and --status-*-bg has no .dark override",
    ),
    (
        "--status-success",
        "--status-success-bg",
        "Badge variant=success in the mockup",
        "same",
    ),
]


def main() -> int:
    css = TOKENS.read_text(encoding="utf-8")
    light, dark = parse_blocks(css)

    def measure(fg_token: str, bg_token: str, is_dark: bool) -> tuple[float, str, str]:
        fg = resolve(fg_token, light, dark, is_dark)
        bg = resolve(bg_token, light, dark, is_dark)
        return ratio(fg, bg), fg, bg

    failures: list[str] = []
    for theme, is_dark in (("light", False), ("dark", True)):
        print(f"\n{theme.upper()} — page-owned pairs (these gate the build)")
        for fg_token, bg_token, where in PAGE_OWNED:
            value, fg, bg = measure(fg_token, bg_token, is_dark)
            mark = "ok " if value >= AA_NORMAL else "FAIL"
            print(f"  {mark} {value:5.2f}:1  {fg_token} {fg} on {bg_token} {bg}  — {where}")
            if value < AA_NORMAL:
                failures.append(
                    f"{theme}: {fg_token} on {bg_token} is {value:.2f}:1, "
                    f"needs {AA_NORMAL}:1 — {where}"
                )

    print("\nDESIGN-SYSTEM TOKEN DEBT — reported, not gated (fix site: tokens.css)")
    debt = 0
    for fg_token, bg_token, where, why in TOKEN_DEBT:
        worst_theme, worst = min(
            (("light", measure(fg_token, bg_token, False)[0]),
             ("dark", measure(fg_token, bg_token, True)[0])),
            key=lambda pair: pair[1],
        )
        if worst >= AA_NORMAL:
            print(f"  ok   {worst:5.2f}:1 worst case  {fg_token} on {bg_token}  — {where}")
            continue
        debt += 1
        print(
            f"  DEBT {worst:5.2f}:1 in {worst_theme:<5} {fg_token} on {bg_token}  — {where}\n"
            f"       {why}"
        )

    if failures:
        print("\nFAIL")
        for line in failures:
            print(f"  {line}")
        return 1

    print(
        f"\nPASS — every pair the landing page controls clears WCAG AA in both themes.\n"
        f"       {debt} token pair(s) still below 4.5:1; see the debt list above."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
