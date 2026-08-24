# TRANSCRIPT.md — Asuna Origin Conversation

> This is an edited project-origin transcript and requirements record based on the conversation that led to Asuna. Filler acknowledgements and repeated speech-recognition fragments are removed where they do not change meaning. Product intent, motivation, decisions, and requested behavior are preserved.

---

## 1. Starting point

The conversation began with the user describing feeling tired, stuck, and unable to attach themselves to a single ongoing project.

They explained that they are in the final year of computer engineering, develop multiple projects, and currently rely mostly on piecework instead of a stable ongoing job.

A recurring frustration was:

- many ideas;
- many projects;
- difficulty carrying one project to completion;
- attention shifting away from work;
- ending up lying down or reading on the phone instead of progressing;
- feeling that a better personal work system should exist.

The core realization was not “I have no projects.”

It was:

> “Kendimi bir şeye bağlayamıyorum.”

And later:

> “Birçok projem var ama hepsini bir şekilde geçiştiriyorum veya tıkandığım noktalar oluyor, devam ettiremiyorum.”

---

## 2. Earlier product idea discussed

The user described a separate idea inspired by something seen in China:

A platform where people can experience the psychological process of shopping even when they do not actually spend money, with optional real product links if they choose to purchase.

The insight behind the idea was that people may enjoy:

- browsing;
- choosing;
- adding;
- progressing through shopping steps;
- acquiring something new;

even when the purchased item quickly becomes unimportant.

This part of the conversation established an important pattern:

The user generates product concepts naturally, but the larger problem is maintaining execution momentum.

---

## 3. Need for continuous presence

While talking, the user realized they may work better when there is an available conversational presence around them.

They said, in effect:

> “Ben sanırım etrafta ses olmasını arayan bir insanım. Sürekli bir iletişim halinde olduğum bir şey olması gerekiyormuş gibi düşünüyorum.”

This led directly to the Asuna concept.

The user asked whether the current conversational AI experience could be integrated into their computer to create something like Jarvis:

- always reachable;
- aware of what the user is doing;
- able to support projects;
- able to give direction;
- able to talk during moments of confusion;
- able to remember the user over time.

This is the product's true origin.

---

## 4. Initial Asuna concept

The desired system was described as a personal Jarvis-like assistant.

Core requirements emerged:

### It should know the user

Not merely receive a prompt each time.

It should accumulate useful, persistent context.

### It should know the user's work

It should understand projects, current progress, blockers, and recent decisions through explicit access to project data/tools.

### It should be immediately reachable

Opening an app, navigating to a chat, or reconstructing context creates friction.

The desired interaction is:

> “Hey Asuna”

followed immediately by natural conversation.

### It should support focus recovery

The user specifically wants help when attention has fragmented and returning to work is difficult.

A representative command is:

> “Asuna beni bir toparla.”

Asuna should be able to respond using actual context, not generic productivity advice.

---

## 5. Voice requirements

The user explicitly preferred a wake word over a keyboard shortcut.

Desired behavior:

> “Hey Asuna”

or conversational variants such as:

> “Asuna nasılsın?”

> “Asuna beni bir toparla.”

The goal is not to continuously upload everything the user says.

The user mentioned they might be singing or making unrelated sounds and that those do not need to be processed as requests.

Therefore the architecture should distinguish:

- local idle wake-word monitoring;
- active AI conversation after activation.

---

## 6. Why the product is personally important

The user connected the product to a larger personal execution problem.

They described repeatedly starting things and leaving them unfinished after attention is diverted.

The desired result is not merely another side project.

It is a system that could help create a new work loop:

- recognize what is happening;
- reduce context switching;
- make recovery easier;
- finish projects;
- create visible progress.

The user expressed a strong desire to reach a moment where a product is not only attempted, but completed and used by real people:

> “Evet, ben bunu denedim, başardım ve şu anda işleyen bir sisteme dönüştü ve insanlar bunu kullanıyor.”

This sentence should remain a product-development north star.

Asuna should help ship Asuna.

---

## 7. Commercial possibility

During the conversation, the user immediately saw a possible business direction.

The personal assistant could later become something sold to larger organizations.

Potential enterprise value comes from the same primitives:

- conversational interface;
- persistent context;
- work awareness;
- controlled tool use;
- permissions;
- auditability;
- organization-specific knowledge.

However, the first version should be built for one real user rather than abstract enterprise requirements.

---

## 8. Naming

The assistant's name is:

# Asuna

Desired activation phrase:

# Hey Asuna

The name should be treated consistently across UI, prompt identity, docs, configuration, and wake-word model.

---

## 9. First architectural discussion

Early architecture ideas included:

- desktop application;
- React-based UI;
- Tauri or Electron;
- local persistence;
- voice API;
- memory;
- tools such as VS Code/Git;
- future proactive behavior.

The architecture later became more precise:

Asuna should be thought of as a personal AI operating layer rather than “a model.”

The model is replaceable.

Asuna itself is the orchestration system around:

- identity;
- context;
- memory;
- voice;
- permissions;
- tools;
- local state.

---

## 10. Proactive assistance

A particularly important proposed behavior was that Asuna should eventually notice patterns in work and intervene carefully.

Example:

If the user has been fighting the same error repeatedly:

> “İki saattir aynı hatayla boğuşuyorsun, istersen birlikte loglara bakalım.”

The user reacted positively to this concept.

The product should therefore be designed so that proactivity can exist later.

However, continuous invasive monitoring is not required for MVP.

Proactivity must be:

- contextual;
- limited;
- useful;
- transparent;
- dismissible.

---

## 11. MVP urgency

The user explicitly wanted to begin immediately and see an MVP as soon as possible.

The goal was described as:

> “MVP'yi bugün görsem çok güzel olabilir.”

The initial MVP should therefore prioritize a vertical slice rather than architecture perfection.

The essential first demonstration is:

1. Asuna can be activated.
2. The user can speak naturally.
3. Asuna speaks naturally.
4. Conversation can be interrupted.
5. Some memory survives.
6. At least one project/computer capability exists.

---

## 12. Existing development environment

At the time of the conversation:

- a starter template already existed;
- the user copied the template;
- an `Asuna` folder/project was created;
- Codex was open;
- Claude Code was also part of the intended development workflow;
- skills/template setup had been activated.

The explicit intention was to provide a detailed Markdown specification to the coding agent so it could transform the existing starter template into an Asuna-specific application.

Therefore:

**Do not assume a blank repository.**

The first coding task must be repository analysis.

---

## 13. Local vs remote project decision

The user asked whether the project should initially be local or remote.

The decision was:

- begin local;
- use Git from the start;
- later keep a private remote repository as backup/collaboration history.

The reason was fast iteration and direct access to the computer environment.

This still matches the local-first product direction.

---

## 14. Documentation request

The user asked for:

- a detailed project Markdown document;
- a transcript/record of the conversation;
- a file that could be given to Codex/Claude Code;
- enough technical depth that the coding agent could rapidly understand the intended product.

The desired document was not meant to be a short prompt.

It was meant to function as a durable technical/product specification.

This repository therefore contains:

- `PROJECT.md` — architecture and product source of truth;
- `CLAUDE.md` — coding-agent operational instructions;
- `TRANSCRIPT.md` — origin conversation and requirements history.

---

## 15. Important correction from the original live discussion

During the live discussion, model names were discussed informally and some references were provisional.

For implementation, always follow current official OpenAI API documentation rather than old conversational model names.

As of 2026-08-24, the project specification selects:

- `gpt-realtime-2.1` for the main realtime voice path;
- `gpt-realtime-2.1-mini` as a possible lower-cost development option.

Model IDs must remain configuration, not product identity.

---

## 16. Another important correction: ChatGPT subscription vs API

The user's existing ChatGPT subscription is useful for ChatGPT/Codex product access according to the terms of those products, but the OpenAI API has its own billing.

The Asuna application should not assume the ChatGPT subscription provides Realtime API credits.

The developer must configure API access/billing separately.

This is why cost awareness belongs in the architecture.

---

## 17. Final distilled requirements

The conversation can be reduced to this product brief:

### Build a desktop AI companion named Asuna that:

- is available through “Hey Asuna”;
- does local wake-word detection;
- starts a low-latency voice conversation;
- understands Turkish naturally;
- can be interrupted;
- remembers useful information over time;
- understands registered software projects;
- can inspect safe project context;
- can use controlled local tools;
- requires approval for risky actions;
- helps the user recover focus;
- does not upload idle room audio;
- is transparent about what it knows and what it does;
- starts as a personal product;
- can later evolve into a commercial platform.

---

## 18. The core “beni toparla” flow

This deserves first-class treatment.

User:

> “Asuna, beni toparla.”

Expected Asuna behavior:

1. Determine whether a current project is known.
2. Retrieve the latest project session summary.
3. Retrieve the last active task.
4. Retrieve recent blockers/decisions.
5. Briefly state where things stand.
6. Propose **one** next concrete action.
7. Ask/offer to use a safe tool if useful.

Example:

> “Asuna projesindeydin. Son hedefimiz Realtime bağlantısını çalıştırmaktı; wake word'ü henüz bağlamadık. Şu an tek işimiz ses oturumunu tarayıcıdan başarıyla açmak. İstersen mevcut Realtime dosyalarını okuyup bağlantı hatasını bulayım.”

Not:

This response should come from real project state, not invented state.

---

## 19. Product philosophy established by the conversation

Asuna should lower the activation energy required to resume meaningful work.

This is more important than having hundreds of features.

The best version of Asuna is one the user reaches for naturally during the workday because it already understands enough context to save effort.

The project should therefore resist becoming:

- another dashboard;
- another generic chat UI;
- another giant todo application;
- another over-engineered multi-agent demo.

The center of gravity is the relationship between:

**voice + context + memory + action**

---

## 20. Starting instruction for the coding agent

After reading these docs, the coding agent should begin with repository inspection.

It should not immediately scaffold a replacement application.

It should identify the smallest path from the existing starter template to the first live Asuna conversation.

The recommended first finish line is:

> Press a temporary activation control → speak → hear Asuna → interrupt Asuna → see transcript → cleanly disconnect.

Then replace the temporary activation control with:

> **Hey Asuna**

Then add:

> memory

Then:

> project context

Then:

> tools

This order protects the project from getting stuck in infrastructure before the key experience exists.

---

# End of origin transcript
