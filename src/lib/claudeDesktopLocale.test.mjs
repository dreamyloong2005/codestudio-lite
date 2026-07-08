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

test("Claude Desktop zh-CN locale covers Claude 1.19367 onboarding and memory labels", () => {
  const ion = readJson("src-tauri/resources/claude-desktop/i18n/ion-dist/i18n/zh-CN.json");

  assert.equal(ion["5OIWPYyjEY"], "开启记忆");
  assert.equal(ion["demo.h"], "开始使用 Claude");
  assert.equal(ion["vm1bq3+TX8"], "开始使用 Claude");
  assert.equal(ion["ejEGdxSUGs"], "首页");
  assert.equal(ion["zZtKixgRbu"], "获取 Pro 计划");
  assert.equal(ion["5vDuzZxMbU"], "试试 Cowork");
  assert.equal(ion["o2vHDAOpDQ"], "试试 Cowork");
  assert.equal(ion["MlGy39Hf4h"], "升级，让 Claude 为你处理真正的任务");
  assert.equal(ion["peYOflkXOK"], "如需在聊天间记住上下文，请<link>在设置中开启记忆</link>。");
  assert.equal(ion["+ZbIiH1928"], "允许 Claude 根据你的聊天生成记忆。");
  assert.equal(ion["3jWLm/Vcib"], "现在无法重新生成记忆");
  assert.equal(ion["8lL3FLt3SE"], "正在更新记忆…");
  assert.equal(ion["9jlvZMEAKh"], "读取记忆");
  assert.equal(ion["demo.memoryT"], "从其他 AI 导入记忆");
  assert.equal(ion["kvTeMA0Bfz"], "Claude 无法保存到记忆。");
  assert.equal(ion["sjdp6mtP7Y"], "正在读取记忆");

  for (const key of [
    "5OIWPYyjEY",
    "6fvtWClSA2",
    "YLcp+eluei",
    "peYOflkXOK",
    "+ZbIiH1928",
    "9jlvZMEAKh",
    "demo.memoryT",
    "kvTeMA0Bfz",
    "sjdp6mtP7Y",
  ]) {
    assert.ok(!ion[key].includes("内存"), `${key} should use 记忆 for Claude memory`);
    assert.ok(!ion[key].includes("存储器"), `${key} should not use storage-device wording`);
  }
});

test("Claude Desktop runtime only translates DOM fallback labels inside UI text", () => {
  const generatedCode = text("Code");
  const generatedHome = text("Home");
  const uiCode = text("Code");
  const uiHome = text("Home");
  const body = element("body", {}, [
    element("article", { class: "markdown prose" }, [generatedCode, generatedHome]),
    element("button", {}, [uiCode]),
    element("nav", {}, [uiHome]),
  ]);

  runTranslationRuntimeOnBody(body);

  assert.equal(generatedCode.nodeValue, "Code");
  assert.equal(generatedHome.nodeValue, "Home");
  assert.equal(uiCode.nodeValue, "代码");
  assert.equal(uiHome.nodeValue, "首页");
});

test("Claude Desktop runtime translates current onboarding CTA fallback labels", () => {
  const memory = text("Turn on memory");
  const pro = text("Get Pro plan");
  const start = text("Get started with Claude");
  const cowork = text("Try Cowork");
  const upgrade = text("Upgrade to let Claude take on real tasks for you");
  const generatedUpgrade = text("Upgrade to let Claude take on real tasks for you");
  const body = element("body", {}, [
    element("main", {}, [
      element("button", {}, [memory]),
      element("button", {}, [pro]),
      element("button", {}, [start]),
      element("button", {}, [cowork]),
      element("p", { role: "button" }, [upgrade]),
    ]),
    element("article", { class: "markdown prose" }, [generatedUpgrade]),
  ]);

  runTranslationRuntimeOnBody(body);

  assert.equal(memory.nodeValue, "开启记忆");
  assert.equal(pro.nodeValue, "获取 Pro 计划");
  assert.equal(start.nodeValue, "开始使用 Claude");
  assert.equal(cowork.nodeValue, "试试 Cowork");
  assert.equal(upgrade.nodeValue, "升级，让 Claude 为你处理真正的任务");
  assert.equal(generatedUpgrade.nodeValue, "Upgrade to let Claude take on real tasks for you");
});

test("Claude Desktop runtime translates first-screen copy from the real locale cache", () => {
  const greeting = text("Good morning, Alex");
  const monday = text("Happy Monday");
  const sunday = text("Happy Sunday, Alex");
  const question = text("What can I help you with today?");
  const headline = text("Let's knock something off your list");
  const generatedHeadline = text("Let's knock something off your list");
  const body = element("body", {}, [
    element("main", { "data-testid": "first-chat-empty-state" }, [
      element("h1", {}, [greeting]),
      element("h1", {}, [monday]),
      element("h1", {}, [sunday]),
      element("p", {}, [question]),
      element("p", {}, [headline]),
    ]),
    element("article", { class: "markdown prose" }, [generatedHeadline]),
  ]);

  runTranslationRuntimeOnBody(body);

  assert.equal(greeting.nodeValue, "早上好，Alex");
  assert.equal(monday.nodeValue, "周一快乐");
  assert.equal(sunday.nodeValue, "周日快乐，Alex");
  assert.equal(question.nodeValue, "今天有什么我可以帮忙的吗？");
  assert.equal(headline.nodeValue, "让我们从你的清单上砍掉一件事");
  assert.equal(generatedHeadline.nodeValue, "Let's knock something off your list");
});
