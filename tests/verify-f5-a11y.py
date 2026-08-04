#!/usr/bin/env python3
"""F5 gate: static accessibility assertions over the built landing page.

Runs against apps/landing/dist/index.html — the artefact a visitor actually
downloads — rather than the sources, so anything the build strips, hydrates or
rewrites is covered too.

Checks:
  1. exactly one <h1>
  2. no heading level skipped in document order
  3. every <img> carries an alt attribute
  4. every control (a / button / summary / role=button / role=img / role=link)
     has an accessible name
  5. every <a> has a href

Name computation follows the parts of accname that matter here: text content,
`aria-label`, `title`, `aria-labelledby`, and an SVG `<title>` — with anything
under `aria-hidden="true"` contributing nothing, so a decorative lucide glyph
never masks a genuinely unnamed control.
"""

from __future__ import annotations

import re
import sys
from html.parser import HTMLParser
from pathlib import Path

DIST = Path(__file__).resolve().parent.parent / "apps/landing/dist/index.html"

# HTML void elements plus the SVG shapes that ship unclosed. These never get a
# matching end tag, so they must never enter the element stack.
VOID = {
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta",
    "param", "source", "track", "wbr",
    "path", "circle", "rect", "line", "polyline", "polygon", "ellipse", "use", "stop",
}
HEADING = re.compile(r"h[1-6]")
# Machinery, not prose: its text is never part of anyone's accessible name.
NAME_OPAQUE = {"script", "style"}
CONTROL_TAGS = {"a", "button", "summary"}
CONTROL_ROLES = {"button", "img", "link"}


class Element:
    __slots__ = ("tag", "attr", "text")

    def __init__(self, tag: str, attr: dict[str, str]) -> None:
        self.tag = tag
        self.attr = attr
        self.text: list[str] = []


class Audit(HTMLParser):
    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.headings: list[tuple[int, str]] = []
        self.errors: list[str] = []
        self.stack: list[Element] = []
        self.counts = {"img": 0, "a": 0, "control": 0}

    # -- stack ---------------------------------------------------------------

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        attr = {k: (v or "") for k, v in attrs}

        if tag == "img":
            self.counts["img"] += 1
            if "alt" not in attr:
                self.errors.append(f"<img src={attr.get('src', '?')!r}> has no alt attribute")
        if tag == "a":
            self.counts["a"] += 1
            if "href" not in attr:
                self.errors.append(f"<a> with no href (class={attr.get('class', '')[:60]!r})")

        if tag in VOID:
            return
        self.stack.append(Element(tag, attr))

    def handle_startendtag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        self.handle_starttag(tag, attrs)
        if tag not in VOID:
            self.handle_endtag(tag)

    def handle_endtag(self, tag: str) -> None:
        if tag in VOID:
            return
        for index in range(len(self.stack) - 1, -1, -1):
            if self.stack[index].tag == tag:
                # Anything still open above it was never closed; close it too.
                unwound = self.stack[index:]
                del self.stack[index:]
                for element in reversed(unwound):
                    self._close(element)
                return

    def handle_data(self, data: str) -> None:
        stripped = data.strip()
        if not stripped or not self.stack:
            return
        if any(
            element.tag in NAME_OPAQUE or element.attr.get("aria-hidden") == "true"
            for element in self.stack
        ):
            return
        for element in self.stack:
            element.text.append(stripped)

    # -- assertions ----------------------------------------------------------

    def _close(self, element: Element) -> None:
        name = self._accessible_name(element)

        # Text already flows up through handle_data; only a name that came from an
        # attribute needs bubbling — that is how an `aria-label`ed icon names the
        # button wrapping it, and it keeps the ancestor's name from doubling.
        if name and not element.text and self.stack and element.attr.get("aria-hidden") != "true":
            self.stack[-1].text.append(name)

        if HEADING.fullmatch(element.tag):
            self.headings.append((int(element.tag[1]), name[:70]))
            return

        is_control = (
            element.tag in CONTROL_TAGS or element.attr.get("role") in CONTROL_ROLES
        )
        if not is_control or element.attr.get("aria-hidden") == "true":
            return

        self.counts["control"] += 1
        if not name:
            self.errors.append(
                f"<{element.tag}> has no accessible name "
                f"(class={element.attr.get('class', '')[:70]!r})"
            )

    @staticmethod
    def _accessible_name(element: Element) -> str:
        for candidate in (
            " ".join(element.text),
            element.attr.get("aria-label", ""),
            element.attr.get("title", ""),
            element.attr.get("aria-labelledby", ""),
        ):
            if candidate.strip():
                return candidate.strip()
        return ""


def main() -> int:
    if not DIST.is_file():
        print(f"FAIL — {DIST} not built. Run `pnpm build` in apps/landing first.")
        return 1

    raw = DIST.read_text(encoding="utf-8")
    audit = Audit()
    audit.feed(raw)
    for element in reversed(audit.stack):
        audit._close(element)  # noqa: SLF001 — flushing the tail of the same object

    errors = list(audit.errors)

    # Cross-check the walk against raw tag counts, so a parser that silently
    # skipped half the document cannot hand out a clean bill of health.
    for label, pattern, walked in (
        ("headings", r"<h[1-6][\s>]", len(audit.headings)),
        ("<img>", r"<img[\s>]", audit.counts["img"]),
        ("<a>", r"<a[\s>]", audit.counts["a"]),
    ):
        found = len(re.findall(pattern, raw))
        if found != walked:
            errors.append(f"parser walked {walked} {label} but the file has {found}")

    h1s = [h for h in audit.headings if h[0] == 1]
    if len(h1s) != 1:
        errors.append(f"expected exactly one <h1>, found {len(h1s)}: {[h[1] for h in h1s]}")

    previous = 0
    for level, text in audit.headings:
        if previous and level > previous + 1:
            errors.append(f"heading level skipped: h{previous} -> h{level} at {text!r}")
        previous = level

    print("Heading outline:")
    for level, text in audit.headings:
        print(f"  {'  ' * (level - 1)}h{level}  {text}")
    print(
        f"\nWalked: {audit.counts['img']} img, {audit.counts['a']} anchors, "
        f"{audit.counts['control']} named controls, {len(audit.headings)} headings"
    )

    if errors:
        print("\nFAIL")
        for line in errors:
            print(f"  {line}")
        return 1

    print("\nPASS — one h1, no skipped level, every img has alt, every control is named,")
    print("       every anchor has a href")
    return 0


if __name__ == "__main__":
    sys.exit(main())
