#!/usr/bin/env bash
# Claude Code のホーム設定をこのリポジトリから Cursor / Claude Code 両方が参照できるようにする。
# 参考: https://qiita.com/nogataka/items/7476eb9dfc8bca4e0bb8（Cursor は .claude/skills を読む）

set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
mkdir -p "${ROOT}/.claude"

ln -sf "${HOME}/.claude/skills" "${ROOT}/.claude/skills"
ln -sf "${HOME}/.claude/CLAUDE.md" "${ROOT}/CLAUDE.md"
ln -sf "${HOME}/.claude/CLAUDE.md" "${ROOT}/AGENTS.md"

echo "OK: ${ROOT}/.claude/skills -> ${HOME}/.claude/skills"
echo "OK: ${ROOT}/CLAUDE.md -> ${HOME}/.claude/CLAUDE.md"
echo "OK: ${ROOT}/AGENTS.md -> ${HOME}/.claude/CLAUDE.md"
