# CLAUDE.md — Asuna Coding Agent Instructions

This repository is **Asuna**, a local-first personal AI companion.

Read `PROJECT.md` before making architectural decisions. Read `TRANSCRIPT.md` to understand the original product intent.

## Prime directive

Do not treat Asuna as a generic chatbot UI.

The product loop is:

**wake → voice conversation → contextual help → memory/tool use → safe session close → idle**

## Before coding

For the first run in an existing starter template:

1. Inspect the repository.
2. Identify framework, package manager, runtime, database, desktop wrapper, test stack, and conventions.
3. Preserve useful template components.
4. Do not perform a broad rewrite without a concrete blocker.
5. Produce a migration/implementation plan before large changes.

## Engineering priorities

In order:

1. Working two-way realtime voice.
2. Reliable lifecycle/state management.
3. Local wake word.
4. Persistent memory.
5. Project context.
6. One safe computer tool.
7. Approval/audit layer.
8. Proactivity.

## Current OpenAI direction

Use the current Realtime Agents architecture unless repository constraints strongly justify lower-level API usage.

Preferred model configuration:

```env
ASUNA_REALTIME_MODEL=gpt-realtime-2.1
```

Development/economy option:

```env
ASUNA_REALTIME_MODEL=gpt-realtime-2.1-mini
```

Keep model configuration centralized.

Never ship a permanent OpenAI API key in client/renderer code. Use a trusted process to mint short-lived Realtime credentials.

## Architecture boundaries

Keep these concerns separate:

- `audio`
- `agent`
- `memory`
- `projects`
- `tools`
- `permissions`
- `security`
- `database`
- `ui`

Do not make React components call arbitrary shell commands or database queries directly.

## Tool rules

Every model-accessible tool must have:

- explicit name;
- narrow purpose;
- schema validation;
- risk level;
- approval policy;
- timeout;
- structured result;
- audit event.

Avoid unrestricted shell execution.

Read-only first.

## Security

Never expose secrets to the model unnecessarily.

Block/guard:

- `.env`
- SSH keys
- credentials
- keychains
- tokens
- private certificates

Filesystem operations must be scoped to registered project roots.

Normalize paths and reject traversal.

## Memory

Do not store the entire transcript as “memory.”

Separate:

- raw/optional transcript;
- session summary;
- candidate durable memories;
- project decisions;
- tasks;
- preferences.

Memory must be inspectable and deletable.

## Wake word

Target phrase:

**Hey Asuna**

Wake-word processing must remain local.

Idle microphone audio must not be uploaded to OpenAI.

The wake-word engine should implement an interface so it can be swapped later.

## UX

Voice is primary.

The UI should make the system trustworthy by showing:

- listening;
- connected;
- speaking;
- tool usage;
- approval requests;
- errors;
- current project.

Avoid building a giant dashboard before the voice loop works.

## Code quality

- TypeScript strict mode where applicable.
- Prefer small, testable services.
- Use existing lint/format conventions.
- Add tests for security/permission/path logic.
- Do not over-abstract the first vertical slice.
- Do not add dependencies without a reason.
- Do not silently ignore errors.
- Do not claim work is complete without running relevant checks.

## Git behavior

Before large edits:

- inspect `git status`;
- avoid overwriting unrelated user changes.

After a milestone:

- summarize files changed;
- summarize tests/checks run;
- report remaining blockers.

Do not push/deploy unless explicitly requested.

## First implementation task

Read:

- `PROJECT.md`
- `CLAUDE.md`
- `TRANSCRIPT.md`
- repository source

Then **do not edit code yet**.

Return:

1. repository architecture summary;
2. reusable starter-template pieces;
3. mismatch vs Asuna requirements;
4. proposed Phase 1 architecture;
5. exact file-change plan;
6. required external setup;
7. risks;
8. shortest route to a working realtime voice demo.

Stop after the plan unless the user has explicitly told you to proceed with implementation.
