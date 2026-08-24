#!/bin/bash
# PostToolUse hook (Edit|Write): run validation after source file edits.
# OPT-IN: not wired in settings.local.json by default — see README.
# Keep VALIDATE_CMD fast (typecheck only); full test suite belongs in /git-full & CI.

INPUT=$(cat)
FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // empty')

[[ -z "$FILE_PATH" ]] && exit 0

# Only validate source files (CUSTOMIZE extensions)
case "$FILE_PATH" in
  *.ts|*.tsx|*.js|*.jsx) ;;
  *) exit 0 ;;
esac

cd "${CLAUDE_PROJECT_DIR:-$(pwd)}" || exit 0

# CUSTOMIZE: your fast validation command, e.g.:
# VALIDATE_CMD="pnpm typecheck"
VALIDATE_CMD=""

[[ -z "$VALIDATE_CMD" ]] && exit 0

OUTPUT=$($VALIDATE_CMD 2>&1)
if [[ $? -ne 0 ]]; then
  # Non-blocking: report to Claude via stdout so it can fix immediately
  echo "VALIDATION FAILED after editing $FILE_PATH:"
  echo "$OUTPUT" | tail -30
fi
exit 0
