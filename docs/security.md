# Security Model

CodeStudio Lite manages a desktop-local gateway, credentials, and configuration files for local AI coding tools. The default posture is explicit, reversible, localhost-only, and quiet.

## Secrets

- API keys must not be written to logs.
- API keys provided through the Setup Wizard are stored in the system keychain in the current Windows build.
- AI client configs should receive only the local CodeStudio token, not provider API keys.
- Provider Profile files store keychain references such as `keychain:codestudio-lite/<profile>/api_key`, not plaintext provider keys.
- Gateway forwarding replaces the client local token with the provider API key only inside the desktop process.
- UI controls display API keys as masked values by default.
- Exported profiles must omit API keys unless the user explicitly chooses otherwise.
- Deep Link imports must open a confirmation screen instead of writing immediately.

## Local Gateway

- The gateway listens on `127.0.0.1` by default, never `0.0.0.0`.
- `/v1/*` routes require `Authorization: Bearer codestudio-local-<random-token>`.
- The local token is stored in `~/.codestudio-lite/app_state.sqlite` and shown only as a masked preview in the UI.
- Unauthorized requests must not reach an upstream provider.
- Request logs default to metadata only: timestamp, client, method, path, provider, model, status, latency, and error summary.
- Prompt text, completions, tool-call arguments, and file contents are not persisted by default.

## Configuration Writes

All writes outside `~/.codestudio-lite` must follow this sequence:

1. show the target file list,
2. show the profile and provider being applied,
3. confirm the operation,
4. create a backup,
5. write to a temporary file,
6. atomically rename into place,
7. re-read and validate the target config,
8. show success or a concrete error.

## Install Commands

Install actions are never silent.

- Show the complete command before running it.
- Require explicit user confirmation.
- Provide a copy-command path.
- Stream logs to the Install Logs view.
- Avoid `curl | sh` style commands in built-in installers.
- Re-run detection after the command exits.

## Excluded Early Features

The MVP must not include CLI behavior, headless server behavior, browser-hosted web UI, SSH remote mode, cloud sync, usage dashboard, AI chat, patch review, session management, failover, circuit breaker, MCP management, skills marketplace, or complex cross-protocol conversion. These features expand the trusted surface and make cleanup and rollback harder.
