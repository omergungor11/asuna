#!/bin/bash
# PreToolUse hook (Bash matcher): protect main branch from direct commit/push.
# OPT-IN: only active when .claude/protect-main marker file exists.
#   Enable:  touch .claude/protect-main
#   Disable: rm .claude/protect-main
# Exit 2 = BLOCK, Exit 0 = ALLOW

INPUT=$(cat)
CMD=$(echo "$INPUT" | jq -r '.tool_input.command // empty')

[[ -z "$CMD" ]] && exit 0

PROJECT_DIR="${CLAUDE_PROJECT_DIR:-$(pwd)}"
[[ -f "$PROJECT_DIR/.claude/protect-main" ]] || exit 0

# Only inspect git commit / git push commands
if ! echo "$CMD" | grep -qE '(^|[;&|[:space:]])git[[:space:]]+(commit|push)'; then
  exit 0
fi

BRANCH=$(git -C "$PROJECT_DIR" symbolic-ref --short HEAD 2>/dev/null || echo "")

# Block commit while on main/master
if echo "$CMD" | grep -qE '(^|[;&|[:space:]])git[[:space:]]+commit' && [[ "$BRANCH" == "main" || "$BRANCH" == "master" ]]; then
  echo "BLOCKED: Direct commit to '$BRANCH' is protected. Create a feature branch (feat/ASU-XXX-...) and open a PR. (Disable: rm .claude/protect-main)" >&2
  exit 2
fi

# Block push to main/master (explicit ref or while on main)
if echo "$CMD" | grep -qE '(^|[;&|[:space:]])git[[:space:]]+push'; then
  if echo "$CMD" | grep -qE '[[:space:]](main|master)([[:space:]]|$|:)' || [[ "$BRANCH" == "main" || "$BRANCH" == "master" ]]; then
    echo "BLOCKED: Direct push to '$BRANCH' is protected. Push a feature branch and open a PR. (Disable: rm .claude/protect-main)" >&2
    exit 2
  fi
fi

exit 0
