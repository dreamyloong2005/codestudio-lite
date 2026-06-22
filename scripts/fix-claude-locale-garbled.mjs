// One-shot repair of the bundled Claude Desktop zh-CN ion/shell locale files.
//
// Root cause: scripts/translate-claude-locale.mjs decoded HTTP response chunks
// per-chunk into a string, so multi-byte UTF-8 characters split across chunk
// boundaries turned into U+FFFD. This left ~107 ion rows garbled and ~550
// entries untranslated. This script keeps the 15843 already-clean translated
// values and re-translates only the missing + garbled entries via the Edge
// Translate API (with Buffer-accumulated responses so the corruption cannot
// recur), then writes the complete files back.

import fs from "node:fs";
import https from "node:https";
import os from "node:os";
import path from "node:path";

const patchDir = path.join(os.homedir(), ".codestudio-lite", "claude-desktop-patch");
const sourceFiles = [
  { en: path.join(patchDir, "zh-CN.json"), out: path.join(patchDir, "zh-CN.json") },
  {
    en: path.join(patchDir, "ion-dist", "i18n", "en-US.json"),
    out: path.join(patchDir, "ion-dist", "i18n", "zh-CN.json"),
  },
];
// Bundled files that ship with the app and must be updated too.
const bundledFiles = [
  "src-tauri/resources/claude-desktop/i18n/zh-CN.json",
  "src-tauri/resources/claude-desktop/i18n/ion-dist/i18n/zh-CN.json",
];
const repoRoot = path.resolve(import.meta.dirname, "..");

const glossary = new Map(
  Object.entries({
    "Actual Size": "实际大小", "Add": "添加", "Advanced": "高级", "All": "全部",
    "Allow": "允许", "Appearance": "外观", "Apply": "应用", "Avatar": "头像", "Back": "返回",
    "Browse": "浏览", "Cancel": "取消", "Chat": "聊天", "Clear avatar": "清除头像", "Claude": "Claude",
    "Close": "关闭", "Code": "代码", "Connect": "连接", "Continue": "继续",
    "Copy": "复制", "Create": "创建", "Delete": "删除", "Disable": "禁用",
    "Done": "完成", "Download": "下载", "Edit": "编辑", "Enable": "启用",
    "Error": "错误", "Export": "导出", "File": "文件", "Forget": "忘记",
    "General": "通用", "Help": "帮助", "History": "历史记录", "Image": "图像",
    "Import": "导入", "Install": "安装", "Learn more": "了解更多",
    "Loading": "加载中", "Model": "模型", "New": "新建", "New chat": "新聊天",
    "New Conversation": "新对话", "Next": "下一步", "No": "否", "OK": "确定",
    "Open": "打开", "Paste": "粘贴", "Privacy": "隐私", "Project": "项目",
    "Projects": "项目", "Retry": "重试", "Save": "保存", "Search": "搜索",
    "Send": "发送", "Settings": "设置", "Sign in": "登录", "Sign out": "退出登录",
    "Skip": "跳过", "Stop": "停止", "Submit": "提交", "Team": "团队",
    "Update": "更新", "Upload": "上传", "Yes": "是",
    "Cowork": "Cowork", "Saved": "已保存", "saved": "已保存",
    "Fable": "Fable", "Haiku": "Haiku", "Opus": "Opus", "Sonnet": "Sonnet",
  }),
);

function readJson(file) { return JSON.parse(fs.readFileSync(file, "utf8")); }
function writeJson(file, value) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, `${JSON.stringify(value, null, 2)}\n`, "utf8");
}
function isProbablyChinese(value) { return /[\u3400-\u9fff]/.test(value); }
function isGarbled(value) { return /[\uFFFD]/.test(value); }

function placeholderMatches(value) {
  return [...value.matchAll(/(<\/?[a-zA-Z][^>]*>|\{[^{}]+\}|%\d*\$?[sdif]|\$\{[^}]+\}|\[[A-Z_]+\])/g)].map((m) => m[0]);
}
function protect(value) {
  const placeholders = [];
  let text = value;
  for (const placeholder of placeholderMatches(value)) {
    let index = placeholders.indexOf(placeholder);
    if (index === -1) { index = placeholders.length; placeholders.push(placeholder); }
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
    .replace(/克劳德/g, "Claude").replace(/克洛德/g, "Claude")
    .replace(/人类/g, "Anthropic").replace(/谷歌/g, "Google").replace(/松弛/g, "Slack")
    .replace(/\s+([，。！？：；、）】》])/g, "$1").replace(/([（【《])\s+/g, "$1")
    .replace(/\s+/g, " ").trim();
}

function httpsPostJson(url, payload, headers = {}) {
  const body = JSON.stringify(payload);
  return new Promise((resolve, reject) => {
    const request = https.request(url, {
      method: "POST",
      headers: { ...headers, "Content-Type": "application/json", "Content-Length": Buffer.byteLength(body) },
    }, (response) => {
      const chunks = [];
      response.on("data", (chunk) => { chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk)); });
      response.on("end", () => { resolve({ statusCode: response.statusCode, body: Buffer.concat(chunks).toString("utf8") }); });
    });
    request.on("error", reject);
    request.write(body);
    request.end();
  });
}
function httpsGet(url, headers = {}) {
  return new Promise((resolve, reject) => {
    https.get(url, { headers }, (response) => {
      const chunks = [];
      response.on("data", (chunk) => { chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk)); });
      response.on("end", () => { resolve({ statusCode: response.statusCode, body: Buffer.concat(chunks).toString("utf8") }); });
    }).on("error", reject);
  });
}

const edgeHeaders = {
  "User-Agent":
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36 Edg/125.0.0.0",
};
let edgeToken = null;
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
    { ...edgeHeaders, Authorization: `Bearer ${edgeToken}` },
  );
  if (response.statusCode === 401) { edgeToken = null; return edgeTranslate(texts); }
  if (response.statusCode !== 200) {
    throw new Error(`Edge translate failed: HTTP ${response.statusCode}, ${response.body.slice(0, 240)}`);
  }
  const parsed = JSON.parse(response.body);
  return parsed.map((item) => item.translations?.[0]?.text ?? "");
}
function sleep(ms) { return new Promise((resolve) => setTimeout(resolve, ms)); }

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

async function main() {
  for (const { en, out } of sourceFiles) {
    if (!fs.existsSync(en)) { console.log(`[skip] missing source ${en}`); continue; }
    const enObj = readJson(en);
    let zhObj = {};
    if (fs.existsSync(out)) {
      try { zhObj = readJson(out); } catch { zhObj = {}; }
    }
    // Build the full target: keep clean existing zh values, retranslate missing/garbled.
    const todo = [];
    for (const [key, value] of Object.entries(enObj)) {
      if (typeof value !== "string" || !value.trim()) continue;
      if (glossary.has(value)) { zhObj[key] = glossary.get(value); continue; }
      const existing = zhObj[key];
      if (typeof existing === "string" && existing.trim() && !isGarbled(existing) && isProbablyChinese(existing)) {
        continue; // keep clean existing translation
      }
      todo.push({ key, value });
    }
    console.log(`[repair] ${path.basename(en)}: ${Object.keys(enObj).length} source, ${todo.length} to (re)translate`);

    let done = 0;
    while (todo.length > 0) {
      const batch = buildBatch(todo);
      const keys = batch.map((b) => b.key);
      let lines;
      try { lines = await edgeTranslate(batch.map((b) => b.text)); }
      catch (error) { console.error(`[translate] failed: ${error.message}`); break; }
      if (lines.length !== batch.length) { console.error(`[translate] mismatch`); break; }
      for (let i = 0; i < batch.length; i++) {
        const restored = normalizeTranslation(restore(lines[i], batch[i].placeholders));
        if (restored && restored !== batch[i].value && placeholdersPreserved(batch[i].value, restored)) {
          zhObj[keys[i]] = restored;
        } else {
          zhObj[keys[i]] = batch[i].value; // fallback to English if translation unusable
        }
      }
      // remove processed
      todo.splice(0, batch.length);
      done += batch.length;
      process.stdout.write(`\r[repair] ${path.basename(en)}: ${done} done, ${todo.length} left`);
      await sleep(120);
    }
    console.log("");
    writeJson(out, zhObj);
    console.log(`[write] ${out}`);
  }
  // Sync the bundled resource files from the patched outputs.
  for (let i = 0; i < bundledFiles.length; i++) {
    const src = sourceFiles[i].out;
    const dest = path.join(repoRoot, bundledFiles[i].replaceAll("/", path.sep));
    if (fs.existsSync(src)) {
      writeJson(dest, readJson(src));
      console.log(`[sync] ${dest}`);
    }
  }
  console.log("[done]");
}

main().catch((error) => { console.error(error); process.exit(1); });
