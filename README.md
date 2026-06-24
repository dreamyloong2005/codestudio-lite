# CodeStudio Lite

CodeStudio Lite 是面向本机 AI 编程工具的桌面控制台。它把本地 Provider Profile、localhost 网关、工具检测、安装更新、配置写入、用量查询和桌面端补丁流程放在同一个 Tauri 应用里，目标是让 Codex、Claude、Gemini、OpenCode、OpenClaw、Hermes 等工具可以在不同供应商和模型之间切换，而不需要反复手动编辑配置文件。

> English documentation starts at [English](#english).

## 核心能力

- **本地 AI 网关**：在 `127.0.0.1:43112` 提供 OpenAI / Anthropic / Gemini 风格接口，并用当前激活的 Provider Profile 转发请求。
- **Provider Profile 管理**：按工具管理官方配置、自定义供应商、协议、模型、Base URL 和系统凭据中的 API Key。
- **配置模式与网关模式**：配置模式直接写入目标工具的原生配置；网关模式把工具指向 CodeStudio Lite 的本地端点，后续只切换本应用内的激活档案。
- **桌面端管理**：提供 Codex 桌面端与 Claude Desktop 的独立页面，用于检测、下载、校验、安装、更新、卸载和启动。
- **Claude Desktop 汉化启动**：支持在 Windows / macOS 上以运行时注入方式启用 Claude Desktop 中文界面，不直接破坏官方安装目录。
- **环境体检与修复**：检测工具安装状态、版本、配置状态、PATH 修复建议、环境变量冲突和系统依赖。
- **请求监控与隐私过滤**：记录元数据级请求日志，支持关闭、检测、脱敏、阻断四种隐私过滤模式。
- **用量查询**：为 Profile 配置通用余额、New API、官方余额接口、Token Plan 或自定义脚本；Codex 官方 OAuth Profile 可读取本机官方登录缓存查询用量。
- **中英文界面**：应用内提供简体中文、繁体中文和英文资源，并把语言偏好保存到本地状态库。

## 本地网关

默认网关地址：

```text
http://127.0.0.1:43112/v1
```

工具专属地址：

```text
http://127.0.0.1:43112/tools/codex/v1
http://127.0.0.1:43112/tools/claude-desktop/v1
```

已实现的入口包括：

```text
GET  /health
GET  /v1/models
POST /v1/chat/completions
POST /v1/responses
POST /v1/messages
POST /v1beta/models/<model>:generateContent
POST /v1beta/models/<model>:streamGenerateContent
```

`/v1/*` 请求需要本地令牌：

```text
Authorization: Bearer codestudio-local-<random-token>
```

网关会读取当前激活的 Provider Profile，把客户端请求转换为上游协议，再把响应转换回客户端协议。当前支持的上游协议包括：

- OpenAI Chat Completions
- OpenAI Responses
- Anthropic Messages
- Google Gemini

同协议请求可以直接转发；跨协议请求会转换文本、多模态内容块、工具调用、用量字段和 SSE 流式事件。

## 支持的工具

AI 工具检测与配置：

- Codex CLI
- Codex VS Code
- Codex Desktop
- Claude Desktop
- Claude Code
- Claude VS Code
- Gemini CLI
- Gemini Code Assist
- OpenCode
- OpenClaw
- Hermes

系统依赖检测：

- Node.js
- Git
- npm
- pnpm
- Bun

桌面端专门页面当前覆盖：

- **Codex Desktop**：Windows App / Portable 安装类型，macOS DMG 来源选择，下载进度、SHA-256 校验、安装、更新、卸载、启动、历史会话同步和插件入口解锁补丁开关。
- **Claude Desktop**：Windows App 为主，只有检测到旧 EXE 安装时才显示 EXE 页；支持官方下载、进度反馈、安装、更新、卸载、启动，以及中文运行时注入。

## 平台状态

- **Windows**：主要开发与验证平台。支持 Windows Credential Manager、WinHTTP 转发、MSIX / Portable 安装流、Claude Desktop Windows App 检测与自动化。
- **macOS**：支持 Tauri 构建、系统 Keychain、Codex / Claude Desktop 的 macOS 启动与部分安装更新路径；Claude Desktop 中文注入依赖辅助功能授权。
- **Linux**：前端和部分检测逻辑可运行，但桌面客户端管理、系统凭据和部分安装器能力不完整。

## 安全与数据

CodeStudio Lite 的默认安全边界是本机、显式、可回滚：

- 网关只监听 `127.0.0.1`，不提供远程服务。
- 外部 AI 客户端只拿到本地 CodeStudio token，不直接写入真实供应商 API Key。
- Provider API Key 和用量查询密钥优先写入系统凭据管理器 / Keychain。
- 请求日志默认只保存元数据，不保存 prompt、completion、工具参数或文件内容。
- 写入目标工具配置前会预览变更、创建备份、临时文件写入、原子替换并再次验证。
- 安装、更新、卸载都要求用户确认，并显示命令、日志、退出码和检测结果。

本地数据目录：

```text
~/.codestudio-lite/
  app_state.sqlite
  downloads/
```

SQLite 中保存 Profile、激活状态、设置、网关配置、请求日志、用量查询配置和备份索引；下载目录保存安装包和临时文件。

## 开发

准备依赖：

```powershell
npm install
```

启动前端开发服务器：

```powershell
npm run dev
```

启动 Tauri 桌面应用：

```powershell
npm run tauri:dev
```

构建前端：

```powershell
npm run build
```

构建桌面应用：

```powershell
npm run tauri:build
```

运行检查和单元测试：

```powershell
npm run check
npm run test:unit
npm test
```

macOS 辅助脚本：

```bash
npm run tauri:build:dmg
npm run tauri:sign:macos
```

## 项目结构

```text
src/
  App.svelte                 应用路由、刷新生命周期、桌面页入口
  routes/                    Dashboard、Gateway、Profiles、Settings、桌面端页面
  components/                状态、图标、提示、活动日志等共享组件
  lib/api.ts                 Tauri 命令封装与浏览器预览 fallback
  lib/locales/               zh-CN、zh-TW、en-US 翻译资源

src-tauri/src/
  commands/                  Tauri 命令边界
  core/gateway.rs            本地网关、协议转换、SSE 转发、隐私过滤
  core/profile.rs            Profile、配置写入、原生配置预览和备份流程
  core/detector.rs           工具与系统环境检测
  core/tool_installer.rs     通用工具安装、更新、卸载和进度事件
  core/codex_client.rs       Codex Desktop 下载、校验、安装、启动与补丁选项
  core/claude_desktop_patch.rs Claude Desktop 运行时注入、调试器开启和汉化逻辑
  core/usage_query.rs        Profile 用量查询脚本和官方 OAuth 用量读取
  core/storage.rs            SQLite 状态、日志、备份和用量结果
```

## 产品边界

CodeStudio Lite 是 GUI-only 桌面应用。当前仓库不计划加入：

- `codestudio` CLI 或 headless server 模式
- 浏览器托管的 Web UI
- SSH / 远程服务器管理
- 云同步
- AI 聊天工作区
- MCP 市场或复杂插件市场
- 远程代理故障转移 / 熔断器

## English

CodeStudio Lite is a desktop control center for local AI coding tools. It brings Provider Profiles, a localhost gateway, tool detection, install/update flows, config writes, usage queries, and desktop-client patch workflows into one Tauri app, so tools such as Codex, Claude, Gemini, OpenCode, OpenClaw, and Hermes can switch providers and models without repeated manual config edits.

## Core Features

- **Local AI Gateway**: serves OpenAI / Anthropic / Gemini style APIs on `127.0.0.1:43112` and forwards requests through the active Provider Profile.
- **Provider Profile management**: stores official profiles, custom providers, protocols, models, base URLs, and API keys kept in the system credential store.
- **Config mode and Gateway mode**: Config mode writes native tool configuration; Gateway mode points tools at CodeStudio Lite so future provider switches happen inside the app.
- **Desktop client management**: dedicated Codex Desktop and Claude Desktop pages for detection, download, hash verification, install, update, uninstall, and launch.
- **Claude Desktop localized launch**: enables Simplified Chinese for Claude Desktop on Windows / macOS through runtime injection without modifying the official installation in place.
- **Environment checks and repair hints**: detects installation state, versions, config state, PATH repair opportunities, environment-variable conflicts, and system dependencies.
- **Request monitor and privacy filter**: keeps metadata-only request logs and supports off, detect, redact, and block modes.
- **Usage queries**: supports general balance, New API, official balance API, Token Plan, and custom scripts; Codex official OAuth profiles can query usage from the local official login cache.
- **Multilingual UI**: ships Simplified Chinese, Traditional Chinese, and English resources with persisted language preference.

## Local Gateway

Default gateway URL:

```text
http://127.0.0.1:43112/v1
```

Tool-scoped URLs:

```text
http://127.0.0.1:43112/tools/codex/v1
http://127.0.0.1:43112/tools/claude-desktop/v1
```

Implemented routes:

```text
GET  /health
GET  /v1/models
POST /v1/chat/completions
POST /v1/responses
POST /v1/messages
POST /v1beta/models/<model>:generateContent
POST /v1beta/models/<model>:streamGenerateContent
```

`/v1/*` requests require a local token:

```text
Authorization: Bearer codestudio-local-<random-token>
```

The gateway reads the active Provider Profile, converts the client request to the upstream protocol, and converts the response back to the client protocol. Supported upstream protocols are:

- OpenAI Chat Completions
- OpenAI Responses
- Anthropic Messages
- Google Gemini

Same-protocol requests can pass through directly. Cross-protocol requests translate text, multimodal content blocks, tool calls, usage fields, and SSE streaming events.

## Supported Tools

AI tool detection and configuration:

- Codex CLI
- Codex VS Code
- Codex Desktop
- Claude Desktop
- Claude Code
- Claude VS Code
- Gemini CLI
- Gemini Code Assist
- OpenCode
- OpenClaw
- Hermes

System dependency detection:

- Node.js
- Git
- npm
- pnpm
- Bun

Dedicated desktop-client pages currently cover:

- **Codex Desktop**: Windows App / Portable install kinds, macOS DMG source selection, download progress, SHA-256 verification, install, update, uninstall, launch, history sync on launch, and force plugin-entry unlock patch toggle.
- **Claude Desktop**: Windows App first; the deprecated EXE tab appears only when an old EXE install is detected. The page supports official download, progress reporting, install, update, uninstall, launch, and runtime Chinese localization.

## Platform Status

- **Windows**: primary development and verification platform. Supports Windows Credential Manager, WinHTTP forwarding, MSIX / Portable install flows, Claude Desktop Windows App detection, and automation.
- **macOS**: supports Tauri builds, system Keychain, Codex / Claude Desktop launch and selected install/update flows; Claude Desktop localization depends on Accessibility permission.
- **Linux**: the frontend and parts of detection can run, but desktop-client management, system credentials, and several installer paths are incomplete.

## Security And Data

CodeStudio Lite defaults to a local, explicit, reversible security model:

- The gateway listens on `127.0.0.1` only and does not expose a remote service.
- External AI clients receive a local CodeStudio token instead of real provider API keys.
- Provider API keys and usage-query secrets are stored in the system credential store / Keychain where supported.
- Request logs store metadata only by default, not prompts, completions, tool arguments, or file contents.
- Native config writes preview changes, create backups, write temporary files, atomically replace targets, and verify after write.
- Install, update, and uninstall actions require confirmation and show commands, logs, exit codes, and detection results.

Local data directory:

```text
~/.codestudio-lite/
  app_state.sqlite
  downloads/
```

SQLite stores profiles, active selections, settings, gateway config, request logs, usage-query config, and backup indexes. The downloads directory stores installers and temporary packages.

## Development

Install dependencies:

```powershell
npm install
```

Run the frontend dev server:

```powershell
npm run dev
```

Run the Tauri desktop app:

```powershell
npm run tauri:dev
```

Build the frontend:

```powershell
npm run build
```

Build the desktop app:

```powershell
npm run tauri:build
```

Run checks and unit tests:

```powershell
npm run check
npm run test:unit
npm test
```

macOS helper scripts:

```bash
npm run tauri:build:dmg
npm run tauri:sign:macos
```

## Repository Map

```text
src/
  App.svelte                 App routing, refresh lifecycle, desktop-client entry points
  routes/                    Dashboard, Gateway, Profiles, Settings, desktop-client pages
  components/                Shared status, icon, notice, and activity-log components
  lib/api.ts                 Tauri command wrapper and browser-preview fallback
  lib/locales/               zh-CN, zh-TW, en-US resources

src-tauri/src/
  commands/                  Tauri command boundary
  core/gateway.rs            Local gateway, protocol conversion, SSE forwarding, privacy filter
  core/profile.rs            Profiles, config writes, native previews, backup flow
  core/detector.rs           Tool and environment detection
  core/tool_installer.rs     Generic install, update, uninstall, and progress events
  core/codex_client.rs       Codex Desktop download, verification, install, launch, patch options
  core/claude_desktop_patch.rs Claude Desktop runtime injection, debugger enabling, localization
  core/usage_query.rs        Profile usage-query scripts and official OAuth usage
  core/storage.rs            SQLite state, logs, backups, and usage results
```

## Product Boundary

CodeStudio Lite is a GUI-only desktop app. This repository currently does not target:

- `codestudio` CLI commands or headless server mode
- Browser-hosted Web UI
- SSH / remote server management
- Cloud sync
- AI chat workspace
- MCP marketplace or complex plugin marketplace
- Remote proxy failover / circuit breaker logic
