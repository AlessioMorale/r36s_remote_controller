#!/usr/bin/env python3
"""Bake the handheld's layout and workspace defaults into a Lichtblick index.html.

    bake_index.py <index.html> <layout.json>

Runs on a fresh upstream build (placeholder present) and on an already baked
index.html (re-bakes in place), so a deployed bundle can be updated without a rebuild.

1. Layout: upstream's index.html has
     globalThis.LICHTBLICK_SUITE_DEFAULT_LAYOUT = [/*LICHTBLICK_SUITE_DEFAULT_LAYOUT_PLACEHOLDER*/][0];
   layout.json goes in there (or replaces the layout baked by an earlier run).
2. Workspace: Lichtblick keeps sidebar state in localStorage["fox.workspace"] (zustand persist,
   version 1, deep-merged over the defaults, which open the left "panel-settings" sidebar).
   The UI runs WebEngine off-the-record, so storage is empty on every start and the left sidebar
   would take 320 of the ~450 px the page gets. A small script, run before the app bundle,
   seeds both sidebars closed.
"""
import json
import re
import sys

PLACEHOLDER = "/*LICHTBLICK_SUITE_DEFAULT_LAYOUT_PLACEHOLDER*/"
LAYOUT_RE = re.compile(r"(globalThis\.LICHTBLICK_SUITE_DEFAULT_LAYOUT = \[).*?(\]\[0\];)", re.S)
SEED_BEGIN, SEED_END = "<!--rc-workspace-seed-->", "<!--/rc-workspace-seed-->"
WORKSPACE = {
    "state": {"sidebars": {"left": {"open": False}, "right": {"open": False}}},
    "version": 1,
}


def main(index: str, layout: str) -> None:
    html = open(index, encoding="utf-8").read()
    data = json.dumps(json.load(open(layout, encoding="utf-8")), separators=(",", ":"))
    data = data.replace("</", "<\\/")

    if PLACEHOLDER in html:
        html = html.replace(PLACEHOLDER, data, 1)
    elif LAYOUT_RE.search(html):
        html = LAYOUT_RE.sub(lambda m: m.group(1) + data + m.group(2), html, count=1)
    else:
        sys.exit("no layout placeholder or baked layout in index.html; upstream changed, review")

    seed = (
        SEED_BEGIN
        + "<script>try{localStorage.setItem(\"fox.workspace\","
        + json.dumps(json.dumps(WORKSPACE, separators=(",", ":")))
        + ")}catch(e){}</script>"
        + SEED_END
    )
    if SEED_BEGIN in html:
        html = re.sub(re.escape(SEED_BEGIN) + ".*?" + re.escape(SEED_END), lambda m: seed, html, flags=re.S)
    else:
        # Must run before the deferred app bundle; the first <script in <head> is that bundle.
        pos = html.find("<script")
        if pos < 0:
            sys.exit("no <script> in index.html; upstream changed, review")
        html = html[:pos] + seed + html[pos:]

    open(index, "w", encoding="utf-8").write(html)


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    main(sys.argv[1], sys.argv[2])
