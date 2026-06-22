# Architecture

CodeStudio Lite is a desktop-only Local AI Gateway and Provider Switcher. It is split into a Svelte frontend and a Rust core exposed through Tauri commands.

```text
Svelte UI
  Dashboard / Gateway controls / Setup Wizard / Profiles / Settings
        |
        | Tauri commands
        v
Rust core
  gateway / detector / profile / doctor / activity log
        |
        +--> Local gateway: 127.0.0.1:43112
        |
        +--> Local system: commands / config files / ~/.codestudio-lite
```

The gateway runs inside the desktop process. It is not a CLI, headless daemon, browser-hosted web UI, SSH service, or remotely managed server.

## Frontend

The frontend lives in `src/`.

- `src/App.svelte` owns the route shell and refresh lifecycle.
- `src/lib/i18n.ts` owns frontend locale state, translation resources, and the typed translation helper.
- `src/routes/Dashboard.svelte` shows Local Gateway state first, then active profile state, connected client status, Provider Profiles, problems, and activity.
- `src/routes/SetupWizard.svelte` is the MVP 0.2 wizard skeleton.
- `src/components/` contains shared status, problem, activity, and secret-input components.
- `src/lib/api.ts` wraps Tauri commands and provides browser-preview mock data when the app is opened outside Tauri.

## Rust Core

The Rust backend lives in `src-tauri/src/`.

- `commands/` exposes Tauri commands only.
- `core/gateway.rs` owns the desktop-local OpenAI-compatible gateway skeleton.
- `core/tool_registry.rs` defines supported tools, commands, config paths, and suggested install commands.
- `core/detector.rs` executes version checks and derives install/config states.
- `core/profile.rs` creates `~/.codestudio-lite`, manages SQLite-backed profiles, and loads profile summaries.
- `core/profile.rs` also reads and updates `ui.language` and other application settings through Tauri commands.
- `core/doctor.rs` turns detection and file checks into a Doctor report.
- `core/activity_log.rs` stores local events in JSONL.
- `core/gateway_request_log.rs` stores metadata-only gateway request records in SQLite.
- `core/credentials.rs` stores Provider API keys in the Windows Credential Manager for the current Windows build.
- `core/upstream_http.rs` forwards upstream HTTP requests through the OS networking stack, including chunked SSE pass-through.

## Local Gateway

The MVP gateway listens on:

```text
Host: 127.0.0.1
Port: 43112
Base URL: http://127.0.0.1:43112/v1
```

Supported skeleton endpoints:

```text
GET  /health
GET  /v1/models
POST /v1/chat/completions
POST /v1/responses
POST /v1/messages
```

`/v1/*` requests require:

```text
Authorization: Bearer codestudio-local-<random-token>
```

The gateway reads the active Provider Profile on each request. Switching a profile updates the active profile pointer and the next gateway request sees the new model/provider without restarting the gateway or connected clients.

## Provider Profiles

## Localization

The frontend uses `src/lib/i18n.ts` for locale state and translation dictionaries. The current MVP ships `zh-CN`, `zh-TW`, and `en-US`. Language preference is persisted in `~/.codestudio-lite/app_state.sqlite` under `ui.language`; browser preview also keeps a localStorage fallback.

New UI strings should be added to `zhCN` first, mirrored in `enUS`, and consumed through `$t("key")` rather than embedded directly in route components.

Provider Profiles describe upstream providers for the Local Gateway, not per-request client rewrites. The MVP schema stores:

```text
id / name / provider / protocol / base_url / model
auth.api_key = keychain reference
timeout_seconds
metadata.created_at / metadata.updated_at / metadata.last_test_status
```

`protocol` is one of `openai-chat-completions`, `openai-responses`, `anthropic-messages`, or `google-gemini`. Config file mode only allows the protocols that the selected tool can write natively; gateway mode can route any supported gateway protocol. Codex, OpenCode, and OpenClaw can be bootstrapped once to the localhost gateway where supported; later profile switching changes only CodeStudio Lite active state.

## Request Monitor

Gateway request monitoring writes metadata-only records to `~/.codestudio-lite/app_state.sqlite`. It does not persist prompts, completions, tool arguments, or file contents by default.

## Forwarding

The forwarding slice separates the client-facing API protocol from the active Provider Profile protocol. Requests can be converted between `openai-chat-completions`, `openai-responses`, `anthropic-messages`, and `google-gemini`: the gateway rewrites the request body to the upstream protocol, forwards it to the profile endpoint, then reshapes the upstream response back to the client protocol. Conversion preserves text, multimodal content blocks, function/tool calls, and protocol-specific usage details where the target protocol has an equivalent shape. Streaming requests use SSE event-level translators: same-protocol streaming can still pass through, while cross-protocol streaming extracts upstream text deltas, tool-call deltas, and usage summaries and emits client-protocol events.

Upstream routes are derived from the active profile protocol: `openai-chat-completions` posts to `<profile.base_url>/chat/completions`, `openai-responses` posts to `<profile.base_url>/responses`, `anthropic-messages` posts to `<profile.base_url>/messages` with Anthropic-style `x-api-key` authentication, and `google-gemini` posts to `<profile.base_url>/models/<model>:generateContent` or `<profile.base_url>/models/<model>:streamGenerateContent?alt=sse` with `x-goog-api-key`.

On Windows, forwarding uses WinHTTP and credentials use Windows Credential Manager. This keeps provider TLS and credential storage on the OS path without adding a heavyweight async HTTP dependency to the desktop process.

## Data Directory

The app stores its own state under:

```text
~/.codestudio-lite/
  app_state.sqlite
  downloads/
```

Profiles, active selections, settings, gateway state, usage-query state, request logs, and internal backup data are stored in SQLite. Downloaded installers and temporary packages are kept under `downloads/`.

## Write Policy

Client bootstrap writes to AI tool files must go through:

1. user confirmation,
2. backup manifest creation,
3. temporary-file write,
4. atomic rename,
5. post-write verification,
6. activity logging.
