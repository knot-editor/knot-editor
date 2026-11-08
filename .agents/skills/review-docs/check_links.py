"""Report broken relative links and anchors in Knot's Markdown documentation.

Run from the repository root: python3 .agents/skills/review-docs/check_links.py
"""

import pathlib
import re
import sys

HEADING = re.compile(r"^#+ (.*)")
LINK = re.compile(r"\]\(([^)\s]+)\)")


def anchors(path):
    found = set()
    for line in path.read_text().splitlines():
        match = HEADING.match(line)
        if match:
            text = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", match.group(1)).lower()
            found.add(re.sub(r"[^a-z0-9 _-]", "", text).replace(" ", "-"))
    return found


def main():
    files = sorted(pathlib.Path("docs").rglob("*.md")) + [pathlib.Path("AGENTS.md")]
    broken = 0
    for source in files:
        for match in LINK.finditer(source.read_text()):
            url = match.group(1)
            if url.startswith(("http:", "https:", "mailto:")):
                continue
            path, _, anchor = url.partition("#")
            target = (source.parent / path).resolve() if path else source.resolve()
            if not target.exists():
                print(f"{source}: missing {url}")
                broken += 1
            elif anchor and target.suffix == ".md" and anchor not in anchors(target):
                print(f"{source}: no anchor {url}")
                broken += 1
    print(f"{broken} broken link(s)")
    return 1 if broken else 0


if __name__ == "__main__":
    sys.exit(main())
