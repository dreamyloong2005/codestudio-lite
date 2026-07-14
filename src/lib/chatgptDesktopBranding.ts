import { writable } from "svelte/store";
import type {
  ChatGPTDesktopProductGeneration,
  DetectionSnapshot,
  InstalledChatGPTDesktop,
  ToolStatus
} from "../types";

export const chatgptDesktopGeneration = writable<ChatGPTDesktopProductGeneration>("current");

const legacyChatGPTDesktopToolIds = new Set(["codex-app", "codex-client", "codex-desktop"]);

export function isChatGPTDesktopToolId(toolId: string) {
  return toolId === "chatgpt-desktop" || legacyChatGPTDesktopToolIds.has(toolId);
}

export function normalizeChatGPTDesktopGeneration(
  generation: ChatGPTDesktopProductGeneration | null | undefined
): ChatGPTDesktopProductGeneration {
  return generation === "legacy" ? "legacy" : generation ?? "current";
}

export function setChatGPTDesktopGeneration(
  generation: ChatGPTDesktopProductGeneration | null | undefined
) {
  chatgptDesktopGeneration.set(normalizeChatGPTDesktopGeneration(generation));
}

export function applyChatGPTDesktopBrandingFromDetection(snapshot: DetectionSnapshot) {
  setChatGPTDesktopGeneration(snapshot.chatgptDesktopProductGeneration);
}

export function applyChatGPTDesktopBrandingFromInstalled(
  installed: InstalledChatGPTDesktop | null | undefined
) {
  if (installed) {
    setChatGPTDesktopGeneration(installed.generation);
  }
}

export function brandChatGPTDesktopText(
  text: string,
  generation: ChatGPTDesktopProductGeneration
) {
  if (generation !== "legacy") {
    return text;
  }
  return text
    .replaceAll("ChatGPT Desktop", "Codex Desktop")
    .replaceAll("ChatGPT desktop", "Codex desktop")
    .replaceAll("ChatGPT 桌面端", "Codex 桌面端")
    .replaceAll("ChatGPT 桌面版", "Codex 桌面版");
}

export function applyChatGPTDesktopToolBranding(tool: ToolStatus, productName: string): ToolStatus {
  if (!isChatGPTDesktopToolId(tool.id)) {
    return tool;
  }
  if (tool.id === "chatgpt-desktop" && tool.name === productName) {
    return tool;
  }
  return { ...tool, id: "chatgpt-desktop", name: productName };
}

function desktopToolPreference(tool: ToolStatus) {
  return (tool.installState === "installed" ? 8 : 0)
    + (tool.running ? 4 : 0)
    + (tool.id === "chatgpt-desktop" ? 2 : 0)
    + (tool.version ? 1 : 0);
}

export function normalizeChatGPTDesktopDetectionSnapshot(
  snapshot: DetectionSnapshot
): DetectionSnapshot {
  const candidates = snapshot.tools.filter((tool) => isChatGPTDesktopToolId(tool.id));
  if (candidates.length === 0) {
    return snapshot;
  }

  const selected = candidates.reduce((preferred, candidate) =>
    desktopToolPreference(candidate) > desktopToolPreference(preferred) ? candidate : preferred
  );
  const generation = legacyChatGPTDesktopToolIds.has(selected.id)
    ? "legacy"
    : normalizeChatGPTDesktopGeneration(snapshot.chatgptDesktopProductGeneration);
  const productName = generation === "legacy" ? "Codex Desktop" : "ChatGPT Desktop";
  const normalizedDesktop = applyChatGPTDesktopToolBranding(selected, productName);
  let desktopInserted = false;
  const tools = snapshot.tools.flatMap((tool) => {
    if (!isChatGPTDesktopToolId(tool.id)) {
      return [tool];
    }
    if (desktopInserted) {
      return [];
    }
    desktopInserted = true;
    return [normalizedDesktop];
  });

  return {
    ...snapshot,
    chatgptDesktopProductGeneration: generation,
    tools
  };
}
