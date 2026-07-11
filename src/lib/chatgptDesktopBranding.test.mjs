import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

function read(path) {
  return readFileSync(path, "utf8");
}

test("legacy branding executes without crossing ChatGPT auth or Codex CLI boundaries", async () => {
  const branding = await import("./chatgptDesktopBranding.ts");
  assert.equal(
    branding.brandChatGPTDesktopText(
      "ChatGPT Desktop / ChatGPT account / Codex CLI",
      "legacy"
    ),
    "Codex Desktop / ChatGPT account / Codex CLI"
  );
  assert.equal(
    branding.brandChatGPTDesktopText("ChatGPT 桌面端 / Codex CLI", "legacy"),
    "Codex 桌面端 / Codex CLI"
  );
  assert.equal(branding.normalizeChatGPTDesktopGeneration(undefined), "current");
  assert.equal(branding.normalizeChatGPTDesktopGeneration("legacy"), "legacy");
  assert.equal(
    branding.applyChatGPTDesktopToolBranding(
      { id: "codex", name: "Codex CLI" },
      "Codex Desktop"
    ).name,
    "Codex CLI"
  );
  assert.equal(
    branding.applyChatGPTDesktopToolBranding(
      { id: "chatgpt-desktop", name: "ChatGPT Desktop" },
      "Codex Desktop"
    ).name,
    "Codex Desktop"
  );
});

test("ChatGPT desktop generation is shared by detection and installed state", () => {
  const types = read("src/types.ts");
  const rustTypes = read("src-tauri/src/core/types.rs");
  const desktop = read("src-tauri/src/core/chatgpt_desktop.rs");
  const detector = read("src-tauri/src/core/detector.rs");

  assert.match(types, /export type ChatGPTDesktopProductGeneration = "current" \| "legacy";/);
  assert.match(types, /chatgptDesktopProductGeneration: ChatGPTDesktopProductGeneration;/);
  assert.match(types, /export interface InstalledChatGPTDesktop \{[\s\S]*?generation: ChatGPTDesktopProductGeneration;/);
  assert.match(rustTypes, /pub enum ChatGptDesktopProductGeneration \{[\s\S]*?Current,[\s\S]*?Legacy,/);
  assert.match(rustTypes, /pub chatgpt_desktop_product_generation: ChatGptDesktopProductGeneration/);
  assert.match(desktop, /pub generation: ChatGptDesktopProductGeneration/);
  assert.match(desktop, /fn chatgpt_desktop_generation_from_windows_root\(/);
  assert.match(desktop, /fn chatgpt_desktop_generation_from_macos_identity\(/);
  assert.match(detector, /chatgpt_desktop_product_generation:\s*chatgpt_desktop::detected_product_generation\(\)/);
});

test("desktop branding defaults to current and only rewrites the product phrase", () => {
  const branding = read("src/lib/chatgptDesktopBranding.ts");
  const i18n = read("src/lib/i18n.ts");
  const enUS = read("src/lib/locales/en-US.ts");
  const profiles = read("src/routes/Profiles.svelte");
  const wizard = read("src/routes/SetupWizard.svelte");

  assert.match(branding, /writable<ChatGPTDesktopProductGeneration>\("current"\)/);
  assert.match(branding, /generation \?\? "current"/);
  assert.match(branding, /"ChatGPT Desktop", "Codex Desktop"/);
  assert.match(branding, /"ChatGPT \u684c\u9762\u7aef", "Codex \u684c\u9762\u7aef"/);
  assert.doesNotMatch(branding, /replaceAll\("ChatGPT",/);
  assert.match(i18n, /derived\(\[locale, chatgptDesktopGeneration\]/);
  assert.match(i18n, /brandChatGPTDesktopText\([\s\S]*?\$generation/);
  assert.match(enUS, /"app\.nav\.chatgptDesktop": "ChatGPT Desktop"/);
  assert.match(profiles, /codex:\s*"Codex"/);
  assert.doesNotMatch(profiles, /codex:\s*"ChatGPT/);
  assert.match(wizard, /id:\s*"codex"[\s\S]*?label:\s*"Codex"/);
});

test("current desktop icon is the inverted CLI mark and legacy keeps the original artwork", () => {
  const component = read("src/components/ToolIcon.svelte");
  const panda = read("panda.config.ts");

  assert.match(component, /current:\s*\{ src: "\/tool-icons\/codex\.svg", tone: "chatgpt-desktop-current" \}/);
  assert.match(component, /legacy:\s*\{ src: "\/tool-icons\/chatgpt-desktop\.png", tone: "chatgpt-desktop-legacy" \}/);
  assert.match(component, /\$chatgptDesktopGeneration/);
  assert.doesNotMatch(component, /case "chatgpt-desktop":\s*case "chatgpt-desktop":/);
  assert.match(panda, /data-tool-icon-tone='chatgpt-desktop-current'[^\n]*data-tool-icon-tone='chatgpt-desktop-legacy'[\s\S]*?background: "#fff"/);
  assert.match(panda, /"chatgpt-desktop-current": \{[\s\S]*?background: "#fff"/);
  assert.match(panda, /data-tool-icon-tone='chatgpt-desktop-current'\] img[\s\S]*?filter: "invert\(1\)"/);
  assert.match(panda, /"chatgpt-desktop-legacy": \{[\s\S]*?background: "#fff"/);
  assert.match(component, /codex: \{ src: "\/tool-icons\/codex\.svg", tone: "codex" \}/);
  assert.match(panda, /codex:\s*\{[\s\S]*?background: "#111111"/);
});

test("app, detail cache, and dashboard consume one desktop branding state", () => {
  const app = read("src/App.svelte");
  const store = read("src/lib/chatgptDesktopStore.ts");
  const dashboard = read("src/routes/Dashboard.svelte");
  const route = read("src/routes/ChatGPTDesktop.svelte");

  assert.match(app, /applyChatGPTDesktopBrandingFromDetection\(nextSnapshot\)/);
  assert.match(store, /applyChatGPTDesktopBrandingFromDetection\(det\)/);
  assert.match(store, /applyChatGPTDesktopBrandingFromInstalled\(state\.installed\)/);
  assert.match(dashboard, /\$:\s*desktopProductName = \$t\("app\.nav\.chatgptDesktop"\)/);
  assert.match(dashboard, /\.map\(\(tool\) => applyChatGPTDesktopToolBranding\(tool, desktopProductName\)\)/);
  assert.match(dashboard, /brandDesktopText\(toolActionError\)/);
  assert.match(route, /brandDesktopText\(operationResult\.notes\.join\(" "\)\)/);
  assert.match(route, /brandDesktopText\(warning\)/);
  assert.match(route, /brandDesktopText\(capability\.detail\)/);
});
