# CodeStudio Lite

CodeStudio Lite is a desktop-only Local AI Gateway and Provider Switcher for AI coding tools.

Core promise: **Switch AI providers instantly, without restarting your coding tools.**

CodeStudio Lite runs a local multi-protocol gateway inside the Tauri desktop app. Codex CLI, Claude Code, OpenCode, OpenClaw, and similar tools can point at one local endpoint:

```text
http://127.0.0.1:43112/v1
```

The desktop app owns the active Provider Profile and local gateway token, so external clients use a local token instead of storing real provider API keys. Switching providers updates CodeStudio Lite state; connected clients do not need to be restarted.

## First Stage Scope

This repository currently targets the Local Gateway MVP skeleton:

- Tauri 2 desktop shell with Svelte 5 and TypeScript.
- Dashboard centered on Local Gateway status, active profile, connected clients, profiles, recent activity, and problems.
- Local gateway on `127.0.0.1:43112`.
- Gateway endpoints for `/health`, `/v1/models`, `/v1/chat/completions`, `/v1/responses`, and `/v1/messages`.
- Non-streaming JSON responses and streaming SSE pass-through for OpenAI Chat Completions, OpenAI Responses, and Claude Messages upstreams.
- Active Provider Profile forwarding for concrete upstream API protocols; API keys stay in the system keychain.
- Multi-language UI foundation with Simplified Chinese and English resources, persisted through `ui.language`.
- Rust command skeleton for detection, Doctor, profile summary, app directory setup, and activity log loading.
- Setup Wizard UI skeleton for target tool, provider, credentials, model, test, preview, and apply steps.
- Local configuration directory bootstrap at `~/.codestudio-lite`.
- Architecture and security documentation.

## Supported MVP Targets

The initial detector tracks:

- Codex CLI
- Claude Code
- Gemini CLI
- OpenCode
- OpenClaw
- Node.js
- Git
- npm
- pnpm
- Bun

## Development

Install JavaScript dependencies:

```powershell
npm install
```

Run the Svelte frontend preview:

```powershell
npm run dev
```

Run the Tauri desktop app after installing Rust and the native Tauri prerequisites:

```powershell
npm run tauri:dev
```

Build the frontend:

```powershell
npm run build
```

## Product Boundary

CodeStudio Lite is a GUI-only desktop application. Do not add:

- `codestudio` CLI commands
- `doctor`, `setup`, or `switch` terminal subcommands
- headless server mode
- browser-hosted web UI
- SSH or remote server management
- local proxy failover or circuit breaker logic
- cloud sync
- usage dashboard
- AI chat or agent workspace

Allowed runtime surfaces are the Tauri desktop app, system tray, local gateway background service owned by the desktop app, GUI Provider Switcher, GUI Setup Wizard, GUI Doctor, and GUI Request Monitor.
