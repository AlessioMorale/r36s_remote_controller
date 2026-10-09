#!/usr/bin/env bash
# Build a pinned, static Lichtblick web bundle for the handheld (plan T1.5).
#
#   lichtblick/build.sh [out_dir]          default out_dir: ./dist
#
# Pinned release: v1.29.1  (commit 95713edb309d77589b130286642ab149995a5f7b,
# tag object verified via the GitHub API on 2026-10-08). Bump LICHTBLICK_TAG and
# LICHTBLICK_COMMIT together; the script refuses a tag that resolves elsewhere.
#
# Build commands, as in the upstream Dockerfile at that tag:
#   corepack enable && yarn install --immutable && yarn run web:build:prod
# (package.json: "web:build:prod": "webpack --mode production ... web/webpack.config.ts";
#  packageManager yarn@4.17.0, engines node >=20, Docker build image node:22.)
# The bundle lands in web/.webpack; this script copies it to out_dir and bakes
# layout.json into index.html. THIS SCRIPT HAS NOT BEEN RUN END TO END (heavy build).
#
# Layout injection: upstream's index.html contains
#   globalThis.LICHTBLICK_SUITE_DEFAULT_LAYOUT = [/*LICHTBLICK_SUITE_DEFAULT_LAYOUT_PLACEHOLDER*/][0];
# (the Docker entrypoint substitutes it the same way). We substitute layout.json
# at build time so the handheld needs no extra mount and no ?layout= parameter.
# Because the layout is the "default layout", Lichtblick selects it only when the
# user has none stored: the UI runs QtWebEngine off-the-record, so that is every start.
#
# Runtime URL (built by the UI from config):
#   file:///opt/kvn_remote_control/lichtblick/index.html?ds=foxglove-websocket&ds.url=ws%3A%2F%2F<robot-zt-ip>%3A8765&openIn=web
# Needs: git, node >= 20, corepack (ships with node), network access.
set -euo pipefail

LICHTBLICK_REPO=https://github.com/lichtblick-suite/lichtblick
LICHTBLICK_TAG=v1.29.1
LICHTBLICK_COMMIT=95713edb309d77589b130286642ab149995a5f7b

HERE=$(cd "$(dirname "$0")" && pwd)
OUT=${1:-$HERE/dist}
SRC=${LICHTBLICK_SRC:-$HERE/.src}

command -v git >/dev/null || { echo "git not found" >&2; exit 1; }
command -v node >/dev/null || { echo "node (>=20) not found" >&2; exit 1; }
NODE_MAJOR=$(node -p 'process.versions.node.split(".")[0]')
[ "$NODE_MAJOR" -ge 20 ] || { echo "node >= 20 required, found $(node -v)" >&2; exit 1; }

if [ ! -d "$SRC/.git" ]; then
  git clone --depth 1 --branch "$LICHTBLICK_TAG" "$LICHTBLICK_REPO" "$SRC"
fi
git -C "$SRC" fetch --depth 1 origin tag "$LICHTBLICK_TAG" --no-tags 2>/dev/null || true
git -C "$SRC" checkout --quiet "$LICHTBLICK_TAG"
HEAD_SHA=$(git -C "$SRC" rev-parse HEAD)
if [ "$HEAD_SHA" != "$LICHTBLICK_COMMIT" ]; then
  echo "tag $LICHTBLICK_TAG resolves to $HEAD_SHA, expected $LICHTBLICK_COMMIT; refusing" >&2
  exit 1
fi

cd "$SRC"
corepack enable
yarn install --immutable
yarn run web:build:prod

BUNDLE=$SRC/web/.webpack
[ -f "$BUNDLE/index.html" ] || { echo "build produced no $BUNDLE/index.html" >&2; exit 1; }
rm -rf "$OUT"
mkdir -p "$OUT"
cp -R "$BUNDLE"/. "$OUT"/

# Bake the fixed layout into index.html (compact JSON, no "/" sequences that could end the script).
python3 - "$OUT/index.html" "$HERE/layout.json" <<'PY'
import json, sys
index, layout = sys.argv[1], sys.argv[2]
html = open(index, encoding="utf-8").read()
marker = "/*LICHTBLICK_SUITE_DEFAULT_LAYOUT_PLACEHOLDER*/"
if marker not in html:
    sys.exit("placeholder not found in index.html; upstream changed, review build.sh")
data = json.dumps(json.load(open(layout, encoding="utf-8")), separators=(",", ":")).replace("</", "<\\/")
open(index, "w", encoding="utf-8").write(html.replace(marker, data, 1))
PY

{
  echo "lichtblick $LICHTBLICK_TAG"
  echo "commit $LICHTBLICK_COMMIT"
  echo "built $(date -u +%Y-%m-%dT%H:%M:%SZ) node $(node -v)"
} > "$OUT/BUILD_INFO.txt"
echo "Lichtblick bundle in $OUT (deploy to /opt/kvn_remote_control/lichtblick on the handheld)"
