#!/bin/bash
# PreToolUse hook: protect sensitive files from Edit/Write
# Exit 2 = BLOCK, Exit 0 = ALLOW

INPUT=$(cat)
FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // empty')

[[ -z "$FILE_PATH" ]] && exit 0

# .env files (all variants, except .env.example)
if [[ "$FILE_PATH" =~ \.env($|\.local|\.production|\.staging|\.test|\.development) ]]; then
  echo "BLOCKED: .env files are protected. Never edit secrets directly." >&2
  exit 2
fi

# Lock files
case "$FILE_PATH" in
  *pnpm-lock.yaml|*package-lock.json|*yarn.lock|*bun.lockb|*bun.lock)
    echo "BLOCKED: Lock files should only be modified by the package manager." >&2
    exit 2 ;;
esac

# .git internals
if [[ "$FILE_PATH" == *"/.git/"* ]]; then
  echo "BLOCKED: .git directory should not be edited directly." >&2
  exit 2
fi

# Keys, certs, tokens
case "$FILE_PATH" in
  *.pem|*.key|*id_rsa*|*id_ed25519*|*.npmrc)
    echo "BLOCKED: Key/credential files are protected." >&2
    exit 2 ;;
esac

# Credential-looking files
if [[ "$FILE_PATH" == *"credentials"* ]] || [[ "$FILE_PATH" == *"secrets."* ]] || [[ "$FILE_PATH" == *"service-account"* ]]; then
  echo "BLOCKED: Credential files are protected." >&2
  exit 2
fi

exit 0
