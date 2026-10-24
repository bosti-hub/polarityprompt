#!/usr/bin/env bash
# Downloads the Vosk speech models and the libvosk native library into models/.
# models/ is not committed (see .gitignore); run this once per machine, also on the
# GitHub build machines (Git Bash on Windows). Users of the packaged app never need
# this: English and the library are inside the installer, other languages are downloaded
# inside the app.
# Usage: scripts/fetch-models.sh          English + Dutch (what the tests and the build need)
#        scripts/fetch-models.sh --all    also German, French, Spanish and Portuguese
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODELS="$REPO_ROOT/models"
TMP="$MODELS/.tmp"
mkdir -p "$MODELS" "$TMP"

VOSK_BASE="https://alphacephei.com/vosk/models"
NL="vosk-model-small-nl-0.22"
EN="vosk-model-small-en-us-0.15"     # bundled: also in apps/desktop/tauri.*.conf.json
EXTRA="vosk-model-small-de-0.15 vosk-model-small-fr-0.22 vosk-model-small-es-0.42 vosk-model-small-pt-0.3"

extract() { # zip, destination dir
  if command -v unzip >/dev/null 2>&1; then unzip -q -o "$1" -d "$2"; else tar -xf "$1" -C "$2"; fi
}

fetch_model() {
  local name="$1"
  if [ -d "$MODELS/$name" ]; then
    echo "model $name already present"
    return
  fi
  echo "downloading $name (about 40 MB)…"
  curl -L --fail --progress-bar -o "$TMP/$name.zip" "$VOSK_BASE/$name.zip"
  extract "$TMP/$name.zip" "$MODELS"
  rm -f "$TMP/$name.zip"
  echo "model $name ready"
}

fetch_model "$NL"
fetch_model "$EN"
if [ "${1:-}" = "--all" ]; then
  for m in $EXTRA; do fetch_model "$m"; done
fi

LIB="$MODELS/libvosk"
mkdir -p "$LIB"
case "$(uname -s)" in
  Darwin)
    # The Vosk project ships the macOS library inside its Python wheel (a wheel is a zip).
    if [ ! -f "$LIB/libvosk.dylib" ]; then
      echo "downloading libvosk for macOS (about 4 MB)…"
      WHL="vosk-0.3.44-py3-none-macosx_10_6_universal2.whl"
      curl -L --fail --progress-bar -o "$TMP/$WHL" \
        "https://files.pythonhosted.org/packages/44/19/5e8299237bc2005c3d155d3b48adba6fd6484465ad5c970302fc1d37947d/$WHL"
      mkdir -p "$TMP/whl"
      extract "$TMP/$WHL" "$TMP/whl"
      cp "$TMP/whl/vosk/libvosk.dyld" "$LIB/libvosk.dylib"
      rm -rf "$TMP/$WHL" "$TMP/whl"
      install_name_tool -id "@rpath/libvosk.dylib" "$LIB/libvosk.dylib"
    fi
    ln -sfn "$NL" "$MODELS/vosk-nl"
    ln -sfn "$EN" "$MODELS/vosk-en"
    echo "libvosk: $(lipo -info "$LIB/libvosk.dylib")"
    ;;
  MINGW*|MSYS*|CYGWIN*|Windows_NT)
    # The GitHub release zip has the DLL, its import library and the MinGW runtime DLLs.
    if [ ! -f "$LIB/libvosk.dll" ]; then
      echo "downloading libvosk for Windows (about 15 MB)…"
      ZIP="vosk-win64-0.3.45.zip"
      curl -L --fail --progress-bar -o "$TMP/$ZIP" \
        "https://github.com/alphacep/vosk-api/releases/download/v0.3.45/$ZIP"
      mkdir -p "$TMP/win"
      extract "$TMP/$ZIP" "$TMP/win"
      cp "$TMP/win"/vosk-win64-*/*.dll "$TMP/win"/vosk-win64-*/*.lib "$LIB/"
      rm -rf "$TMP/$ZIP" "$TMP/win"
    fi
    ls "$LIB"
    ;;
  *)
    echo "unsupported platform: $(uname -s)"; exit 1 ;;
esac

rmdir "$TMP" 2>/dev/null || true
echo "done: $(ls "$MODELS")"
