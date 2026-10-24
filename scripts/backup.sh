#!/usr/bin/env bash
# Copies the source tree (incl. local git history, excl. build artifacts and models) to OneDrive.
# Usage: scripts/backup.sh            (run from anywhere; it finds the repo root itself)
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST_ROOT="${POLARITYPROMPT_BACKUP_DIR:-$HOME/Library/CloudStorage/OneDrive-Personal/Claude/PolarityPrompt/backup}"
STAMP="$(date +%Y-%m-%d_%H%M)"

mkdir -p "$DEST_ROOT/latest" "$DEST_ROOT/snapshots"

# 1. Mirror: always the current state of the source.
rsync -a --delete \
  --exclude 'target/' \
  --exclude 'node_modules/' \
  --exclude 'models/' \
  --exclude 'dist/' \
  --exclude '.DS_Store' \
  "$REPO_ROOT/" "$DEST_ROOT/latest/"

# 2. Dated snapshot: a zip of the mirror, so an accidental bad state can be rolled back by date.
( cd "$DEST_ROOT/latest" && zip -qr "$DEST_ROOT/snapshots/polarityprompt-$STAMP.zip" . )

# 3. Keep the last 14 snapshots.
ls -1t "$DEST_ROOT/snapshots"/polarityprompt-*.zip 2>/dev/null | tail -n +15 | xargs -I{} rm -f "{}"

echo "Backup done → $DEST_ROOT/latest (mirror) and snapshots/polarityprompt-$STAMP.zip"
