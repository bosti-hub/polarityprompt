#!/usr/bin/env bash
# Copies the source tree (incl. local git history, excl. build artifacts and models) to OneDrive.
# Usage: scripts/backup.sh            (run from anywhere; it finds the repo root itself)
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST_ROOT="${POLARITYPROMPT_BACKUP_DIR:-$HOME/Library/CloudStorage/OneDrive-Personal/Claude/PolarityPrompt/backup}"
STAMP="$(date +%Y-%m-%d_%H%M)"

mkdir -p "$DEST_ROOT/latest" "$DEST_ROOT/snapshots"

# Neither step reads files that are already in OneDrive: with Files On-Demand those can be
# cloud-only placeholders, and reading one waits for a download (rsync: "mmap: Operation
# timed out").

# 1. Mirror: always the current state of the source. --whole-file copies changed files
#    instead of comparing them with the old copy.
rsync -a --delete --whole-file \
  --exclude 'target/' \
  --exclude 'node_modules/' \
  --exclude 'models/' \
  --exclude 'dist/' \
  --exclude '.DS_Store' \
  "$REPO_ROOT/" "$DEST_ROOT/latest/"

# 2. Dated snapshot: a zip of the source (same exclusions), so an accidental bad state can be
#    rolled back by date.
( cd "$REPO_ROOT" && zip -qr "$DEST_ROOT/snapshots/polarityprompt-$STAMP.zip" . \
    -x 'target/*' '*/target/*' 'node_modules/*' '*/node_modules/*' 'models/*' \
       'dist/*' '*/dist/*' '*.DS_Store' )

# 3. Keep the last 14 snapshots.
ls -1t "$DEST_ROOT/snapshots"/polarityprompt-*.zip 2>/dev/null | tail -n +15 | xargs -I{} rm -f "{}"

echo "Backup done → $DEST_ROOT/latest (mirror) and snapshots/polarityprompt-$STAMP.zip"
