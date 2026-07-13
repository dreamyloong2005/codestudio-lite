import type {
  ListProfileModelsResult,
  ProviderApplyMode,
  TestProfileConnectionResult
} from "../../../types";
import {
  canonicalProfileToolId,
  profileSupportsConfigProtocol
} from "../../profiles/catalog";

const OPENAI_CHAT = "openai-chat-completions";
const OPENAI_RESPONSES = "openai-responses";
const ANTHROPIC_MESSAGES = "anthropic-messages";
const GOOGLE_GEMINI = "google-gemini";

export function providerIsOfficial(provider: string): boolean {
  return provider.trim() === "official";
}

export function providerRequiresApiKey(provider: string): boolean {
  return !providerIsOfficial(provider);
}

export function normalizeBrowserProfileMode(
  provider: string,
  requested?: ProviderApplyMode | null
): ProviderApplyMode {
  const mode = requested ?? (providerIsOfficial(provider) ? "config" : "gateway");
  if (providerIsOfficial(provider) && mode === "gateway") {
    throw new Error("Official provider uses the client login directly and cannot use Gateway profile.");
  }
  return mode;
}

export function ensureCustomOfficialProfileAllowed(
  app: string,
  provider: string,
  mode: ProviderApplyMode
): void {
  if (providerIsOfficial(provider) && !(canonicalProfileToolId(app) === "codex" && mode === "config")) {
    throw new Error("Only Codex OAuth profiles can be saved as custom official profiles.");
  }
}

export function normalizeBrowserProtocol(value?: string | null): string {
  const protocol = (value ?? "").trim();
  if ([OPENAI_CHAT, OPENAI_RESPONSES, ANTHROPIC_MESSAGES, GOOGLE_GEMINI].includes(protocol)) {
    return protocol;
  }
  throw new Error("Unsupported Provider API protocol.");
}

export function browserProtocolLabel(value?: string | null): string {
  let protocol: string;
  try {
    protocol = normalizeBrowserProtocol(value);
  } catch {
    return value?.trim() || "Unknown protocol";
  }
  if (protocol === OPENAI_RESPONSES) return "OpenAI Responses API";
  if (protocol === ANTHROPIC_MESSAGES) return "Claude Messages API";
  if (protocol === GOOGLE_GEMINI) return "Gemini API";
  return "OpenAI Chat Completions";
}

export function ensureBrowserProfileProtocolSupported(
  app: string,
  mode: ProviderApplyMode,
  provider: string,
  protocol: string
): void {
  if (providerIsOfficial(provider) || mode === "gateway") return;
  let normalized: string;
  try {
    normalized = normalizeBrowserProtocol(protocol);
  } catch {
    normalized = protocol;
  }
  if (!profileSupportsConfigProtocol(app, normalized)) {
    throw new Error(`Config profiles do not support ${browserProtocolLabel(protocol)} for '${canonicalProfileToolId(app)}'.`);
  }
}

export function validateBrowserBaseUrl(
  baseUrl: string
): TestProfileConnectionResult["checks"][number] {
  const trimmed = baseUrl.trim();
  if (!/^https?:\/\//i.test(trimmed)) {
    return { id: "base-url", label: "Base URL", status: "error", detail: "Base URL must start with http:// or https://" };
  }
  try {
    const parsed = new URL(trimmed);
    if (!["http:", "https:"].includes(parsed.protocol)) {
      return { id: "base-url", label: "Base URL", status: "error", detail: "Base URL must start with http:// or https://" };
    }
    if (!parsed.hostname) {
      return { id: "base-url", label: "Base URL", status: "error", detail: "Base URL must include a host." };
    }
    return { id: "base-url", label: "Base URL", status: "ok", detail: trimmed };
  } catch {
    return { id: "base-url", label: "Base URL", status: "error", detail: "Base URL is not a valid URL." };
  }
}

export function validateBrowserBaseUrlForProvider(
  provider: string,
  baseUrl: string
): TestProfileConnectionResult["checks"][number] {
  if (providerIsOfficial(provider) && !baseUrl.trim()) {
    return {
      id: "base-url",
      label: "Base URL",
      status: "info",
      detail: "Official provider uses the target client's own login and default endpoint."
    };
  }
  return validateBrowserBaseUrl(baseUrl);
}

export function validateBrowserBaseUrlForProviderOrThrow(provider: string, baseUrl: string): void {
  const check = validateBrowserBaseUrlForProvider(provider, baseUrl);
  if (check.status === "error") throw new Error(check.detail);
}

export function browserProfileModels(protocol: string): ListProfileModelsResult["models"] {
  if (protocol === ANTHROPIC_MESSAGES) {
    return [
      { id: "claude-sonnet-4-6", name: "Claude Sonnet 4.6", ownedBy: "anthropic", supports1m: true },
      { id: "claude-opus-4-8", name: "Claude Opus 4.8", ownedBy: "anthropic", supports1m: true },
      { id: "claude-haiku-4-5", name: "Claude Haiku 4.5", ownedBy: "anthropic", supports1m: true }
    ];
  }
  if (protocol === GOOGLE_GEMINI) {
    return [
      { id: "gemini-2.5-pro", name: "Gemini 2.5 Pro", ownedBy: "google", supports1m: true },
      { id: "gemini-2.5-flash", name: "Gemini 2.5 Flash", ownedBy: "google", supports1m: true }
    ];
  }
  return [
    { id: "gpt-5", name: "GPT-5", ownedBy: "openai", supports1m: false },
    { id: "gpt-5-mini", name: "GPT-5 mini", ownedBy: "openai", supports1m: false },
    { id: "gpt-4.1", name: "GPT-4.1", ownedBy: "openai", supports1m: false }
  ];
}

export function browserCredentialStatus(
  provider: string,
  secretProvided: boolean
): TestProfileConnectionResult["status"] {
  return providerIsOfficial(provider) ? "info" : secretProvided ? "ok" : "error";
}

export function browserCredentialDetail(provider: string, secretProvided: boolean): string {
  if (providerIsOfficial(provider)) {
    return "Official login flow does not require an API key in this profile draft.";
  }
  return secretProvided
    ? "The Provider API key will be stored in the system keychain when this profile is saved; it is not written to TOML or logs."
    : "Provider API key is required for non-official providers.";
}
