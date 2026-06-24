import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const localeFiles = [
  "src-tauri/resources/claude-desktop/i18n/zh-CN.json",
  "src-tauri/resources/claude-desktop/i18n/ion-dist/i18n/zh-CN.json",
];

const readJson = (path) => JSON.parse(readFileSync(new URL(`../../${path}`, import.meta.url), "utf8"));
const stripUrls = (value) => value.replace(/[a-z][a-z0-9+.-]*:\/\/\S+/gi, "");
const badArtifactTerms = /(?<!复)制品|产物|神器|伪影|\bArtifacts?\b|\bartifacts?\b/;

const readTranslationRuntime = () => {
  const patch = readFileSync(
    new URL("../../src-tauri/src/core/claude_desktop_patch.rs", import.meta.url),
    "utf8",
  );
  const match = patch.match(/const TRANSLATION_RUNTIME: &str = r##"([\s\S]*?)"##;/);
  assert.ok(match, "TRANSLATION_RUNTIME raw string should exist");
  return match[1];
};

class FakeTextNode {
  constructor(value) {
    this.nodeType = 3;
    this.nodeValue = value;
    this.parentElement = null;
  }
}

class FakeElement {
  constructor(tagName, attrs = {}, children = []) {
    this.nodeType = 1;
    this.tagName = tagName.toUpperCase();
    this.attrs = attrs;
    this.childNodes = [];
    this.parentElement = null;
    children.forEach((child) => this.append(child));
  }

  append(child) {
    child.parentElement = this;
    this.childNodes.push(child);
  }

  getAttribute(name) {
    return this.attrs[name] ?? null;
  }

  setAttribute(name, value) {
    this.attrs[name] = value;
  }

  closest(selector) {
    for (let el = this; el; el = el.parentElement) {
      if (selector.split(",").some((part) => matchesSimpleSelector(el, part.trim()))) {
        return el;
      }
    }
    return null;
  }
}

const text = (value) => new FakeTextNode(value);
const element = (tagName, attrs, children) => new FakeElement(tagName, attrs, children);

const attrValue = (el, name) => String(el.attrs[name] ?? "");
const matchesSimpleSelector = (el, selector) => {
  if (!selector) return false;
  if (/^[a-z]+$/i.test(selector)) return el.tagName.toLowerCase() === selector.toLowerCase();

  const roleMatch = selector.match(/^\[role="([^"]+)"\]$/);
  if (roleMatch) return attrValue(el, "role") === roleMatch[1];

  const containsMatch = selector.match(/^\[([^\]=*]+)\*="([^"]+)"\]$/);
  if (containsMatch) return attrValue(el, containsMatch[1]).includes(containsMatch[2]);

  const attrMatch = selector.match(/^\[([^\]]+)\]$/);
  if (attrMatch) return Object.hasOwn(el.attrs, attrMatch[1]);

  return false;
};

const collectTextNodes = (root, out = []) => {
  for (const child of root.childNodes ?? []) {
    if (child.nodeType === 3) out.push(child);
    if (child.nodeType === 1) collectTextNodes(child, out);
  }
  return out;
};

const runTranslationRuntimeOnBody = (body) => {
  const storage = new Map([["__cslWantedLocale", "zh-CN"]]);
  const context = {
    console,
    CustomEvent: class CustomEvent {},
    document: {
      body,
      documentElement: element("html", {}, []),
      readyState: "complete",
      title: "",
      addEventListener() {},
      createTreeWalker(root) {
        const nodes = collectTextNodes(root);
        let idx = -1;
        return {
          currentNode: null,
          nextNode() {
            idx += 1;
            this.currentNode = nodes[idx] ?? null;
            return !!this.currentNode;
          },
        };
      },
      querySelectorAll() {
        return [];
      },
    },
    globalThis: null,
    localStorage: {
      getItem(key) {
        return storage.get(key) ?? null;
      },
      setItem(key, value) {
        storage.set(key, String(value));
      },
    },
    location: { hostname: "claude.ai" },
    MutationObserver: class MutationObserver {
      observe() {}
    },
    navigator: { language: "zh-CN" },
    NodeFilter: { SHOW_TEXT: 4 },
    sessionStorage: {
      getItem() {
        return null;
      },
      setItem() {},
    },
    setInterval() {},
    setTimeout(fn) {
      fn();
    },
    StorageEvent: class StorageEvent {},
    window: {
      dispatchEvent() {},
    },
  };
  context.globalThis = context;
  vm.runInNewContext(readTranslationRuntime(), context);
};

test('Claude Desktop zh-CN locale translates "Artifact" as "工件"', () => {
  const failures = [];

  for (const file of localeFiles) {
    const locale = readJson(file);
    for (const [key, value] of Object.entries(locale)) {
      if (typeof value !== "string") continue;
      if (badArtifactTerms.test(stripUrls(value))) {
        failures.push(`${file}:${key}: ${value}`);
      }
    }
  }

  assert.deepEqual(failures, []);
});

test('Claude Desktop zh-CN locale uses "凭据" for provider credentials', () => {
  const ion = readJson("src-tauri/resources/claude-desktop/i18n/ion-dist/i18n/zh-CN.json");

  assert.equal(ion["w4MKEU2/Va"], "{provider} 凭据");
});

test("Claude Desktop zh-CN locale keeps agent and Chrome terminology curated", () => {
  const ion = readJson("src-tauri/resources/claude-desktop/i18n/ion-dist/i18n/zh-CN.json");
  const failures = [];

  for (const [key, value] of Object.entries(ion)) {
    if (typeof value !== "string") continue;
    if (value.includes("Claude是AI的") || value.includes("Claude 是 AI 的")) {
      failures.push(`${key}: ${value}`);
    }
    if (value.includes("铬")) {
      failures.push(`${key}: ${value}`);
    }
  }

  assert.deepEqual(failures, []);
  assert.equal(ion["FGmLea0H7Z"], "Claude是智能体，会犯错。请仔细核对引用的来源。");
  assert.equal(ion["klYUxraRh/"], "在你的Chrome里使用Claude");
  assert.equal(ion["sS42/B0p76"], "Chrome");
});

test("Claude Desktop runtime only translates DOM fallback labels inside UI text", () => {
  const generatedCode = text("Code");
  const uiCode = text("Code");
  const body = element("body", {}, [
    element("article", { class: "markdown prose" }, [generatedCode]),
    element("button", {}, [uiCode]),
  ]);

  runTranslationRuntimeOnBody(body);

  assert.equal(generatedCode.nodeValue, "Code");
  assert.equal(uiCode.nodeValue, "代码");
});
