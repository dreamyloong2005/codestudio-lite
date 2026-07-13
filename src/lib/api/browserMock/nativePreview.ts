import type { NativeConfigDiffLine, PreviewProfileApplyResult, ProfileDraft, ProviderApplyMode } from "../../../types";
import { canonicalProfileToolId as canonicalProfileApp, profileSupportsConfigProtocol } from "../../profiles/catalog";
import { browserProtocolLabel as mockProtocolLabel, normalizeBrowserProtocol as normalizeMockProtocol, providerIsOfficial, providerRequiresApiKey } from "./profilePolicy";

const PROTOCOL_OPENAI_CHAT_COMPLETIONS = "openai-chat-completions";
const PROTOCOL_OPENAI_RESPONSES = "openai-responses";
const PROTOCOL_ANTHROPIC_MESSAGES = "anthropic-messages";
const PROTOCOL_GOOGLE_GEMINI = "google-gemini";
const CLAUDE_DESKTOP_PROFILE_ID = "00000000-0000-4000-8000-000000157210";
const CLAUDE_DESKTOP_DEFAULT_ROUTE_ID = "claude-sonnet-4-6";
const CLAUDE_DESKTOP_DEFAULT_ROUTES = ["claude-sonnet-4-6", "claude-opus-4-8", "claude-haiku-4-5", "claude-fable-5"];

function normalizeMockProfileReviewModel(app: string, value?: string | null): string | null {
  if (canonicalProfileApp(app) !== "codex") return null;
  return value?.trim() || null;
}

function isCodexFamilyApp(app: string): boolean {
  return canonicalProfileApp(app) === "codex";
}

function mockToolConfigPath(toolId: string): string | null {
  const paths: Record<string, string> = {
    codex: "~/.codex/config.toml",
    claude: "~/.claude",
    "claude-desktop": `~/AppData/Local/Claude-3p/configLibrary/${CLAUDE_DESKTOP_PROFILE_ID}.json`,
    gemini: "~/.gemini",
    "gemini-code-assist": "~/AppData/Roaming/Code/User/settings.json",
    opencode: "~/.config/opencode",
    openclaw: "~/.openclaw",
    hermes: "~/.hermes/config.yaml",
    grok: "~/.grok/config.toml",
    pi: "~/.pi/agent/models.json"
  };
  return paths[canonicalProfileApp(toolId)] ?? null;
}

export function mockNativeConfigPath(app: string, mode: ProviderApplyMode, provider: string): string | null {
  const canonicalApp = canonicalProfileApp(app);
  if (mode === "gateway") {
    if (isCodexFamilyApp(canonicalApp)) {
      return "~/.codex/config.toml";
    }
    if (canonicalApp === "claude-desktop") {
      return mockClaudeDesktopProfilePath();
    }
    if (canonicalApp === "claude") {
      return "~/.claude/settings.json";
    }
    if (canonicalApp === "gemini") {
      return "~/.gemini/.env";
    }
    if (canonicalApp === "opencode") {
      return "~/.config/opencode/opencode.json";
    }
    if (canonicalApp === "openclaw") {
      return "~/.openclaw/openclaw.json";
    }
    if (canonicalApp === "hermes") {
      return "~/.hermes/config.yaml";
    }
    if (canonicalApp === "grok") {
      return "~/.grok/config.toml";
    }
    if (canonicalApp === "pi") {
      return "~/.pi/agent/models.json";
    }
    return null;
  }
  if (canonicalApp === "claude-desktop") {
    return mockClaudeDesktopProfilePath();
  }
  if (providerIsOfficial(provider) && !isCodexFamilyApp(canonicalApp)) {
    return null;
  }
  if (isCodexFamilyApp(canonicalApp)) {
    return "~/.codex/config.toml";
  }
  if (canonicalApp === "claude") {
    return "~/.claude/settings.json";
  }
  if (canonicalApp === "gemini") {
    return "~/.gemini/.env";
  }
  if (canonicalApp === "gemini-code-assist") {
    return "~/AppData/Roaming/Code/User/settings.json";
  }
  if (canonicalApp === "opencode") {
    return "~/.config/opencode/opencode.json";
  }
  if (canonicalApp === "openclaw") {
    return "~/.openclaw/openclaw.json";
  }
  if (canonicalApp === "hermes") {
    return "~/.hermes/config.yaml";
  }
  if (canonicalApp === "grok") {
    return "~/.grok/config.toml";
  }
  if (canonicalApp === "pi") {
    return "~/.pi/agent/models.json";
  }
  return null;
}

function mockClaudeDesktopProfilePath(): string {
  return `~/AppData/Local/Claude-3p/configLibrary/${CLAUDE_DESKTOP_PROFILE_ID}.json`;
}

function mockClaudeDesktopGatewayBaseUrl(): string {
  return "http://127.0.0.1:43112/tools/claude-desktop";
}

export function mockNativeConfigPreview(
  profile: ProfileDraft,
  nativeConfigPath: string | null,
  mode: ProviderApplyMode
): PreviewProfileApplyResult["nativeDiff"] {
  if (!isCodexFamilyApp(profile.app)) {
    return withMockNativeContent(mockNonCodexNativeConfigPreview(profile, nativeConfigPath, mode));
  }

  if (mode === "config") {
    const wireApi = mockCodexWireApi(profile.protocol);
    if (!wireApi) {
      return null;
    }
    if (profile.provider === "official") {
      return withMockNativeContent({
        tool: "codex",
        path: nativeConfigPath ?? "~/.codex/config.toml",
        status: "preview",
        writeEnabled: true,
        changes: [
          {
            key: "model_provider",
            action: "update",
            before: "custom",
            after: "openai",
            detail: "Selects Codex's official OpenAI provider."
          },
          {
            key: "cli_auth_credentials_store",
            action: "update",
            before: null,
            after: "file",
            detail: "Uses file-backed Codex authentication so managed credentials are read from auth.json."
          },
          {
            key: "model",
            action: profile.model ? "update" : "remove",
            before: "gpt-5-codex",
            after: profile.model || null,
            detail: profile.model ? "Sets Codex to the selected official model." : "Official provider can use Codex's own model default."
          },
          mockCodexReviewModelChange(profile, profile.model),
          {
            key: "model_providers.openai.base_url",
            action: "remove",
            before: "https://example.invalid/v1",
            after: null,
            detail: "Removes any custom OpenAI base URL override for the official provider."
          },
          {
            key: "model_providers.openai.requires_openai_auth",
            action: "add",
            before: null,
            after: "false",
            detail: "Disables Codex's built-in OpenAI auth requirement for this managed provider."
          },
          {
            key: "model_providers.openai.http_headers",
            action: "add",
            before: null,
            after: '{ "x-openai-actor-authorization" = "codestudio-lite" }',
            detail: "Adds the CodeStudio Lite actor-authorization header to this managed provider."
          }
        ],
        warnings: [
          "Official provider uses the target client's own login.",
          "No Provider API key or model override is required."
        ]
      });
    }

    const providerId = "custom";
    const directChanges: NativeConfigDiffLine[] = [
      {
        key: "model_provider",
        action: "update",
        before: "custom",
        after: providerId,
        detail: "Selects the direct provider entry managed by CodeStudio Lite."
      },
      {
        key: "cli_auth_credentials_store",
        action: "update",
        before: null,
        after: "file",
        detail: "Uses file-backed Codex authentication so managed credentials are read from auth.json."
      },
      {
        key: `model_providers.${providerId}.wire_api`,
        action: "add",
        before: null,
        after: wireApi,
        detail: "Uses Codex's selected provider wire API."
      },
      {
        key: `model_providers.${providerId}.base_url`,
        action: "add",
        before: null,
        after: profile.baseUrl,
        detail: "Points Codex directly at the upstream Provider Base URL."
      },
      {
        key: `model_providers.${providerId}.requires_openai_auth`,
        action: "add",
        before: null,
        after: "false",
        detail: "Disables Codex's built-in OpenAI auth requirement for this managed provider."
      },
      {
        key: `model_providers.${providerId}.http_headers`,
        action: "add",
        before: null,
        after: '{ "x-openai-actor-authorization" = "codestudio-lite" }',
        detail: "Adds the CodeStudio Lite actor-authorization header to this managed provider."
      }
    ];
    if (profile.model) {
      directChanges.push({
        key: "model",
        action: "update",
        before: "gpt-5-codex",
        after: profile.model,
        detail: "Sets Codex to the selected upstream model."
      });
    } else {
      directChanges.push({
        key: "model",
        action: "remove",
        before: "gpt-5-codex",
        after: null,
        detail: "Removes the model override when the profile has no selected model."
      });
    }
    directChanges.push(mockCodexReviewModelChange(profile, profile.model));
    return withMockNativeContent({
      tool: "codex",
      path: nativeConfigPath ?? "~/.codex/config.toml",
      status: "preview",
      writeEnabled: true,
      changes: directChanges,
      warnings: [
        "Config profiles write Codex's provider entry directly to the selected upstream Provider.",
        "The preview masks the Provider API key. Apply writes the actual key from the system keychain to Codex auth.json.",
        "Changing Codex config usually requires restarting Codex or opening a new Codex session."
      ]
    });
  }

  const gatewayBaseUrl = mockGatewayBaseUrlForTool(profile.app);
  const gatewayModel = profile.model || "default";
  return withMockNativeContent({
    tool: "codex",
    path: nativeConfigPath ?? "~/.codex/config.toml",
    status: "preview",
    writeEnabled: true,
    changes: [
      {
        key: "model_provider",
        action: "update",
        before: "custom",
        after: "custom",
        detail: "Selects the CodeStudio Lite localhost provider."
      },
      {
        key: "cli_auth_credentials_store",
        action: "update",
        before: null,
        after: "file",
        detail: "Uses file-backed Codex authentication so managed credentials are read from auth.json."
      },
      {
        key: "model",
        action: "update",
        before: "gpt-5-codex",
        after: gatewayModel,
        detail: "Sets Codex to the virtual model name resolved by the Local Gateway."
      },
      mockCodexReviewModelChange(profile, gatewayModel),
      {
        key: "model_providers.custom.base_url",
        action: "add",
        before: null,
        after: gatewayBaseUrl,
        detail: "Points Codex at the tool-scoped CodeStudio Lite Local Gateway."
      },
      {
        key: "model_providers.custom.requires_openai_auth",
        action: "add",
        before: null,
        after: "false",
        detail: "Disables Codex's built-in OpenAI auth requirement for this managed provider."
      },
      {
        key: "model_providers.custom.http_headers",
        action: "add",
        before: null,
        after: '{ "x-openai-actor-authorization" = "codestudio-lite" }',
        detail: "Adds the CodeStudio Lite actor-authorization header to this managed provider."
      }
    ],
    warnings: [
      "Gateway profiles are a one-time relay injection target, not a direct Provider switch.",
      "Switching profiles later changes only the Gateway active profile for this tool.",
      "The preview masks the local CodeStudio token. Apply writes only this local token to Codex auth.json; upstream Provider keys stay in the system keychain.",
      "Codex official login is still required for the desktop app; the Local Gateway only takes over model requests.",
      "If Codex is already running, restart Codex or open a new Codex session after bootstrap so it reloads config.toml."
    ]
  });
}

function effectiveMockCodexReviewModel(profile: ProfileDraft, primaryModel: string): string | null {
  if (!isCodexFamilyApp(profile.app)) {
    return null;
  }
  const override = normalizeMockProfileReviewModel(profile.app, profile.reviewModel);
  if (override) {
    return override;
  }
  const primary = primaryModel.trim();
  return primary.length > 0 ? primary : null;
}

function mockCodexReviewModelChange(profile: ProfileDraft, primaryModel: string): NativeConfigDiffLine {
  const reviewModel = effectiveMockCodexReviewModel(profile, primaryModel);
  const hasOverride = normalizeMockProfileReviewModel(profile.app, profile.reviewModel) !== null;
  return {
    key: "review_model",
    action: reviewModel ? "update" : "remove",
    before: "gpt-5-codex",
    after: reviewModel,
    detail: reviewModel
      ? hasOverride
        ? "Sets the Codex model used for code review."
        : "Keeps the Codex review model aligned with the primary model."
      : "Removes the Codex review model because the profile has no primary model to follow."
  };
}

function withMockNativeContent(
  preview: PreviewProfileApplyResult["nativeDiff"]
): PreviewProfileApplyResult["nativeDiff"] {
  if (!preview || preview.content) {
    return preview;
  }
  const lines = [
    `# ${preview.tool}`,
    `# ${preview.path}`,
    ...preview.changes
      .map((change) =>
        change.after === null
          ? `# remove ${change.key}`
          : `${change.key} = ${JSON.stringify(change.after)}`
      )
  ];
  return {
    ...preview,
    content: lines.join("\n")
  };
}

function mockCodexWireApi(protocol: string): string | null {
  const normalized = normalizeMockProtocol(protocol);
  if (normalized === PROTOCOL_OPENAI_RESPONSES) {
    return "responses";
  }
  if (normalized === PROTOCOL_OPENAI_CHAT_COMPLETIONS) {
    return "chat";
  }
  return null;
}

function mockRuntimeBaseUrl(protocol: string, baseUrl: string): string {
  const trimmed = baseUrl.trim().replace(/\/+$/, "");
  if (!trimmed) {
    return "";
  }
  const addV1 =
    protocol === PROTOCOL_OPENAI_CHAT_COMPLETIONS || protocol === PROTOCOL_OPENAI_RESPONSES;
  const withRuntimePath = (path: string) => {
    const clean = path.replace(/\/+$/, "");
    const lastSegment = clean.split("/").filter(Boolean).at(-1) ?? "";
    const alreadyVersioned = /^v\d(?:[a-z0-9._-]*)$/i.test(lastSegment);
    if (!addV1 || alreadyVersioned) {
      return clean;
    }
    return clean ? `${clean}/v1` : "/v1";
  };

  try {
    const parsed = new URL(trimmed);
    parsed.pathname = withRuntimePath(parsed.pathname);
    parsed.search = "";
    parsed.hash = "";
    return parsed.toString();
  } catch {
    return withRuntimePath(trimmed);
  }
}

function mockNonCodexNativeConfigPreview(
  profile: ProfileDraft,
  nativeConfigPath: string | null,
  mode: ProviderApplyMode
): PreviewProfileApplyResult["nativeDiff"] {
  if (canonicalProfileApp(profile.app) === "claude-desktop") {
    return mockClaudeDesktopNativeConfigPreview(profile, nativeConfigPath, mode);
  }

  if (mode === "gateway") {
    return mockNonCodexGatewayNativeConfigPreview(profile, nativeConfigPath);
  }

  if (mode !== "config") {
    return null;
  }

  const app = canonicalProfileApp(profile.app);
  if (!mockConfigProtocolSupported(profile)) {
    return null;
  }
    const providerId = "custom";
  const secret = profile.authRef ? "keychain:****" : "(missing keychain secret)";
  const model = profile.model.trim();
  const path =
    mockNativeConfigPath(app, mode, profile.provider) ??
    nativeConfigPath ??
    mockToolConfigPath(app) ??
    "~/.codestudio-lite/native-config";

  if (providerIsOfficial(profile.provider)) {
    return mockNonCodexOfficialNativeConfigPreview(app, path);
  }

  if (app === "claude") {
    return {
      tool: app,
      path,
      status: "preview",
      writeEnabled: true,
      changes: [
        {
          key: "env.ANTHROPIC_BASE_URL",
          action: "update",
          before: "https://api.anthropic.com",
          after: profile.baseUrl,
          detail: "Points Claude Code at the selected upstream Provider Base URL."
        },
        {
          key: "env.ANTHROPIC_AUTH_TOKEN",
          action: "add",
          before: null,
          after: secret,
          detail: "Stores the selected Provider API key as Claude Code's bearer token."
        },
        {
          key: "model",
          action: model ? "update" : "remove",
          before: "claude-sonnet-4-5",
          after: model || null,
          detail: model ? "Sets Claude Code to the selected upstream model." : "Model is optional; no Claude model override will be written."
        }
      ],
      warnings: [
        "Config profiles write Claude Code user settings under the env section.",
        "The selected endpoint must be Anthropic/Claude-compatible; generic OpenAI-only endpoints need a translator.",
        "Restart Claude Code or open a new session after applying so settings reload."
      ]
    };
  }

  if (app === "gemini") {
    return {
      tool: app,
      path,
      status: "preview",
      writeEnabled: true,
      changes: [
        {
          key: "GEMINI_API_KEY",
          action: "add",
          before: null,
          after: secret,
          detail: "Stores the selected Provider API key for Gemini CLI."
        },
        {
          key: "GOOGLE_GEMINI_BASE_URL",
          action: "update",
          before: "https://generativelanguage.googleapis.com",
          after: profile.baseUrl,
          detail: "Points Gemini CLI at the selected upstream Provider Base URL."
        },
        {
          key: "GEMINI_MODEL",
          action: model ? "update" : "remove",
          before: "gemini-2.5-pro",
          after: model || null,
          detail: model ? "Sets Gemini CLI to the selected upstream model." : "Model is optional; no Gemini model override will be written."
        }
      ],
      warnings: [
        "Gemini CLI reads API key and base URL from environment variables, so this adapter writes ~/.gemini/.env.",
        "Restart Gemini CLI or open a new terminal session after applying so environment variables reload."
      ]
    };
  }

  if (app === "gemini-code-assist") {
    return {
      tool: app,
      path,
      status: "preview",
      writeEnabled: true,
      changes: [
        {
          key: "geminicodeassist.geminiApiKey",
          action: "add",
          before: null,
          after: secret,
          detail: "Stores the selected Provider API key for Gemini Code Assist."
        },
        {
          key: "Provider Base URL",
          action: "update",
          before: null,
          after: profile.baseUrl,
          detail: "Gemini Code Assist does not expose a VS Code setting for custom Base URL; this stays in the CodeStudio Lite profile."
        },
        {
          key: "Model",
          action: model ? "update" : "remove",
          before: null,
          after: model || null,
          detail: model ? "Gemini Code Assist does not expose a VS Code setting for model override; this stays in the CodeStudio Lite profile." : "Model is optional and Gemini Code Assist has no model override setting to write."
        }
      ],
      warnings: [
        "Gemini Code Assist stores its API key in VS Code user settings.",
        "The public Gemini Code Assist VS Code setting exposes the API key; Provider Base URL and model are kept in CodeStudio Lite but are not written to the extension config.",
        "Restart VS Code or reload the Gemini Code Assist extension after applying so settings reload."
      ]
    };
  }

  if (app === "opencode") {
    return {
      tool: app,
      path,
      status: "preview",
      writeEnabled: true,
      changes: [
        {
          key: "$schema",
          action: "add",
          before: null,
          after: "https://opencode.ai/config.json",
          detail: "Keeps OpenCode config aligned with the published schema."
        },
        {
          key: `provider.${providerId}.npm`,
          action: "add",
          before: null,
          after: "@ai-sdk/openai-compatible",
          detail: "Uses OpenCode's OpenAI-compatible provider package."
        },
        {
          key: `provider.${providerId}.options.baseURL`,
          action: "add",
          before: null,
          after: profile.baseUrl,
          detail: "Points OpenCode at the selected upstream Provider Base URL."
        },
        {
          key: `provider.${providerId}.options.apiKey`,
          action: "add",
          before: null,
          after: secret,
          detail: "Stores the selected Provider API key for OpenCode."
        },
        {
          key: "model",
          action: model ? "update" : "remove",
          before: "openai/gpt-5",
          after: model ? `${providerId}/${model}` : null,
          detail: model ? "Selects the provider/model pair in OpenCode." : "Model is optional; no OpenCode model override will be written."
        }
      ],
      warnings: [
        "OpenCode custom providers are written to opencode.json using the OpenAI-compatible provider package.",
        "Existing JSONC/JSON5 comments are not preserved when CodeStudio Lite writes the file."
      ]
    };
  }

  if (app === "openclaw") {
    return {
      tool: app,
      path,
      status: "preview",
      writeEnabled: true,
      changes: [
        {
          key: "models.mode",
          action: "add",
          before: null,
          after: "merge",
          detail: "Merges CodeStudio Lite provider definitions with existing OpenClaw providers."
        },
        {
          key: `models.providers.${providerId}.api`,
          action: "add",
          before: null,
          after: "openai-completions",
          detail: "Uses OpenClaw's OpenAI-compatible API adapter."
        },
        {
          key: `models.providers.${providerId}.baseUrl`,
          action: "add",
          before: null,
          after: profile.baseUrl,
          detail: "Points OpenClaw at the selected upstream Provider Base URL."
        },
        {
          key: `models.providers.${providerId}.apiKey`,
          action: "add",
          before: null,
          after: secret,
          detail: "Stores the selected Provider API key for OpenClaw."
        },
        {
          key: "agents.defaults.model.primary",
          action: model ? "update" : "unchanged",
          before: "openai/gpt-5",
          after: model ? `${providerId}/${model}` : null,
          detail: model ? "Selects the provider/model pair as OpenClaw's primary default." : "Model is optional; no OpenClaw model override will be written."
        }
      ],
      warnings: [
        "OpenClaw providers are written in models.mode=merge so existing provider definitions can stay available.",
        "Existing JSON5 comments are not preserved when CodeStudio Lite writes the file."
      ]
    };
  }

  if (app === "hermes") {
    return {
      tool: app,
      path,
      status: "preview",
      writeEnabled: true,
      changes: [
        {
          key: "model.provider",
          action: "add",
          before: null,
          after: "custom",
          detail: "Selects Hermes custom provider mode."
        },
        {
          key: "model.base_url",
          action: "add",
          before: null,
          after: profile.baseUrl,
          detail: "Points Hermes at the selected upstream Provider Base URL."
        },
        {
          key: "model.api_key",
          action: "add",
          before: null,
          after: secret,
          detail: "Stores the selected Provider API key for Hermes."
        },
        {
          key: "model.api_mode",
          action: "add",
          before: null,
          after: "chat_completions",
          detail: "Uses Hermes' OpenAI Chat Completions custom endpoint mode."
        },
        {
          key: "model.default",
          action: model ? "update" : "remove",
          before: "gpt-5",
          after: model || null,
          detail: model ? "Sets Hermes to the selected upstream model." : "Model is optional; no Hermes model override will be written."
        }
      ],
      warnings: [
        "Hermes custom providers are written to ~/.hermes/config.yaml under the model section.",
        "Existing YAML comments are not preserved when CodeStudio Lite writes the file.",
        "Hermes config profiles currently target OpenAI Chat Completions endpoints."
      ]
    };
  }

  if (app === "grok") {
    const protocol = profile.protocol;
    const apiBackend =
      protocol === PROTOCOL_OPENAI_RESPONSES
        ? "responses"
        : protocol === PROTOCOL_ANTHROPIC_MESSAGES
          ? "messages"
          : "chat_completions";
    return {
      tool: app,
      path,
      status: "preview",
      writeEnabled: true,
      changes: [
        {
          key: "models.default",
          action: "add",
          before: null,
          after: "codestudio",
          detail: "Selects the CodeStudio managed Grok model as the session default."
        },
        {
          key: "model.codestudio.model",
          action: "add",
          before: null,
          after: model || "default",
          detail: "Sets the model id Grok sends to the upstream API."
        },
        {
          key: "model.codestudio.base_url",
          action: "add",
          before: null,
          after: profile.baseUrl,
          detail: "Points Grok at the selected upstream Provider Base URL."
        },
        {
          key: "model.codestudio.api_key",
          action: "add",
          before: null,
          after: secret,
          detail: "Stores the selected Provider API key for Grok."
        },
        {
          key: "model.codestudio.api_backend",
          action: "add",
          before: null,
          after: apiBackend,
          detail: "Selects the Grok API backend protocol for this model."
        }
      ],
      warnings: [
        "Grok custom models are written to ~/.grok/config.toml under [models] and [model.codestudio].",
        "Existing TOML comments are not preserved when CodeStudio Lite writes the file.",
        "Restart Grok or open a new session after applying so the model catalog reloads."
      ]
    };
  }

  if (app === "pi") {
    const api =
      profile.protocol === PROTOCOL_OPENAI_RESPONSES
        ? "openai-responses"
        : profile.protocol === PROTOCOL_ANTHROPIC_MESSAGES
          ? "anthropic-messages"
          : profile.protocol === PROTOCOL_GOOGLE_GEMINI
            ? "google-generative-ai"
            : "openai-completions";
    const changes: NonNullable<PreviewProfileApplyResult["nativeDiff"]>["changes"] = [
      {
        key: "providers.codestudio.baseUrl",
        action: "add",
        before: null,
        after: mockRuntimeBaseUrl(profile.protocol, profile.baseUrl),
        detail: "Points Pi Agent at the selected upstream Provider Base URL."
      },
      {
        key: "providers.codestudio.api",
        action: "add",
        before: null,
        after: api,
        detail: "Selects the Pi Agent API adapter for this provider."
      },
      {
        key: "providers.codestudio.apiKey",
        action: "add",
        before: null,
        after: secret,
        detail: "Stores the selected Provider API key for Pi Agent."
      },
      {
        key: "providers.codestudio.models",
        action: "add",
        before: null,
        after: `[${model || "default"}]`,
        detail: "Registers the selected model under the managed Pi provider."
      }
    ];
    if (api === "openai-completions" || api === "openai-responses") {
      changes.push(
        {
          key: "providers.codestudio.compat.supportsDeveloperRole",
          action: "add",
          before: null,
          after: "false",
          detail: "Uses system-role prompts for broader OpenAI-compatible endpoint support."
        },
        {
          key: "providers.codestudio.compat.supportsReasoningEffort",
          action: "add",
          before: null,
          after: "false",
          detail: "Avoids unsupported reasoning_effort fields on compatible endpoints."
        }
      );
    }
    return {
      tool: app,
      path,
      status: "preview",
      writeEnabled: true,
      changes,
      warnings: [
        "Pi Agent custom providers are written to ~/.pi/agent/models.json.",
        "Existing JSON comments are not preserved when CodeStudio Lite writes the file.",
        "Open /model in Pi after applying to select the managed provider model."
      ]
    };
  }

  return null;
}

function mockNonCodexOfficialNativeConfigPreview(
  app: string,
  path: string
): PreviewProfileApplyResult["nativeDiff"] {
  const base = {
    tool: app,
    path,
    status: "preview",
    writeEnabled: true
  };

  if (app === "claude") {
    return {
      ...base,
      changes: [
        {
          key: "env.ANTHROPIC_BASE_URL",
          action: "remove",
          before: "https://api.example.test/v1",
          after: null,
          detail: "Restores Claude Code to the client's own official endpoint."
        },
        {
          key: "env.ANTHROPIC_AUTH_TOKEN",
          action: "remove",
          before: "<redacted>",
          after: null,
          detail: "Removes the CodeStudio Lite managed API token from Claude settings."
        },
        {
          key: "model",
          action: "remove",
          before: "claude-sonnet-4-5",
          after: null,
          detail: "Removes the CodeStudio Lite managed model override."
        },
        {
          key: "env.ANTHROPIC_MODEL",
          action: "remove",
          before: "claude-sonnet-4-5",
          after: null,
          detail: "Removes the CodeStudio Lite managed model environment override."
        }
      ],
      warnings: [
        "Official provider restores Claude Code to its own login.",
        "CodeStudio Lite removes managed API or Gateway fields from Claude settings."
      ]
    };
  }

  if (app === "gemini") {
    return {
      ...base,
      changes: [
        {
          key: "GEMINI_API_KEY",
          action: "remove",
          before: "<redacted>",
          after: null,
          detail: "Removes the CodeStudio Lite managed Gemini API key."
        },
        {
          key: "GOOGLE_GEMINI_BASE_URL",
          action: "remove",
          before: "https://api.example.test/v1",
          after: null,
          detail: "Restores Gemini CLI to the client's own official endpoint."
        },
        {
          key: "GEMINI_MODEL",
          action: "remove",
          before: "gemini-2.5-pro",
          after: null,
          detail: "Removes the CodeStudio Lite managed model override."
        }
      ],
      warnings: [
        "Official provider restores Gemini CLI to its own login.",
        "CodeStudio Lite removes managed API or Gateway values from ~/.gemini/.env."
      ]
    };
  }

  if (app === "gemini-code-assist") {
    return {
      ...base,
      changes: [
        {
          key: "geminicodeassist.geminiApiKey",
          action: "remove",
          before: "<redacted>",
          after: null,
          detail: "Removes the CodeStudio Lite managed Gemini Code Assist API key."
        }
      ],
      warnings: [
        "Official provider restores Gemini Code Assist to its own login.",
        "CodeStudio Lite removes the managed API key setting from VS Code user settings."
      ]
    };
  }

  if (app === "opencode") {
    return {
      ...base,
      changes: [
        {
          key: "provider.custom",
          action: "remove",
          before: "managed provider entries",
          after: null,
          detail: "Removes CodeStudio Lite managed OpenCode provider entries."
        },
        {
          key: "model",
          action: "remove",
          before: "custom/default",
          after: null,
          detail: "Removes the active model only when it points to a CodeStudio Lite managed provider."
        }
      ],
      warnings: ["Official provider removes CodeStudio Lite managed OpenCode provider entries."]
    };
  }

  if (app === "openclaw") {
    return {
      ...base,
      changes: [
        {
          key: "models.providers.custom",
          action: "remove",
          before: "managed provider entries",
          after: null,
          detail: "Removes CodeStudio Lite managed OpenClaw provider entries."
        },
        {
          key: "agents.defaults.model.primary",
          action: "remove",
          before: "custom/default",
          after: null,
          detail: "Removes the primary model only when it points to a CodeStudio Lite managed provider."
        }
      ],
      warnings: ["Official provider removes CodeStudio Lite managed OpenClaw provider entries."]
    };
  }

  if (app === "hermes") {
    return {
      ...base,
      changes: [
        {
          key: "model.provider",
          action: "remove",
          before: "custom",
          after: null,
          detail: "Restores Hermes away from the CodeStudio Lite managed custom provider mode."
        },
        {
          key: "model.base_url",
          action: "remove",
          before: "https://api.example.test/v1",
          after: null,
          detail: "Removes the CodeStudio Lite managed Base URL."
        },
        {
          key: "model.api_key",
          action: "remove",
          before: "<redacted>",
          after: null,
          detail: "Removes the CodeStudio Lite managed API key."
        },
        {
          key: "model.api_mode",
          action: "remove",
          before: "chat_completions",
          after: null,
          detail: "Removes the CodeStudio Lite managed API mode."
        },
        {
          key: "model.default",
          action: "remove",
          before: "gpt-5",
          after: null,
          detail: "Removes the CodeStudio Lite managed model override."
        }
      ],
      warnings: ["Official provider removes CodeStudio Lite managed Hermes custom endpoint fields."]
    };
  }

  if (app === "pi") {
    return {
      ...base,
      changes: [
        {
          key: "providers.codestudio",
          action: "remove",
          before: "managed provider entries",
          after: null,
          detail: "Removes CodeStudio Lite managed Pi Agent provider entries."
        }
      ],
      warnings: ["Official provider removes CodeStudio Lite managed Pi Agent provider entries."]
    };
  }

  return null;
}

function mockClaudeDesktopNativeConfigPreview(
  profile: ProfileDraft,
  nativeConfigPath: string | null,
  mode: ProviderApplyMode
): PreviewProfileApplyResult["nativeDiff"] {
  if (mode === "config" && !providerIsOfficial(profile.provider) && !mockConfigProtocolSupported(profile)) {
    return null;
  }

  const path = nativeConfigPath ?? mockClaudeDesktopProfilePath();
  const secret = profile.authRef ? "keychain:****" : "(missing keychain secret)";
  const commonWarnings = [
    "Also updates ~/AppData/Local/Claude/claude_desktop_config.json.",
    "Also updates ~/AppData/Local/Claude-3p/claude_desktop_config.json and configLibrary/_meta.json.",
    "Restart Claude Desktop after applying so it reloads the config library."
  ];

  if (mode === "config" && providerIsOfficial(profile.provider)) {
    return {
      tool: "claude-desktop",
      path,
      status: "preview",
      writeEnabled: true,
      changes: [
        {
          key: "deploymentMode",
          action: "update",
          before: "3p",
          after: "1p",
          detail: "Restores Claude Desktop to first-party official mode in both config files."
        },
        {
          key: "configLibrary/_meta.appliedId",
          action: "remove",
          before: CLAUDE_DESKTOP_PROFILE_ID,
          after: null,
          detail: "Removes the CodeStudio Lite profile from Claude Desktop's 3P config library."
        },
        {
          key: `${CLAUDE_DESKTOP_PROFILE_ID}.json`,
          action: "remove",
          before: "CodeStudio Lite 3P profile",
          after: null,
          detail: "Deletes the managed CodeStudio Lite Claude Desktop 3P profile file."
        }
      ],
      warnings: [
        "Claude Desktop official mode restores deploymentMode=1p and removes the CodeStudio Lite 3P profile entry.",
        "No Provider API key or model override is required.",
        ...commonWarnings
      ]
    };
  }

  if (mode === "config") {
    const modelSpecs = mockClaudeDesktopDirectModelSpecs(profile);
    return {
      tool: "claude-desktop",
      path,
      status: "preview",
      writeEnabled: true,
      changes: [
        {
          key: "developer_settings.allowDevTools",
          action: "update",
          before: "false",
          after: "true",
          detail: "Enables Claude Desktop developer mode before applying the managed 3P profile."
        },
        {
          key: "deploymentMode",
          action: "update",
          before: "1p",
          after: "3p",
          detail: "Switches Claude Desktop to third-party provider mode in both config files."
        },
        {
          key: "inferenceProvider",
          action: "update",
          before: "official",
          after: "gateway",
          detail: "Uses Claude Desktop's built-in 3P inference gateway provider."
        },
        {
          key: "inferenceGatewayBaseUrl",
          action: "update",
          before: "https://api.anthropic.com",
          after: profile.baseUrl,
          detail: "Points Claude Desktop directly at the selected Anthropic-compatible Provider Base URL."
        },
        {
          key: "inferenceGatewayApiKey",
          action: "add",
          before: null,
          after: secret,
          detail: "Stores the selected Provider API key in Claude Desktop's 3P profile."
        },
        {
          key: "inferenceModels",
          action: modelSpecs.length ? "update" : "remove",
          before: "[]",
          after: modelSpecs.length ? JSON.stringify(modelSpecs) : null,
          detail: modelSpecs.length
            ? "Exposes the selected Claude-safe model in Claude Desktop's model menu."
            : "Model is optional; no Claude Desktop model menu override will be written."
        }
      ],
      warnings: [
        "Claude Desktop config profile writes the 3P profile system used by Claude Desktop.",
        "CodeStudio Lite enables Claude Desktop developer mode before writing the 3P profile if it is not already enabled.",
        "The selected endpoint must be Anthropic Messages compatible; generic OpenAI-only endpoints need Gateway profiles.",
        ...commonWarnings
      ]
    };
  }

  const modelSpecs = mockClaudeDesktopGatewayModelSpecs(profile);
  return {
    tool: "claude-desktop",
    path,
    status: "preview",
    writeEnabled: true,
    changes: [
      {
        key: "developer_settings.allowDevTools",
        action: "update",
        before: "false",
        after: "true",
        detail: "Enables Claude Desktop developer mode before applying the managed Gateway profile."
      },
      {
        key: "deploymentMode",
        action: "update",
        before: "1p",
        after: "3p",
        detail: "Switches Claude Desktop to third-party provider mode in both config files."
      },
      {
        key: "inferenceProvider",
        action: "update",
        before: "official",
        after: "gateway",
        detail: "Uses Claude Desktop's built-in 3P inference gateway provider."
      },
      {
        key: "inferenceGatewayBaseUrl",
        action: "update",
        before: "https://api.anthropic.com",
        after: mockClaudeDesktopGatewayBaseUrl(),
        detail: "Points Claude Desktop at the tool-scoped CodeStudio Lite Local Gateway."
      },
      {
        key: "inferenceGatewayApiKey",
        action: "add",
        before: null,
        after: "codestudio-local-****7f3a2c",
        detail: "Stores only the local CodeStudio token, not the real upstream Provider API key."
      },
      {
        key: "inferenceModels",
        action: "update",
        before: "[]",
        after: JSON.stringify(modelSpecs),
        detail: "Exposes Claude Desktop-safe route IDs while the Gateway resolves the real upstream model."
      }
    ],
    warnings: [
      "Claude Desktop gateway profile writes the 3P profile to the tool-scoped CodeStudio Lite Local Gateway URL.",
      "CodeStudio Lite enables Claude Desktop developer mode before writing the Gateway profile if it is not already enabled.",
      "Applying a Gateway profile does not start the Gateway automatically; use the sidebar Gateway controls when you want it running.",
      ...commonWarnings
    ]
  };
}

function mockClaudeDesktopDirectModelSpecs(profile: ProfileDraft): unknown[] {
  const model = profile.model.trim();
  if (!model || !mockClaudeDesktopSafeModelId(model)) {
    return [];
  }
  return [model];
}

function mockClaudeDesktopGatewayModelSpecs(profile: ProfileDraft): unknown[] {
  const model = profile.model.trim();
  if (!model) {
    return CLAUDE_DESKTOP_DEFAULT_ROUTES.map((name) => ({ name, supports1m: true }));
  }
  if (mockClaudeDesktopSafeModelId(model)) {
    return [{ name: model, supports1m: true }];
  }
  return [{ name: CLAUDE_DESKTOP_DEFAULT_ROUTE_ID, labelOverride: model, supports1m: true }];
}

function mockClaudeDesktopSafeModelId(model: string): boolean {
  const normalized = model.trim().toLowerCase();
  if (normalized.includes("[1m]")) {
    return false;
  }
  const routeTail = normalized.startsWith("anthropic/claude-")
    ? normalized.slice("anthropic/claude-".length)
    : normalized.startsWith("claude-")
      ? normalized.slice("claude-".length)
      : "";
  return ["sonnet-", "opus-", "haiku-", "fable-"].some((prefix) => routeTail.startsWith(prefix) && routeTail.length > prefix.length);
}

function mockNonCodexGatewayNativeConfigPreview(
  profile: ProfileDraft,
  nativeConfigPath: string | null
): PreviewProfileApplyResult["nativeDiff"] {
  const app = canonicalProfileApp(profile.app);
  const path =
    mockNativeConfigPath(app, "gateway", profile.provider) ??
    nativeConfigPath ??
    mockToolConfigPath(app) ??
    "~/.codestudio-lite/native-config";
  const gatewayBaseUrl = mockGatewayBaseUrlForTool(app);
  const providerId = "custom";
  const providerName = "CodeStudio Lite Local Gateway";
  const localToken = "codestudio-local-****7f3a2c";
  const localModel = profile.model || "default";
  const modelRef = `${providerId}/${localModel}`;
  const commonWarnings = [
    "Applying a Gateway profile does not start the Gateway automatically; use the sidebar Gateway controls when you want it running.",
    "Real upstream Provider API keys stay in the system keychain and are used by the local gateway."
  ];

  if (app === "claude") {
    return {
      tool: app,
      path,
      status: "preview",
      writeEnabled: true,
      changes: [
        {
          key: "env.ANTHROPIC_BASE_URL",
          action: "update",
          before: "https://api.anthropic.com",
          after: gatewayBaseUrl,
          detail: "Points Claude Code at the tool-scoped CodeStudio Lite Local Gateway."
        },
        {
          key: "env.ANTHROPIC_AUTH_TOKEN",
          action: "add",
          before: null,
          after: localToken,
          detail: "Stores only the local CodeStudio token, not the real upstream Provider API key."
        },
        {
          key: "model",
          action: "update",
          before: "claude-sonnet-4-5",
          after: localModel,
          detail: "Sets Claude Code to the virtual model name resolved by the Local Gateway."
        },
        {
          key: "env.ANTHROPIC_MODEL",
          action: "update",
          before: "claude-sonnet-4-5",
          after: localModel,
          detail: "Keeps the local gateway virtual model available to Claude Code environment consumers."
        }
      ],
      warnings: [
        "Gateway profiles write Claude Code settings to the tool-scoped local gateway URL.",
        "Restart Claude Code or open a new session after applying so settings reload.",
        ...commonWarnings
      ]
    };
  }

  if (app === "gemini") {
    return {
      tool: app,
      path,
      status: "preview",
      writeEnabled: true,
      changes: [
        {
          key: "GEMINI_API_KEY",
          action: "add",
          before: null,
          after: localToken,
          detail: "Stores only the local CodeStudio token, not the real upstream Provider API key."
        },
        {
          key: "GOOGLE_GEMINI_BASE_URL",
          action: "update",
          before: "https://generativelanguage.googleapis.com",
          after: gatewayBaseUrl,
          detail: "Points Gemini CLI at the tool-scoped CodeStudio Lite Local Gateway."
        },
        {
          key: "GEMINI_MODEL",
          action: "update",
          before: "gemini-2.5-pro",
          after: localModel,
          detail: "Sets Gemini CLI to the virtual model name resolved by the Local Gateway."
        }
      ],
      warnings: [
        "Gateway profiles write Gemini CLI environment values to the tool-scoped local gateway URL.",
        "Restart Gemini CLI or open a new terminal session after applying so environment variables reload.",
        ...commonWarnings
      ]
    };
  }

  if (app === "opencode") {
    return {
      tool: app,
      path,
      status: "preview",
      writeEnabled: true,
      changes: [
        {
          key: `provider.${providerId}.npm`,
          action: "add",
          before: null,
          after: "@ai-sdk/openai-compatible",
          detail: "Uses OpenCode's OpenAI-compatible provider package."
        },
        {
          key: `provider.${providerId}.name`,
          action: "add",
          before: null,
          after: providerName,
          detail: "Adds a readable provider label for the Local Gateway."
        },
        {
          key: `provider.${providerId}.options.baseURL`,
          action: "add",
          before: null,
          after: gatewayBaseUrl,
          detail: "Points OpenCode at the tool-scoped CodeStudio Lite Local Gateway."
        },
        {
          key: `provider.${providerId}.options.apiKey`,
          action: "add",
          before: null,
          after: localToken,
          detail: "Stores only the local CodeStudio token, not the real upstream Provider API key."
        },
        {
          key: "model",
          action: "update",
          before: "openai/gpt-5",
          after: modelRef,
          detail: "Selects the local gateway provider/model pair in OpenCode."
        },
        {
          key: `provider.${providerId}.models.${localModel}.name`,
          action: "add",
          before: null,
          after: localModel,
          detail: "Registers the local gateway virtual model under the managed provider."
        }
      ],
      warnings: [
        "Gateway profiles write OpenCode's provider entry to the tool-scoped local gateway URL.",
        "Existing JSONC/JSON5 comments are not preserved when CodeStudio Lite writes the file.",
        ...commonWarnings
      ]
    };
  }

  if (app === "openclaw") {
    return {
      tool: app,
      path,
      status: "preview",
      writeEnabled: true,
      changes: [
        {
          key: "models.mode",
          action: "add",
          before: null,
          after: "merge",
          detail: "Merges CodeStudio Lite provider definitions with existing OpenClaw providers."
        },
        {
          key: `models.providers.${providerId}.api`,
          action: "add",
          before: null,
          after: "openai-completions",
          detail: "Uses OpenClaw's OpenAI-compatible API adapter."
        },
        {
          key: `models.providers.${providerId}.baseUrl`,
          action: "add",
          before: null,
          after: gatewayBaseUrl,
          detail: "Points OpenClaw at the tool-scoped CodeStudio Lite Local Gateway."
        },
        {
          key: `models.providers.${providerId}.apiKey`,
          action: "add",
          before: null,
          after: localToken,
          detail: "Stores only the local CodeStudio token, not the real upstream Provider API key."
        },
        {
          key: "agents.defaults.model.primary",
          action: "update",
          before: "openai/gpt-5",
          after: modelRef,
          detail: "Selects the local gateway provider/model pair as OpenClaw's primary default."
        }
      ],
      warnings: [
        "Gateway profiles write OpenClaw's provider entry to the tool-scoped local gateway URL.",
        "Existing JSON5 comments are not preserved when CodeStudio Lite writes the file.",
        ...commonWarnings
      ]
    };
  }

  if (app === "hermes") {
    return {
      tool: app,
      path,
      status: "preview",
      writeEnabled: true,
      changes: [
        {
          key: "model.provider",
          action: "add",
          before: null,
          after: "custom",
          detail: "Selects Hermes custom provider mode."
        },
        {
          key: "model.base_url",
          action: "add",
          before: null,
          after: gatewayBaseUrl,
          detail: "Points Hermes at the tool-scoped CodeStudio Lite Local Gateway."
        },
        {
          key: "model.api_key",
          action: "add",
          before: null,
          after: localToken,
          detail: "Stores only the local CodeStudio token, not the real upstream Provider API key."
        },
        {
          key: "model.api_mode",
          action: "add",
          before: null,
          after: "chat_completions",
          detail: "Uses Hermes' OpenAI Chat Completions custom endpoint mode."
        },
        {
          key: "model.default",
          action: "update",
          before: "gpt-5",
          after: localModel,
          detail: "Sets Hermes to the virtual model name resolved by the Local Gateway."
        }
      ],
      warnings: [
        "Gateway profiles write Hermes custom provider settings to the tool-scoped local gateway URL.",
        "Existing YAML comments are not preserved when CodeStudio Lite writes the file.",
        ...commonWarnings
      ]
    };
  }

  if (app === "pi") {
    return {
      tool: app,
      path,
      status: "preview",
      writeEnabled: true,
      changes: [
        {
          key: "providers.codestudio.baseUrl",
          action: "add",
          before: null,
          after: gatewayBaseUrl,
          detail: "Points Pi Agent at the tool-scoped CodeStudio Lite Local Gateway."
        },
        {
          key: "providers.codestudio.api",
          action: "add",
          before: null,
          after: "openai-completions",
          detail: "Uses OpenAI Chat Completions against the Local Gateway."
        },
        {
          key: "providers.codestudio.apiKey",
          action: "add",
          before: null,
          after: localToken,
          detail: "Stores only the local CodeStudio token, not the real upstream Provider API key."
        },
        {
          key: "providers.codestudio.compat.supportsDeveloperRole",
          action: "add",
          before: null,
          after: "false",
          detail: "Uses system-role prompts for Local Gateway compatibility."
        },
        {
          key: "providers.codestudio.models",
          action: "add",
          before: null,
          after: `[${localModel}]`,
          detail: "Registers the virtual gateway model under the managed Pi provider."
        }
      ],
      warnings: [
        "Gateway profiles write Pi Agent provider settings to the tool-scoped local gateway URL.",
        "Existing JSON comments are not preserved when CodeStudio Lite writes the file.",
        "Open /model in Pi after applying to select the managed provider model.",
        ...commonWarnings
      ]
    };
  }

  return null;
}

function mockConfigProtocolSupportedFields(app: string, provider: string, value: string): boolean {
  if (providerIsOfficial(provider)) {
    return true;
  }
  let protocol: string;
  try {
    protocol = normalizeMockProtocol(value);
  } catch {
    return false;
  }
  return profileSupportsConfigProtocol(app, protocol);
}

function mockProfileProtocolSupportedForMode(
  app: string,
  mode: ProviderApplyMode,
  provider: string,
  protocol: string
): boolean {
  if (providerIsOfficial(provider) || mode === "gateway") {
    return true;
  }
  return mockConfigProtocolSupportedFields(app, provider, protocol);
}

function mockConfigProtocolSupported(profile: ProfileDraft): boolean {
  return mockConfigProtocolSupportedFields(profile.app, profile.provider, profile.protocol);
}

function mockGatewayBaseUrlForTool(toolId: string): string {
  return `http://127.0.0.1:43112/tools/${canonicalProfileApp(toolId)}/v1`;
}

function requireMockField(label: string, value: unknown): string {
  if (typeof value !== "string" || !value.trim()) {
    throw new Error(`${label} is required`);
  }
  return value.trim();
}

function requireMockToken(label: string, value: unknown): string {
  const trimmed = requireMockField(label, value);
  const pattern = label === "Provider" ? /^[A-Za-z0-9_.-]+$/ : /^[A-Za-z0-9_-]+$/;
  if (!pattern.test(trimmed)) {
    throw new Error(label === "Provider"
      ? `${label} can only contain letters, numbers, '-', '_' and '.'`
      : `${label} can only contain letters, numbers, '-' and '_'`);
  }
  return trimmed;
}

export function mockModePreviews(
  profile: ProfileDraft,
  configNativeDiff: PreviewProfileApplyResult["nativeDiff"],
  gatewayNativeDiff: PreviewProfileApplyResult["nativeDiff"]
): PreviewProfileApplyResult["modePreviews"] {
  const isCodexTool = isCodexFamilyApp(profile.app);
  const isOfficial = providerIsOfficial(profile.provider);
  const officialClientConfig = isOfficial && !isCodexTool;
  const configProtocolSupported = mockConfigProtocolSupported(profile);
  const configSupported = Boolean(configNativeDiff) || officialClientConfig;
  const configBlockedReason = !configProtocolSupported && !isOfficial
    ? `Config profiles do not support ${mockProtocolLabel(profile.protocol)} for '${profile.app}'.`
    : !configSupported && !isOfficial
    ? `Config profile adapter is not implemented for '${profile.app}'.`
    : !profile.authRef && providerRequiresApiKey(profile.provider)
      ? "Config profiles need a stored Provider API key for this Provider."
      : null;
  const gatewayWritesNativeConfig = Boolean(gatewayNativeDiff);
  const gatewaySupported = !isOfficial;

  return [
    {
      mode: "config",
      label: "Client config profile",
      description: "Back up and modify the target client's native provider config directly. This makes the client talk to the selected upstream Provider without CodeStudio Lite in the request path.",
      supported: configSupported && !configBlockedReason,
      recommended: isOfficial && configSupported && !configBlockedReason,
      writesNativeConfig: Boolean(configNativeDiff),
      startsGateway: false,
      blockedReason: configBlockedReason,
      nativeDiff: configNativeDiff,
      warnings: officialClientConfig
        ? [
            "Official provider uses the target client's own login.",
            "No Provider API key or model override is required."
          ]
        : configNativeDiff
        ? [
            "Config profiles write Provider connection details into the client config.",
            "Frequent Provider switching may require the client to reload its own config."
          ]
        : []
    },
    {
      mode: "gateway",
      label: "Gateway profile",
      description: gatewayWritesNativeConfig
        ? "Back up and point the client at the local CodeStudio Gateway once. This apply only switches the active Provider profile; start the Gateway from the sidebar when needed."
        : "Switch the active Provider profile for the local Gateway. This apply does not start the Gateway or modify this tool's native config.",
      supported: gatewaySupported,
      recommended: gatewaySupported,
      writesNativeConfig: gatewayWritesNativeConfig,
      startsGateway: false,
      blockedReason: isOfficial
        ? "Official provider uses the client login directly and does not run through the local gateway."
        : null,
      nativeDiff: gatewayNativeDiff,
      warnings: gatewayWritesNativeConfig
          ? [
            "The client still needs to reload config after the first gateway bootstrap.",
            "Applying a Gateway profile does not start the Gateway automatically; use the sidebar Gateway controls when you want it running.",
            "Real upstream Provider API keys stay in the system keychain and are used by the local gateway."
          ]
        : [
            `No native gateway bootstrap is written for '${profile.app}'; configure the client to use the Gateway URL manually or wait for a validated adapter.`,
            "Applying a Gateway profile does not start the Gateway automatically; use the sidebar Gateway controls when you want it running.",
            "Real upstream Provider API keys stay in the system keychain and are used by the local gateway."
          ]
    }
  ];
}


