#!/usr/bin/env bash
# Renders the link-preview cards into frontend/static. Needs google-chrome
# (fonts load from Google Fonts) and ImageMagick.
#   home.html -> og-card.jpg   lab.html -> og-lab.jpg
set -euo pipefail
dir="$(cd "$(dirname "$0")" && pwd)"
static="$dir/../static"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
render() {
	google-chrome --headless=new --no-sandbox --disable-gpu --hide-scrollbars \
		--window-size=1200,630 --virtual-time-budget=4000 \
		--screenshot="$tmp/$1.png" "file://$dir/$1.html" 2>/dev/null
	convert "$tmp/$1.png" -strip -quality 90 "$static/$2"
}
render home og-card.jpg
render lab og-lab.jpg
