#!/usr/bin/env bash
# Puts the Anima server into the app sources, for the version in src-tauri/server-version:
#   src-tauri/binaries/anima-server-<target-triple>[.exe]   (Tauri sidecar)
#   src-tauri/resources/server/{web/dist, models, config.default.toml}
#
# Usage: tools/fetch-server.sh [--target <triple>] [--from <extracted release folder>]
# Without --from, downloads the release asset from github.com/AnimaLab/Anima.
set -euo pipefail
cd "$(dirname "$0")/.."

target="" from=""
while [ $# -gt 0 ]; do
  case "$1" in
    --target) target="$2"; shift ;;
    --from) from="$2"; shift ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
  shift
done
[ -n "$target" ] || target="$(rustc -vV | sed -n 's/^host: //p')"
version="$(tr -d " \n" < src-tauri/server-version)"
ext=""; [[ "$target" == *windows* ]] && ext=".exe"

if [ -z "$from" ]; then
  name="anima-server-v${version}-${target}"
  archive="${name}.tar.gz"; [[ "$target" == *windows* ]] && archive="${name}.zip"
  tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
  echo "downloading $archive"
  curl -fsSL -o "$tmp/$archive" "https://github.com/AnimaLab/Anima/releases/download/v${version}/${archive}"
  case "$archive" in
    *.zip) unzip -q "$tmp/$archive" -d "$tmp" ;;
    *) tar xzf "$tmp/$archive" -C "$tmp" ;;
  esac
  from="$tmp/$name"
fi

got="$("$from/anima-server$ext" --version 2>/dev/null | awk 'NR==1{print $NF}' || true)"
if [ -n "$got" ] && [ "$got" != "$version" ] && [ "$ext" = "" ]; then
  echo "version mismatch: $from has $got, server-version says $version" >&2; exit 1
fi

mkdir -p src-tauri/binaries
cp "$from/anima-server$ext" "src-tauri/binaries/anima-server-${target}${ext}"
chmod +x "src-tauri/binaries/anima-server-${target}${ext}"

res=src-tauri/resources/server
rm -rf "$res"; mkdir -p "$res/web"
cp -R "$from/web/dist" "$res/web/dist"
cp -R "$from/models" "$res/models"
cp "$from/config.default.toml" "$res/config.default.toml"
echo "anima-server $version ($target) ready"
