export const PROFILE_PROTOCOL_OPTIONS = [
  { id: "openai-chat-completions", labelKey: "wizard.protocol.openaiChatCompletions" },
  { id: "openai-responses", labelKey: "wizard.protocol.openaiResponses" },
  { id: "anthropic-messages", labelKey: "wizard.protocol.anthropicMessages" },
  { id: "google-gemini", labelKey: "wizard.protocol.googleGemini" }
] as const;

export type ProfileProtocolId = (typeof PROFILE_PROTOCOL_OPTIONS)[number]["id"];

export type ProfileToolDefinition = {
  id: string;
  label: string;
  officialProfileNameKey: string;
  defaultProfileNameKey: string;
  officialProtocol: ProfileProtocolId;
  defaultProtocol: ProfileProtocolId;
  configProtocols: readonly ProfileProtocolId[];
  supportsReviewModel: boolean;
  supportsModelMappings: boolean;
};

export const PROFILE_TOOL_CATALOG: readonly ProfileToolDefinition[] = [
  {
    id: "codex",
    label: "Codex",
    officialProfileNameKey: "profiles.officialProfile.codex",
    defaultProfileNameKey: "wizard.defaultProfile.codex",
    officialProtocol: "openai-responses",
    defaultProtocol: "openai-responses",
    configProtocols: ["openai-chat-completions", "openai-responses"],
    supportsReviewModel: true,
    supportsModelMappings: false
  },
  {
    id: "claude-desktop",
    label: "Claude Desktop",
    officialProfileNameKey: "profiles.officialProfile.claudeDesktop",
    defaultProfileNameKey: "wizard.defaultProfile.claudeDesktop",
    officialProtocol: "anthropic-messages",
    defaultProtocol: "anthropic-messages",
    configProtocols: ["anthropic-messages"],
    supportsReviewModel: false,
    supportsModelMappings: false
  },
  {
    id: "claude",
    label: "Claude Code",
    officialProfileNameKey: "profiles.officialProfile.claude",
    defaultProfileNameKey: "wizard.defaultProfile.claude",
    officialProtocol: "anthropic-messages",
    defaultProtocol: "anthropic-messages",
    configProtocols: ["anthropic-messages"],
    supportsReviewModel: false,
    supportsModelMappings: true
  },
  {
    id: "gemini-code-assist",
    label: "Gemini Code Assist",
    officialProfileNameKey: "profiles.officialProfile.geminiCodeAssist",
    defaultProfileNameKey: "wizard.defaultProfile.geminiCodeAssist",
    officialProtocol: "google-gemini",
    defaultProtocol: "google-gemini",
    configProtocols: ["google-gemini"],
    supportsReviewModel: false,
    supportsModelMappings: false
  },
  {
    id: "opencode",
    label: "OpenCode",
    officialProfileNameKey: "profiles.officialProfile.opencode",
    defaultProfileNameKey: "wizard.defaultProfile.opencode",
    officialProtocol: "openai-chat-completions",
    defaultProtocol: "openai-chat-completions",
    configProtocols: ["openai-chat-completions", "openai-responses"],
    supportsReviewModel: false,
    supportsModelMappings: false
  },
  {
    id: "openclaw",
    label: "OpenClaw",
    officialProfileNameKey: "profiles.officialProfile.openclaw",
    defaultProfileNameKey: "wizard.defaultProfile.openclaw",
    officialProtocol: "openai-chat-completions",
    defaultProtocol: "openai-chat-completions",
    configProtocols: ["openai-chat-completions"],
    supportsReviewModel: false,
    supportsModelMappings: false
  },
  {
    id: "hermes",
    label: "Hermes",
    officialProfileNameKey: "profiles.officialProfile.hermes",
    defaultProfileNameKey: "wizard.defaultProfile.hermes",
    officialProtocol: "openai-chat-completions",
    defaultProtocol: "openai-chat-completions",
    configProtocols: ["openai-chat-completions"],
    supportsReviewModel: false,
    supportsModelMappings: false
  },
  {
    id: "grok",
    label: "Grok",
    officialProfileNameKey: "profiles.officialProfile.grok",
    defaultProfileNameKey: "wizard.defaultProfile.grok",
    officialProtocol: "openai-responses",
    defaultProtocol: "openai-responses",
    configProtocols: ["openai-chat-completions", "openai-responses", "anthropic-messages"],
    supportsReviewModel: false,
    supportsModelMappings: false
  },
  {
    id: "pi",
    label: "Pi Agent",
    officialProfileNameKey: "profiles.officialProfile.pi",
    defaultProfileNameKey: "wizard.defaultProfile.pi",
    officialProtocol: "anthropic-messages",
    defaultProtocol: "openai-responses",
    configProtocols: ["openai-chat-completions", "openai-responses", "anthropic-messages", "google-gemini"],
    supportsReviewModel: false,
    supportsModelMappings: false
  }
];

export const PROFILE_TOOL_ORDER = PROFILE_TOOL_CATALOG.map((tool) => tool.id);
export const PROFILE_TOOL_LABELS = Object.fromEntries(
  PROFILE_TOOL_CATALOG.map((tool) => [tool.id, tool.label])
) as Record<string, string>;
export const OFFICIAL_PROFILE_NAME_KEYS = Object.fromEntries(
  PROFILE_TOOL_CATALOG.map((tool) => [tool.id, tool.officialProfileNameKey])
) as Record<string, string>;

export function canonicalProfileToolId(toolId: string): string {
  const normalized = toolId.trim().toLowerCase();
  if (["codex", "codex-cli", "chatgpt-desktop", "codex-app", "codex-client", "codex-desktop", "codex-vscode", "codex-code-vscode", "codex-vs-code"].includes(normalized)) {
    return "codex";
  }
  if (["claude-desktop", "claude-app", "claude-client"].includes(normalized)) {
    return "claude-desktop";
  }
  if (["claude", "claude-code", "claude-vscode", "claude-code-vscode", "claude-vs-code"].includes(normalized)) {
    return "claude";
  }
  if (["antigravity", "antigravity-cli", "agy"].includes(normalized)) {
    return "antigravity";
  }
  if (["gemini-code-assist", "gemini-vscode", "gemini-code-vscode", "gemini-vs-code"].includes(normalized)) {
    return "gemini-code-assist";
  }
  if (["opencode", "open-code"].includes(normalized)) {
    return "opencode";
  }
  if (["openclaw", "open-claw"].includes(normalized)) {
    return "openclaw";
  }
  if (["hermes", "hermes-agent"].includes(normalized)) {
    return "hermes";
  }
  if (["grok", "grok-cli", "grok-build"].includes(normalized)) {
    return "grok";
  }
  if (["pi", "pi-agent", "pi-coding-agent"].includes(normalized)) {
    return "pi";
  }
  return normalized;
}

export function profileToolDefinition(toolId: string): ProfileToolDefinition | undefined {
  const canonical = canonicalProfileToolId(toolId);
  return PROFILE_TOOL_CATALOG.find((tool) => tool.id === canonical);
}

export function configProtocolIdsForTool(toolId: string): readonly ProfileProtocolId[] {
  return profileToolDefinition(toolId)?.configProtocols ?? [];
}

export function profileSupportsConfigProtocol(toolId: string, protocol: string): boolean {
  return configProtocolIdsForTool(toolId).includes(protocol as ProfileProtocolId);
}

export function profileSupportsReviewModel(toolId: string): boolean {
  return Boolean(profileToolDefinition(toolId)?.supportsReviewModel);
}

export function profileSupportsModelMappings(toolId: string): boolean {
  return Boolean(profileToolDefinition(toolId)?.supportsModelMappings);
}
