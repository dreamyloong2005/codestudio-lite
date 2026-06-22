import fs from "node:fs";
import https from "node:https";
import os from "node:os";
import path from "node:path";

const patchDir = path.join(os.homedir(), ".codestudio-lite", "claude-desktop-patch");
const files = [
  path.join(patchDir, "zh-CN.json"),
  path.join(patchDir, "ion-dist", "i18n", "zh-CN.json"),
];
const cacheFile = path.join(patchDir, "translation-cache.zh-CN.json");
const progressFile = path.join(patchDir, "translation-progress.json");
const edgeHeaders = {
  "User-Agent":
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36 Edg/125.0.0.0",
};
let edgeToken = null;

const glossary = new Map(
  Object.entries({
    "Actual Size": "实际大小",
    "Add": "添加",
    "Advanced": "高级",
    "All": "全部",
    "Allow": "允许",
    "Appearance": "外观",
    "Avatar": "头像",
    "Apply": "应用",
    "Back": "返回",
    "Browse": "浏览",
    "Clear avatar": "清除头像",
    "Cancel": "取消",
    "Chat": "聊天",
    "Claude": "Claude",
    "Close": "关闭",
    "Code": "代码",
    "Connect": "连接",
    "Continue": "继续",
    "Copy": "复制",
    "Cowork": "Cowork",
    "Create": "创建",
    "Delete": "删除",
    "Disable": "禁用",
    "Done": "完成",
    "Download": "下载",
    "Edit": "编辑",
    "Enable": "启用",
    "Error": "错误",
    "Export": "导出",
    "Fable": "Fable",
    "File": "文件",
    "Forget": "忘记",
    "General": "通用",
    "Haiku": "Haiku",
    "Help": "帮助",
    "History": "历史记录",
    "Image": "图像",
    "Import": "导入",
    "Install": "安装",
    "Learn more": "了解更多",
    "Loading": "加载中",
    "Model": "模型",
    "New": "新建",
    "New chat": "新聊天",
    "New Conversation": "新对话",
    "Next": "下一步",
    "No": "否",
    "OK": "确定",
    "Open": "打开",
    "Opus": "Opus",
    "Paste": "粘贴",
    "Privacy": "隐私",
    "Project": "项目",
    "Projects": "项目",
    "Retry": "重试",
    "Save": "保存",
    "Saved": "已保存", "saved": "已保存",
    "Search": "搜索",
    "Send": "发送",
    "Settings": "设置",
    "Sign in": "登录",
    "Sign out": "退出登录",
    "Skip": "跳过",
    "Sonnet": "Sonnet",
    "Stop": "停止",
    "Submit": "提交",
    "Team": "团队",
    "Update": "更新",
    "Upload": "上传",
    "Yes": "是",
  }),
);

function readJson(file) {
  return JSON.parse(fs.readFileSync(file, "utf8"));
}

function writeJson(file, value) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, `${JSON.stringify(value, null, 2)}\n`, "utf8");
}

function isProbablyChinese(value) {
  return /[\u3400-\u9fff]/.test(value);
}

function placeholderMatches(value) {
  return [
    ...value.matchAll(/(<\/?[a-zA-Z][^>]*>|\{[^{}]+\}|%\d*\$?[sdif]|\$\{[^}]+\}|\[[A-Z_]+\])/g),
  ].map((match) => match[0]);
}

function protect(value) {
  const placeholders = [];
  let text = value;
  for (const placeholder of placeholderMatches(value)) {
    let index = placeholders.indexOf(placeholder);
    if (index === -1) {
      index = placeholders.length;
      placeholders.push(placeholder);
    }
    text = text.split(placeholder).join(`[[PH${index}]]`);
  }
  return { text, placeholders };
}

function restore(value, placeholders) {
  let text = value;
  placeholders.forEach((placeholder, index) => {
    text = text.replace(new RegExp(`\\[\\[\\s*PH\\s*${index}\\s*\\]\\]`, "gi"), placeholder);
  });
  return text;
}

function placeholdersPreserved(source, translated) {
  const expected = placeholderMatches(source).sort();
  const actual = placeholderMatches(translated).sort();
  return expected.length === actual.length && expected.every((item, index) => item === actual[index]);
}

function normalizeTranslation(value) {
  return value
    .replace(/克劳德/g, "Claude")
    .replace(/克洛德/g, "Claude")
    .replace(/人类/g, "Anthropic")
    .replace(/谷歌/g, "Google")
    .replace(/松弛/g, "Slack")
    .replace(/\s+([，。！？：；、）】》])/g, "$1")
    .replace(/([（【《])\s+/g, "$1")
    .replace(/\s+/g, " ")
    .trim();
}

function httpsGet(url, headers = {}) {
  return new Promise((resolve, reject) => {
    https
      .get(url, { headers }, (response) => {
        // Accumulate raw Buffer chunks and decode once at the end. Decoding
        // per-chunk corrupts multi-byte UTF-8 characters split across chunk
        // boundaries (each boundary produces a U+FFFD), which is what garbled
        // the previously generated zh-CN ion locale.
        const chunks = [];
        response.on("data", (chunk) => {
          chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk));
        });
        response.on("end", () => {
          resolve({ statusCode: response.statusCode, body: Buffer.concat(chunks).toString("utf8") });
        });
      })
      .on("error", reject);
  });
}

function httpsPostJson(url, payload, headers = {}) {
  const body = JSON.stringify(payload);
  return new Promise((resolve, reject) => {
    const request = https.request(
      url,
      {
        method: "POST",
        headers: {
          ...headers,
          "Content-Type": "application/json",
          "Content-Length": Buffer.byteLength(body),
        },
      },
      (response) => {
        // Accumulate raw Buffer chunks; see httpsGet for why per-chunk string
        // concatenation corrupts multi-byte UTF-8 across chunk boundaries.
        const chunks = [];
        response.on("data", (chunk) => {
          chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk));
        });
        response.on("end", () => {
          resolve({ statusCode: response.statusCode, body: Buffer.concat(chunks).toString("utf8") });
        });
      },
    );
    request.on("error", reject);
    request.write(body);
    request.end();
  });
}

async function edgeTranslate(texts) {
  if (!edgeToken) {
    const auth = await httpsGet("https://edge.microsoft.com/translate/auth", edgeHeaders);
    if (auth.statusCode !== 200 || !auth.body.trim().startsWith("ey")) {
      throw new Error(`Edge auth failed: HTTP ${auth.statusCode}, ${auth.body.slice(0, 160)}`);
    }
    edgeToken = auth.body.trim();
  }

  const response = await httpsPostJson(
    "https://api-edge.cognitive.microsofttranslator.com/translate?api-version=3.0&from=en&to=zh-Hans",
    texts.map((Text) => ({ Text })),
    {
      ...edgeHeaders,
      Authorization: `Bearer ${edgeToken}`,
    },
  );

  if (response.statusCode === 401) {
    edgeToken = null;
    return edgeTranslate(texts);
  }
  if (response.statusCode !== 200) {
    throw new Error(`Edge translate failed: HTTP ${response.statusCode}, ${response.body.slice(0, 240)}`);
  }

  const parsed = JSON.parse(response.body);
  return parsed.map((item) => item.translations?.[0]?.text ?? "");
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function collectEntries() {
  const entries = [];
  for (const file of files) {
    const object = readJson(file);
    for (const [key, value] of Object.entries(object)) {
      if (typeof value !== "string" || !value.trim() || isProbablyChinese(value)) continue;
      entries.push({ file, key, value });
    }
  }
  return entries;
}

function buildBatch(items, maxChars = 4500, maxLines = 80) {
  const batch = [];
  let chars = 0;
  for (const item of items) {
    const protectedItem = protect(item.value);
    const nextChars = chars + protectedItem.text.length + 1;
    if (batch.length > 0 && (batch.length >= maxLines || nextChars > maxChars)) break;
    batch.push({ ...item, ...protectedItem });
    chars = nextChars;
  }
  return batch;
}

function loadCache() {
  if (!fs.existsSync(cacheFile)) return {};
  return readJson(cacheFile);
}

function saveProgress(cache, stats) {
  writeJson(cacheFile, cache);
  writeJson(progressFile, {
    updatedAt: new Date().toISOString(),
    ...stats,
  });
}

async function main() {
  const cache = loadCache();
  const entries = collectEntries();
  let translated = 0;
  let skipped = 0;
  let failed = 0;
  for (const [source, target] of glossary.entries()) {
    cache[source] = target;
  }

  while (true) {
    const remaining = entries.filter((entry) => !cache[entry.value]);
    if (remaining.length === 0) break;
    const batch = buildBatch(remaining);

    let lines;
    try {
      lines = await edgeTranslate(batch.map((entry) => entry.text));
    } catch (error) {
      failed += batch.length;
      console.error(`[translate] failed: ${error.message}`);
      break;
    }

    if (lines.length !== batch.length) {
      failed += batch.length;
      console.error(`[translate] line mismatch: expected ${batch.length}, got ${lines.length}`);
      break;
    }

    for (let index = 0; index < batch.length; index += 1) {
      const entry = batch[index];
      const restored = normalizeTranslation(restore(lines[index], entry.placeholders));
      if (!restored || restored === entry.value || !placeholdersPreserved(entry.value, restored)) {
        cache[entry.value] = entry.value;
        skipped += 1;
        continue;
      }
      cache[entry.value] = restored;
      translated += 1;
    }

    if ((translated + skipped + failed) % 120 === 0) {
      saveProgress(cache, {
        total: entries.length,
        cached: Object.keys(cache).length,
        translated,
        skipped,
        failed,
      });
    }
    process.stdout.write(
      `\r[translate] total=${entries.length} cached=${Object.keys(cache).length} translated=${translated} skipped=${skipped} failed=${failed}`,
    );
    await sleep(120);
  }

  for (const file of files) {
    const object = readJson(file);
    let changed = 0;
    for (const [key, value] of Object.entries(object)) {
      if (typeof value === "string" && cache[value]) {
        object[key] = cache[value];
        changed += 1;
      }
    }
    writeJson(file, object);
    console.log(`\n[write] ${file}: ${changed} translated values`);
  }

  saveProgress(cache, {
    total: entries.length,
    cached: Object.keys(cache).length,
    translated,
    skipped,
    failed,
  });
  console.log(`\n[done] cache=${Object.keys(cache).length} translated=${translated} skipped=${skipped} failed=${failed}`);
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
