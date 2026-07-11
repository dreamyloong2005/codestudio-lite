import { writable } from "svelte/store";
import type {
  ChatGPTDesktopProductGeneration,
  DetectionSnapshot,
  InstalledChatGPTDesktop,
  ToolStatus
} from "../types";

export const chatgptDesktopGeneration = writable<ChatGPTDesktopProductGeneration>("current");

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
  if (tool.id !== "chatgpt-desktop" || tool.name === productName) {
    return tool;
  }
  return { ...tool, name: productName };
}
