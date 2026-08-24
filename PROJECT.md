# ASUNA — PROJECT.md

> **Status:** Living product/architecture specification  
> **Version:** 0.1 — MVP foundation  
> **Date:** 2026-08-24  
> **Primary goal:** Build a local-first, low-latency personal AI companion that can be reached instantly by voice, remembers useful context, understands ongoing projects, and can safely use tools on the user's computer.

---

## 0. Executive Summary

**Asuna is not a chatbot.**

Asuna is a personal AI operating layer designed to help one person think, build, remember, recover focus, and take action.

The core experience should feel closer to a persistent desktop companion than to opening a chat window:

- The user says **“Hey Asuna”**.
- Asuna wakes immediately.
- The user speaks naturally.
- Asuna understands the request with low latency.
- Asuna can remember relevant prior context.
- Asuna can inspect the user's current project/workspace through explicit tools.
- Asuna can suggest the next useful step when the user is stuck.
- Asuna can execute approved local actions through a controlled tool layer.
- After the conversation, useful information can be stored as durable memory.
- The user can return later and continue without restating everything.

The first version is intentionally personal and local-first. It should become useful to its creator before it is generalized into a commercial product.

The long-term product opportunity is broader: an enterprise/personal productivity voice agent that sits between a human and their software environment, understands ongoing work, and provides context-aware assistance through voice.

---

# 1. Product Vision

## 1.1 Product statement

Asuna is a persistent AI companion that can:

1. **Listen when explicitly activated.**
2. **Talk naturally and interruptibly.**
3. **Understand the user's work context.**
4. **Remember durable information.**
5. **Recognize what project the user is working on.**
6. **Help the user recover when focus is lost.**
7. **Use software tools through explicit permissions.**
8. **Gradually become more personalized without becoming opaque or invasive.**

The user should not need to “go to AI.”

AI should be available in the environment, while remaining under the user's control.

---

# 2. Why This Product Exists

The initial problem is not lack of ideas.

The initial problem is fragmentation.

The user often has:

- multiple active projects;
- open technical decisions;
- unfinished tasks;
- context spread across editors, terminals, notes, repositories, browsers, and chats;
- difficulty returning to a project after attention shifts;
- periods where deciding “what should I do now?” consumes more energy than the actual work.

Traditional chat assistants add another destination the user must consciously open.

Asuna should reduce that friction.

The system should be able to answer questions such as:

- “Asuna, beni toparla.”
- “En son bu projede ne yapıyorduk?”
- “Bugün neye odaklanmam daha mantıklı?”
- “Bu projede nerede tıkandık?”
- “Son hatayı tekrar göster.”
- “Bu repoyu analiz et ve sıradaki üç adımı söyle.”
- “Şu an dağıldım; bana tek bir sonraki görev ver.”
- “Dün konuştuğumuz fikri hatırlıyor musun?”
- “Codex'in yaptığı değişiklikleri özetle.”
- “Bu terminal hatasına bak.”
- “Bu işi bitirmek için benden ne eksik?”

The system is therefore both:

- a **voice interface**, and
- a **context + memory + tool orchestration layer**.

---

# 3. MVP Definition

## 3.1 MVP success condition

The first real MVP is successful when the following flow works end-to-end:

1. Asuna runs on the user's Mac.
2. A local wake-word service listens for **“Hey Asuna”**.
3. Wake-word detection activates an interaction session.
4. The microphone is routed into an OpenAI Realtime session.
5. The user and Asuna can speak naturally.
6. The user can interrupt Asuna while it is speaking.
7. Asuna has a stable identity and system prompt.
8. Asuna can read a small amount of persistent memory.
9. Asuna can write a new memory after a conversation when appropriate.
10. Asuna has at least one real computer tool.
11. Every dangerous or destructive action requires confirmation.
12. A simple local UI shows:
   - listening state,
   - thinking state,
   - speaking state,
   - current transcript,
   - current project/context,
   - tool calls,
   - errors.

### Recommended first tool

For the first end-to-end milestone, implement one low-risk tool such as:

- `get_current_project_context()`
- `list_recent_projects()`
- `read_project_summary()`
- `open_project_in_vscode(projectPath)`

Do **not** make unrestricted shell execution the first tool.

---

# 4. Non-Goals for MVP

The MVP should **not** attempt to implement all of the following immediately:

- autonomous long-running computer control;
- unrestricted terminal access;
- full filesystem indexing;
- screen recording;
- continuous cloud microphone streaming;
- multi-user accounts;
- enterprise tenancy;
- mobile applications;
- calendar/email integrations;
- browser automation;
- full agent swarm architecture;
- emotional state diagnosis;
- fully automatic task planning;
- remote wake-word detection;
- constant storage of everything the user says.

These can be added later.

The MVP exists to prove the core loop:

**wake → talk → understand → remember → use one tool → return to idle**

---

# 5. Product Principles

## 5.1 Local-first by default

Anything that can remain local should remain local.

Examples:

- wake-word detection;
- application state;
- project registry;
- memory database;
- permission state;
- tool audit log;
- local configuration;
- activity metadata.

Audio should not be transmitted until an active interaction is intentionally started.

## 5.2 Explicit activation

Asuna should not behave like a hidden recorder.

Default state:

`IDLE_WAKE_WORD`

After activation:

`ACTIVE_CONVERSATION`

After inactivity or a verbal close:

back to:

`IDLE_WAKE_WORD`

## 5.3 Useful memory, not infinite memory

Asuna should not blindly persist entire conversations forever.

Memory must be classified.

Possible memory types:

- `profile`
- `preference`
- `project`
- `decision`
- `task`
- `working_context`
- `relationship`
- `idea`
- `routine`
- `tool_state`

Every stored memory should have:

- id;
- type;
- content;
- source;
- created_at;
- updated_at;
- confidence;
- importance;
- last_accessed_at;
- optional expiry;
- optional project_id.

## 5.4 Human approval for meaningful actions

Actions should have risk levels.

### Risk level 0 — read-only
No approval required.

Examples:

- read file;
- list project files;
- read git status;
- inspect recent logs.

### Risk level 1 — reversible low-risk
Approval can be configurable.

Examples:

- open application;
- create note;
- create draft file;
- switch project.

### Risk level 2 — mutation
Always show clear confirmation in MVP.

Examples:

- edit file;
- install package;
- run build command;
- commit changes.

### Risk level 3 — destructive/external
Always require explicit approval.

Examples:

- delete files;
- push to remote;
- send email;
- publish content;
- deploy;
- spend money;
- modify system settings.

## 5.5 One next step beats ten suggestions

When the user says they are stuck or scattered, Asuna should prefer:

1. determine context;
2. identify blockage;
3. propose one smallest next action;
4. help execute it;
5. only then expand.

---

# 6. Technology Strategy

## 6.1 Preserve the existing starter template

There is already a starter template in the repository.

**Do not rebuild the project from scratch until the template has been inspected.**

The coding agent must first identify:

- framework;
- package manager;
- language;
- desktop shell if any;
- backend/runtime;
- database layer;
- styling system;
- existing auth;
- environment variable strategy;
- testing stack;
- lint/format setup;
- folder conventions.

Only replace a component if the existing choice blocks a core Asuna requirement.

---

# 7. Recommended MVP Stack

The exact stack should adapt to the existing template, but the preferred architecture is:

## Desktop shell

**Tauri 2 + React + TypeScript**

Why:

- lightweight desktop distribution;
- access to native capabilities;
- safer capability-based model than an unrestricted Electron main process;
- macOS support;
- suitable foundation for system tray, global shortcuts, notifications, native window behavior, and future OS tools.

If the starter template is already Electron and is well structured, do not automatically migrate during MVP.

## Frontend

- React
- TypeScript
- existing template styling system
- simple state machine for voice status

Recommended application states:

```text
BOOTING
IDLE_WAKE_WORD
WAKING
CONNECTING
LISTENING
USER_SPEAKING
ASSISTANT_THINKING
ASSISTANT_SPEAKING
TOOL_PENDING
AWAITING_APPROVAL
ERROR
```

## AI / orchestration

Preferred:

- OpenAI Agents SDK for TypeScript
- `RealtimeAgent`
- `RealtimeSession`

### Current realtime model

As of 2026-08-24, use:

`gpt-realtime-2.1`

A lower-cost development option may use:

`gpt-realtime-2.1-mini`

Do not hard-code the model throughout the app. Use configuration:

```env
ASUNA_REALTIME_MODEL=gpt-realtime-2.1
```

## Voice transport

For a local desktop UI with microphone input:

**WebRTC is the preferred initial transport.**

Reasons:

- designed for low-latency media;
- Realtime Agents SDK supports the browser/WebRTC flow;
- natural interruption handling;
- suitable for desktop webviews.

Use WebSocket later when there is a server-centric requirement.

## Authentication

Never place a permanent OpenAI API key in renderer/client code.

For production-style architecture:

1. local or remote trusted backend has `OPENAI_API_KEY`;
2. client requests a short-lived Realtime client secret;
3. backend creates the ephemeral client token;
4. client connects with the temporary token.

For a personal local MVP, the trusted token-minting process may run locally, but the API key must remain outside the browser/webview bundle.

## Important billing note

A ChatGPT subscription and OpenAI API usage are separate billing systems.

Do not assume ChatGPT Plus/Pro automatically supplies API credit.

The project must therefore expose cost controls and make model selection configurable.

---

# 8. Wake Word

## Required phrase

Primary trigger:

**“Hey Asuna”**

Secondary direct trigger may later support:

- “Asuna”
- “Asuna nasılsın?”
- “Asuna beni toparla.”

But MVP wake-word detection should use one carefully trained trigger to reduce false positives.

## Recommended wake-word provider

Use an adapter interface.

```ts
interface WakeWordProvider {
  initialize(): Promise<void>;
  start(): Promise<void>;
  stop(): Promise<void>;
  onDetected(callback: (event: WakeWordEvent) => void): () => void;
}
```

First implementation candidate:

**Picovoice Porcupine**

> **REVISION (2026-08-24, ASU-008):** This candidate is superseded. Picovoice shut down its
> Free Tier on 2026-06-30 ("no non-commercial tier planned"), removed the Rust binding, and
> validates the AccessKey online at engine init — violating the local-first principle.
> The selected engine is **sherpa-onnx `KeywordSpotter`** (Apache-2.0, fully offline, running
> in the Tauri Rust process). The `WakeWordProvider` interface below is unchanged — exactly the
> replaceability this section required. See `docs/decisions/ADR-004-wake-word-provider.md`.

Original rationale (kept for the record):

- local/on-device detection;
- macOS support including Apple Silicon;
- custom wake words;
- suitable for always-listening activation.

The provider must be replaceable later.

Do not couple the rest of Asuna to a single wake-word vendor.

## Privacy behavior

When idle:

- local microphone frames may be processed only by the wake-word engine;
- do not send idle microphone audio to OpenAI;
- do not persist idle audio.

After wake:

- stop or suspend wake-word processing;
- begin active conversation;
- show visible state that the assistant is listening.

---

# 9. Conversation Lifecycle

## 9.1 Idle

```text
Wake word engine: ON
Realtime session: OFF or disconnected
Cloud audio: NONE
UI: minimal/tray
```

## 9.2 Activation

User:

> Hey Asuna

System:

1. wake-word callback fires;
2. play optional subtle activation tone;
3. open/focus minimal Asuna overlay;
4. mint ephemeral Realtime token if needed;
5. connect `RealtimeSession`;
6. load current short-term context;
7. begin microphone streaming;
8. Asuna may respond with a short acknowledgement.

Avoid long greetings.

Examples:

- “Buradayım.”
- “Dinliyorum.”
- “Söyle.”

## 9.3 Active conversation

During session:

- support user interruption;
- show live transcript;
- expose approved tools;
- write tool events to audit log;
- retrieve only relevant memory;
- keep responses concise by default.

## 9.4 Session close

Session ends when:

- explicit phrase:
  - “Tamam Asuna.”
  - “Sonra devam ederiz.”
  - “Kapat.”
- inactivity timeout;
- UI stop button;
- unrecoverable network error.

Then:

1. stop cloud audio;
2. disconnect/idle Realtime session;
3. create session summary;
4. extract candidate memories;
5. persist accepted memories;
6. update project context;
7. return to wake-word state.

---

# 10. Identity and Behavior

Asuna's personality should feel:

- warm;
- concise when work is active;
- technically competent;
- non-patronizing;
- calm;
- proactive without nagging;
- willing to challenge unclear plans;
- focused on completion;
- comfortable with Turkish and English;
- able to switch naturally between technical English terms and Turkish conversation.

Asuna should not:

- exaggerate confidence;
- pretend to have seen files it has not accessed;
- claim actions happened if tools failed;
- invent memories;
- over-talk when the user needs execution;
- repeatedly ask broad questions when enough context exists.

---

# 11. Core System Prompt Requirements

The runtime prompt should encode principles, not huge amounts of volatile data.

A conceptual prompt:

```text
You are Asuna, a persistent personal AI companion and work copilot.

Your job is to help the user think, remember, build, and finish.

You have access only to the context and tools explicitly provided to you.
Never claim you saw a file, screen, repository, task, or event unless a tool or context source provided it.

Prefer one concrete next step when the user is stuck.

Use memory carefully:
- retrieve only relevant memories,
- never invent memories,
- distinguish remembered facts from current assumptions.

For tool actions:
- read-only tools may be used when relevant,
- mutating or external actions require the configured approval policy,
- explain dangerous actions before requesting approval.

The user speaks primarily Turkish and frequently uses English technical terminology.
Respond naturally in the language of the current conversation.

Keep work conversations efficient.
Do not produce long motivational speeches when the user is asking to execute a task.

You are not merely a chat interface.
You are the conversational layer over the user's projects, memories, and approved tools.
```

The final prompt should be stored in a versioned file, for example:

`src/asuna/prompts/core.ts`

or:

`prompts/asuna-core.md`

---

# 12. Memory Architecture

Memory is central to Asuna.

Do not start with a complex vector platform unless required.

## 12.1 MVP database

Preferred:

**SQLite**

Possible libraries:

- Drizzle ORM;
- Prisma;
- better-sqlite3;
- template's existing database layer.

If the starter template already has a strong local database abstraction, preserve it.

## 12.2 Memory tables

Suggested schema:

### `memories`

```text
id
kind
title
content
summary
project_id nullable
importance
confidence
source_session_id nullable
created_at
updated_at
last_accessed_at
expires_at nullable
is_archived
embedding nullable/later
metadata_json
```

### `projects`

```text
id
name
path
description
status
primary_language
framework
git_remote nullable
last_opened_at
created_at
updated_at
metadata_json
```

### `sessions`

```text
id
started_at
ended_at
project_id nullable
summary
transcript_path nullable
model
token/cost metadata
created_at
```

### `tasks`

```text
id
project_id nullable
title
description
status
priority
source
created_at
updated_at
completed_at nullable
```

### `tool_events`

```text
id
session_id
tool_name
risk_level
arguments_redacted
approval_state
result_summary
created_at
```

---

# 13. Memory Retrieval Strategy

MVP retrieval should be deterministic before becoming clever.

Build in stages.

## Stage A — exact/project retrieval

If current project is known:

- inject project summary;
- latest project decision memories;
- latest incomplete tasks;
- last session summary.

## Stage B — semantic retrieval

Add embeddings/vector search only after enough memories exist to justify it.

Potential implementation:

- SQLite vector extension, or
- small local vector DB, or
- hosted vector layer later.

## Stage C — memory consolidation

Periodically merge duplicates.

Example:

Instead of storing:

- “User likes concise answers while coding.”
- “When coding user wants short responses.”
- “User prefers direct answers during development.”

consolidate into one durable preference.

---

# 14. Working Context vs Durable Memory

These must be separate.

## Working context

Short-lived information:

- current file;
- terminal error;
- active branch;
- current task;
- last tool result;
- current conversation.

## Durable memory

Long-lived information:

- project purpose;
- architectural decision;
- persistent preference;
- stable workflow;
- important contact/integration;
- project milestone.

Do not promote every working-context item to durable memory.

---

# 15. Project Awareness

Asuna should know what the user is working on only through explicit local context providers.

Create a `ProjectContextService`.

Responsibilities:

- register known project roots;
- detect current project;
- inspect project metadata;
- read selected context files;
- expose safe summary to the agent.

Recommended project context sources:

1. `PROJECT.md`
2. `README.md`
3. `CLAUDE.md`
4. `AGENTS.md`
5. `package.json`
6. `pyproject.toml`
7. `Cargo.toml`
8. `.git/config`
9. git status
10. current branch
11. recent commits
12. explicit user-defined notes

Do not dump the entire repository into the voice session.

---

# 16. Context Compression

For each project, maintain a machine-readable summary.

Example:

`/.asuna/context.json`

```json
{
  "projectName": "Asuna",
  "objective": "Local-first voice AI companion",
  "currentMilestone": "Realtime voice MVP",
  "activeTask": "Connect wake word to realtime session",
  "blockers": [],
  "recentDecisions": [
    "Use OpenAI Realtime Agents SDK",
    "Use gpt-realtime-2.1",
    "Keep wake-word detection local"
  ]
}
```

The exact format may evolve.

This file is not the only source of truth; it is a compact handoff artifact.

---

# 17. Tool Architecture

All computer capabilities must use explicit tool definitions.

Use a registry.

```ts
type ToolRisk = 0 | 1 | 2 | 3;

interface AsunaToolDefinition {
  name: string;
  description: string;
  risk: ToolRisk;
  requiresApproval: boolean;
  execute(args: unknown, ctx: ToolContext): Promise<ToolResult>;
}
```

## Initial tool set

### `get_current_project`

Risk: 0

Returns:

- project id;
- name;
- path;
- git branch;
- project summary.

### `read_project_file`

Risk: 0

Restrictions:

- must stay inside registered project root;
- deny `.env`, secret stores, keychains unless explicitly approved;
- enforce max file size.

### `get_git_status`

Risk: 0

### `list_recent_project_activity`

Risk: 0

### `open_project`

Risk: 1

Can:

- focus/open VS Code or configured editor.

### `create_project_note`

Risk: 1

Writes only to a dedicated `.asuna/notes/` directory during MVP.

---

# 18. Shell Tool Policy

Do not expose:

```ts
run_any_shell_command(command: string)
```

as an unrestricted model tool.

Instead create scoped tools.

Examples:

- `run_tests`
- `run_lint`
- `git_status`
- `git_diff`
- `npm_install_package`
- `start_dev_server`

Each tool should:

1. validate arguments;
2. restrict working directory;
3. have a timeout;
4. capture stdout/stderr;
5. classify risk;
6. log execution;
7. require approval when needed.

Later, a controlled shell tool can exist with an allowlist/approval gate.

---

# 19. Security Model

Security is part of the product, not a later patch.

## Secrets

Never expose:

- OpenAI permanent API key;
- GitHub tokens;
- cloud credentials;
- private keys;
- `.env` contents;
- keychain secrets;

to the model unless a very specific workflow requires it.

Tools should perform privileged operations without returning secret values.

## Filesystem sandbox

Every project tool receives a registered root.

Normalize and resolve paths.

Reject path traversal.

Example denial:

`../../.ssh/id_ed25519`

## Tool audit

Every tool call should store:

- time;
- tool;
- redacted args;
- approval;
- success/failure;
- summary.

## Visible action state

When Asuna uses a tool, the UI should show it.

The user should never wonder whether the agent is silently modifying the computer.

---

# 20. Privacy

Asuna's trust depends on predictable privacy.

Required MVP rules:

- Wake-word processing is local.
- Idle audio is not uploaded.
- Active listening is visibly indicated.
- Session transcript storage is configurable.
- Memory storage is inspectable.
- User can delete memories.
- User can disable durable memory.
- Sensitive files are blocked by default.
- Tool calls are logged.
- No hidden screen capture.

Later:

- encrypted database;
- OS keychain for credentials;
- retention policies;
- per-memory privacy classes.

---

# 21. UI / UX

The desktop UI should not become the main product.

Voice is primary.

UI exists for confidence, context, and control.

## Minimal overlay

Show:

- Asuna icon/status;
- live state;
- microphone state;
- short transcript;
- current project;
- active tool;
- stop button.

## Main window

Tabs/sections:

### Conversation
- transcript
- session history

### Projects
- registered projects
- current task
- latest summary

### Memory
- searchable memories
- edit/delete/archive
- memory type

### Tools
- enabled tools
- approval policy

### Settings
- wake word
- voice
- realtime model
- response style
- privacy
- API/billing status
- inactivity timeout

---

# 22. Suggested Repository Structure

Adapt this to the starter template.

```text
asuna/
├── PROJECT.md
├── CLAUDE.md
├── TRANSCRIPT.md
├── README.md
├── .env.example
├── docs/
│   ├── architecture/
│   │   ├── voice.md
│   │   ├── memory.md
│   │   ├── tools.md
│   │   └── security.md
│   └── decisions/
├── src/
│   ├── app/
│   ├── components/
│   ├── asuna/
│   │   ├── agent/
│   │   │   ├── realtime-agent.ts
│   │   │   ├── session-manager.ts
│   │   │   └── instructions.ts
│   │   ├── audio/
│   │   │   ├── wake-word-provider.ts
│   │   │   ├── sherpa-kws-provider.ts   # (ADR-004 revision; engine in src-tauri)
│   │   │   └── audio-state.ts
│   │   ├── memory/
│   │   │   ├── memory-service.ts
│   │   │   ├── memory-retrieval.ts
│   │   │   └── memory-extraction.ts
│   │   ├── projects/
│   │   │   ├── project-registry.ts
│   │   │   └── project-context.ts
│   │   ├── tools/
│   │   │   ├── registry.ts
│   │   │   ├── permissions.ts
│   │   │   └── implementations/
│   │   ├── security/
│   │   └── observability/
│   ├── db/
│   └── shared/
├── src-tauri/              # if using Tauri
├── tests/
└── scripts/
```

---

# 23. Configuration

Example:

```env
OPENAI_API_KEY=
ASUNA_REALTIME_MODEL=gpt-realtime-2.1
ASUNA_REALTIME_VOICE=
ASUNA_WAKE_WORD=Hey Asuna
ASUNA_MEMORY_ENABLED=true
ASUNA_TRANSCRIPT_STORAGE=true
ASUNA_TOOL_APPROVAL_MODE=safe
ASUNA_IDLE_TIMEOUT_SECONDS=45
ASUNA_LOG_LEVEL=info
```

Do not commit real secrets.

---

# 24. Realtime Agent Pseudocode

Exact SDK syntax should follow the installed/current SDK version.

Conceptually:

```ts
const asuna = new RealtimeAgent({
  name: "Asuna",
  instructions: buildAsunaInstructions(context),
  tools: [
    getCurrentProjectTool,
    getGitStatusTool,
    readProjectFileTool
  ]
});

const session = new RealtimeSession(asuna, {
  model: config.realtimeModel
});

await session.connect({
  apiKey: ephemeralClientSecret
});
```

The application should wrap this behind:

`AsunaRealtimeService`

so SDK details do not leak across the entire codebase.

---

# 25. Session Context Builder

Before each activated conversation, create a concise context package.

Example:

```ts
interface SessionBootstrapContext {
  userPreferences: Memory[];
  currentProject?: ProjectContext;
  recentSession?: SessionSummary;
  activeTasks: Task[];
  relevantMemories: Memory[];
}
```

Do not attach huge raw histories.

Use the minimum context required to be helpful.

---

# 26. Memory Extraction

After a session, run a separate memory extraction step.

Do not ask the realtime model to persist arbitrary text directly into the database.

Pipeline:

```text
conversation
↓
session summary
↓
candidate memories
↓
validation / deduplication
↓
storage
```

Candidate memory structure:

```json
{
  "kind": "project_decision",
  "content": "Asuna should keep wake-word detection local.",
  "importance": 0.9,
  "confidence": 1.0,
  "projectId": "asuna"
}
```

For highly personal/sensitive categories, consider explicit confirmation before durable storage.

---

# 27. Proactive Assistance

Proactivity is a later milestone, but design for it now.

Asuna can eventually observe safe activity metadata such as:

- active project;
- elapsed work session;
- repeated build failures;
- task switching frequency;
- unresolved errors.

It should **not** immediately monitor everything.

Start with explicit activity events generated by Asuna's own tools.

Example future trigger:

```text
Same test failed 4 times in 25 minutes.
```

Asuna:

> “Aynı test birkaç kez aynı noktada düşmüş. İstersen son iki hatayı karşılaştırayım.”

Good proactivity:

- contextual;
- rare;
- actionable;
- dismissible.

Bad proactivity:

- constant coaching;
- moral judgment;
- excessive notifications.

---

# 28. Cost Management

Voice agents can create continuous API usage.

Implement:

- session duration tracking;
- token/audio usage metadata if available;
- daily estimated cost;
- selectable model;
- idle disconnect;
- maximum session duration;
- development mode using a cheaper realtime model;
- no active Realtime session while merely waiting for wake word.

Potential modes:

### Quality
`gpt-realtime-2.1`

### Economy / development
`gpt-realtime-2.1-mini`

Do not optimize prematurely at the cost of proving the experience.

---

# 29. Observability

Log state transitions.

Example:

```text
12:10:01 WAKE_WORD_DETECTED
12:10:01 CONNECTING_REALTIME
12:10:02 REALTIME_CONNECTED
12:10:03 USER_SPEECH_STARTED
12:10:07 USER_SPEECH_ENDED
12:10:07 TOOL_CALL get_current_project
12:10:08 ASSISTANT_SPEECH_STARTED
12:10:13 ASSISTANT_SPEECH_ENDED
12:10:42 SESSION_IDLE_TIMEOUT
12:10:43 SESSION_SUMMARY_SAVED
```

Never log secrets.

In development, provide a debug console.

---

# 30. Error Handling

Asuna must fail gracefully.

Examples:

## API unavailable

Say/show:

> “Şu an ses bağlantısını kuramadım. Yerel moddayım.”

Possible fallback later:

- local text-only;
- cached project context;
- wake-word remains functional.

## Microphone unavailable

Show explicit permission/setup guidance.

## Tool error

Do not pretend success.

Say:

> “Projeyi açmayı denedim ama VS Code komutu bulunamadı.”

## Memory database error

Continue conversation without memory and surface status.

---

# 31. Testing Strategy

## Unit tests

- memory ranking;
- permission logic;
- path sandboxing;
- project detection;
- tool schemas;
- state transitions.

## Integration tests

- ephemeral token endpoint;
- Realtime session lifecycle;
- tool call round trip;
- memory storage;
- session finalization.

## Manual acceptance tests

### Voice
- say “Hey Asuna”;
- verify activation;
- speak Turkish;
- interrupt response;
- resume;
- close session.

### Memory
- tell Asuna a project decision;
- close;
- start new session;
- ask what was decided.

### Tools
- ask current project;
- open project;
- verify UI logs tool call.

### Privacy
- verify idle mode sends no cloud audio;
- inspect logs;
- verify secrets are redacted.

---

# 32. Development Phases

## Phase 0 — Template audit

**No feature coding yet.**

Agent must output:

- stack analysis;
- folder map;
- dependencies;
- reusable components;
- risks;
- recommended changes;
- migration plan.

## Phase 1 — Realtime voice

Deliver:

- temporary/manual activation button;
- Realtime connection;
- natural two-way audio;
- interruption;
- transcript;
- state UI.

This proves the hardest interaction loop before wake word.

## Phase 2 — Wake word

Deliver:

- local “Hey Asuna” detection;
- idle → active transition;
- no idle cloud audio;
- session timeout.

## Phase 3 — Memory

Deliver:

- SQLite;
- session summary;
- durable project memories;
- retrieval on next conversation;
- memory UI.

## Phase 4 — Project context

Deliver:

- project registry;
- context files;
- git metadata;
- current project tool.

## Phase 5 — One useful action

Deliver:

- open current project in editor;
- tool approval UI;
- audit event.

## Phase 6 — Focus recovery

Deliver command:

> “Asuna, beni toparla.”

Behavior:

1. inspect current project context;
2. show active task;
3. show last known blocker;
4. propose exactly one next action;
5. offer to execute a safe tool.

---

# 33. MVP Acceptance Checklist

- [ ] Existing template audited before major refactor
- [ ] App launches on macOS
- [ ] API key never shipped in renderer bundle
- [ ] Realtime session uses temporary client credential
- [ ] `gpt-realtime-2.1` is configurable
- [ ] Two-way voice works
- [ ] User can interrupt Asuna
- [ ] Live state is visible
- [ ] Local wake word detects “Hey Asuna”
- [ ] Idle audio is not sent to cloud
- [ ] Session closes on timeout
- [ ] SQLite persistence works
- [ ] At least one durable memory survives restart
- [ ] Current project can be identified
- [ ] At least one real tool works
- [ ] Tool execution is shown in UI
- [ ] Mutating actions require approval
- [ ] Errors are surfaced honestly
- [ ] Session summary is created
- [ ] User can inspect/delete memory
- [ ] README contains local run instructions

---

# 34. First Coding-Agent Task

The coding agent should receive this instruction after reading this file:

```text
Read PROJECT.md, CLAUDE.md, TRANSCRIPT.md, and the existing repository.

Do not modify code yet.

First perform a repository audit:
1. identify the current stack and architecture;
2. identify what parts of the starter template can be preserved;
3. compare the current architecture with the Asuna MVP architecture;
4. list blockers and unnecessary complexity;
5. propose the smallest migration plan that gets us to Phase 1: realtime two-way voice;
6. list exact files you would create/change;
7. identify any credentials or external setup required;
8. call out assumptions.

Prioritize a working end-to-end vertical slice over premature abstractions.

After the audit, stop and present the plan.
```

---

# 35. Definition of “Done” for Today's First Vertical Slice

The first coding session does not need the entire product.

A valuable first finish line is:

1. app runs;
2. click “Talk to Asuna”;
3. Realtime voice connects;
4. user talks in Turkish;
5. Asuna responds with low latency;
6. interruption works;
7. transcript appears;
8. disconnect works cleanly.

Then:

9. attach wake word;
10. attach memory;
11. attach tools.

**Do not block the voice proof-of-concept on perfect memory or perfect desktop automation.**

---

# 36. Long-Term Architecture Direction

Asuna may later evolve into several cooperating subsystems:

```text
                    ┌───────────────────────┐
                    │      User / Voice     │
                    └───────────┬───────────┘
                                │
                    ┌───────────▼───────────┐
                    │   Interaction Layer   │
                    │ Realtime / UI / Audio │
                    └───────────┬───────────┘
                                │
                    ┌───────────▼───────────┐
                    │      Asuna Core       │
                    │ identity + policies   │
                    └──────┬────┬────┬──────┘
                           │    │    │
             ┌─────────────┘    │    └──────────────┐
             │                  │                   │
    ┌────────▼────────┐ ┌───────▼────────┐ ┌────────▼────────┐
    │ Memory Service  │ │ Project Context │ │  Tool Runtime   │
    └────────┬────────┘ └───────┬────────┘ └────────┬────────┘
             │                  │                   │
       ┌─────▼─────┐      ┌─────▼─────┐       ┌─────▼─────┐
       │  SQLite   │      │ Git/files │       │ OS/apps   │
       └───────────┘      └───────────┘       └───────────┘
```

Later services may include:

- calendar agent;
- email agent;
- coding agent;
- research agent;
- personal knowledge agent;
- task planner;
- browser agent;
- enterprise connectors.

But they should attach to the stable Asuna Core, not replace it.

---

# 37. Commercial Direction

Potential future positioning:

> **A persistent AI work companion that understands your ongoing context and can safely act across your tools.**

Potential customer categories:

- developers;
- founders;
- executives;
- agencies;
- operations teams;
- customer support;
- internal knowledge workers.

Enterprise differentiators could include:

- private deployment;
- organization knowledge;
- role-based tool permissions;
- audit logs;
- internal MCP/tool ecosystem;
- company-specific memory boundaries;
- compliance controls;
- team handoff context.

Do not optimize the MVP for enterprise yet.

First prove one person genuinely wants Asuna running every day.

---

# 38. Product Metric That Matters First

The first meaningful metric is not number of messages.

It is:

**Does the user voluntarily call Asuna during real work because it is faster than opening another app?**

Supporting signals:

- daily activated sessions;
- time from wake word to useful response;
- sessions that lead to a completed task;
- successful context recall;
- number of manual restatements avoided;
- number of safe tool actions completed.

---

# 39. Final Engineering Principles

1. Preserve existing working template code where possible.
2. Build one vertical slice at a time.
3. Voice first.
4. Wake word local.
5. Secrets never in renderer.
6. Memory must be inspectable.
7. Tools must be scoped.
8. Dangerous actions require approval.
9. Never pretend a tool succeeded.
10. Never pretend context exists when it was not retrieved.
11. Avoid giant prompts.
12. Summarize context instead of dumping it.
13. Keep model/provider boundaries behind interfaces.
14. Make model IDs configurable.
15. Optimize for the user's actual daily workflow.
16. Finish small loops before expanding.
17. Every new capability must answer: “Does this make Asuna easier to reach, more context-aware, or more useful at completing work?”

---

# 40. Immediate Next Action

**Do not start by implementing every subsystem.**

The repository's next action is:

> Audit the starter template and produce the minimal change plan for a working `gpt-realtime-2.1` two-way voice vertical slice.

Once that is working, attach **“Hey Asuna”** as the activation mechanism.

That is the foundation.
