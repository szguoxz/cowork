#!/bin/bash
set -e

DEST=/tmp/cowork-github-sync
REMOTE=https://github.com/szguoxz/cowork.git
SRC="$(cd "$(dirname "$0")" && pwd)"

rm -rf "$DEST"
mkdir -p "$DEST"
cd "$DEST"
git init
git remote add origin "$REMOTE"

rsync -a "$SRC/" "$DEST/" \
    --exclude target \
    --exclude node_modules \
    --exclude .git \
    --exclude deps

git add -A
git commit -m "Update $(date +%F)"
git branch -M main
git push -f origin main

echo "Pushed to $REMOTE"
