
(() => {
  const codestudioLiteInjectedSettings = __CODESTUDIO_LITE_SETTINGS__;
  const codestudioLiteLocalPluginMarketplaces = __CODESTUDIO_LITE_PLUGIN_MARKETPLACES__;
  const codestudioLiteCodexEnhancementsVersion = "5";
  window.__codestudioLiteCodexEnhancementSettings = codestudioLiteInjectedSettings;
  window.__codestudioLitePluginMarketplaces = codestudioLiteLocalPluginMarketplaces;
  function codestudioLiteSettings() {
    return window.__codestudioLiteCodexEnhancementSettings || codestudioLiteInjectedSettings;
  }
  if (window.__codestudioLiteCodexEnhancements === codestudioLiteCodexEnhancementsVersion) {
    window.__codestudioLiteCodexEnhancementsRefresh?.();
    return true;
  }
  if (window.__codestudioLiteCodexEnhancementsTimer) {
    clearInterval(window.__codestudioLiteCodexEnhancementsTimer);
    window.__codestudioLiteCodexEnhancementsTimer = null;
  }
  if (window.__codestudioLiteCodexEnhancementsObserver) {
    window.__codestudioLiteCodexEnhancementsObserver.disconnect?.();
    window.__codestudioLiteCodexEnhancementsObserver = null;
  }
  window.__codestudioLiteCodexEnhancements = codestudioLiteCodexEnhancementsVersion;
  const styleId = "codestudio-lite-codex-enhancement-style";
  const pluginMarketplaceUnlockVersion = "3";
  const codexPluginAutoExpandVersion = "1";
  const codexPluginAutoExpandMaxClicks = 80;
  const codexPluginAutoExpandClickDelayMs = 90;
  const codexModelJsonResponsePatchVersion = "2";
  const codexModelMessagePatchVersion = "2";
  const codexStatsigModelWhitelistPatchVersion = "2";
  const codexAppServerModelRequestPatchVersion = "2";
  const codexServiceTierRequestOverrideVersion = "3";
  const codexServiceTierBadgeClass = "codestudio-lite-service-tier-badge";
  const codexServiceTierBadgeVersion = "3";
  const codexThreadServiceTierVersion = "1";
  const codexThreadServiceTierKey = "codestudioLiteCodexThreadServiceTierOverrides";
  const codexThreadServiceTierMaxEntries = 120;
  const codexThreadServiceTierDraftBindWindowMs = 60 * 1000;
  const codexDefaultServiceTierSetting = { key: "default-service-tier", default: null };
  const codexServiceTierFallbackFastValue = "priority";
  const codexServiceTierSupportedFastModels = new Set([
    "gpt-5.4",
    "gpt-5.5",
    "gpt-5.6-sol",
    "gpt-5.6-terra",
    "gpt-5.6-luna",
  ]);
  const codexThreadServiceTierModes = new Set(["inherit", "standard", "fast"]);
  const codexServiceTierControlModes = new Set(["inherit", "global-standard", "global-fast", "custom"]);
  const modulePromises = new Map();
  let codexModelCatalog = normalizeModelCatalog(codestudioLiteSettings().modelCatalog);
  let codexModelCatalogPromise = null;
  let codexModelCatalogLoadedAt = 0;
  let codexModelWhitelistRefreshTimer = 0;
  let codexModelWhitelistRefreshUntil = 0;
  let codestudioLiteRefreshScheduled = false;
  let codestudioLitePendingMutations = null;
  let codestudioLiteSlowRefreshCount = 0;
  let codestudioLiteRefreshDisabledUntil = 0;
  let codexServiceTierComposerCache = { element: null, expiresAt: 0 };
  let codexServiceTierStateLoadStarted = false;
  let codexServiceTierState = {
    status: "loading",
    serviceTier: null,
    message: "正在读取…",
    fastTierValue: "priority",
    controlMode: "inherit",
    defaultMode: "inherit",
    activeThreadId: "",
    threadMode: "inherit",
    effectiveServiceTier: null,
    effectiveMode: "standard",
    fastModelName: "",
    fastSupported: false,
  };
  const codexModelListRequestIds = new Set();

  function ensureStyle() {
    if (document.getElementById(styleId)) return;
    const style = document.createElement("style");
    style.id = styleId;
    style.textContent = `
      .${codexServiceTierBadgeClass} {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        flex: 0 0 auto;
        height: 24px;
        min-width: 54px;
        box-sizing: border-box;
        border: 1px solid rgba(148,163,184,.28);
        border-radius: 999px;
        padding: 0 8px;
        font: 600 11px system-ui, sans-serif;
        color: inherit;
        background: rgba(148,163,184,.11);
        cursor: pointer;
      }
      .${codexServiceTierBadgeClass}:hover { border-color: rgba(16,163,127,.44); background: rgba(16,163,127,.13); }
      .${codexServiceTierBadgeClass}[data-tier="fast"] { border-color: rgba(16,163,127,.55); background: rgba(16,163,127,.18); color: #0f8f6a; }
      .${codexServiceTierBadgeClass}[data-tier="unsupported"] { border-color: rgba(251,191,36,.48); background: rgba(251,191,36,.13); color: #a16207; }
      .${codexServiceTierBadgeClass}[data-tier="loading"],
      .${codexServiceTierBadgeClass}[data-disabled="true"] { opacity: .62; cursor: not-allowed; }
      .codestudio-lite-codex-toast {
        position: fixed;
        left: 50%;
        bottom: 24px;
        transform: translateX(-50%);
        z-index: 2147483647;
        max-width: min(420px, calc(100vw - 32px));
        box-sizing: border-box;
        border: 1px solid rgba(148,163,184,.3);
        border-radius: 8px;
        padding: 9px 12px;
        color: #f8fafc;
        background: rgba(15,23,42,.94);
        box-shadow: 0 14px 40px rgba(15,23,42,.28);
        font: 500 12px/1.35 system-ui, sans-serif;
      }
    `;
    document.head.appendChild(style);
  }

  function recordPluginUnlockDiagnostic(event, payload = {}) {
    window.__codestudioLitePluginUnlockDiagnostics = window.__codestudioLitePluginUnlockDiagnostics || [];
    window.__codestudioLitePluginUnlockDiagnostics.push({ event, payload, at: Date.now() });
    if (window.__codestudioLitePluginUnlockDiagnostics.length > 80) {
      window.__codestudioLitePluginUnlockDiagnostics.splice(0, window.__codestudioLitePluginUnlockDiagnostics.length - 80);
    }
  }

  function codexAppAssetUrl(namePart) {
    const resources = [
      ...Array.from(document.scripts || []).map((script) => script.src),
      ...Array.from(document.querySelectorAll("link[href]") || []).map((link) => link.href),
      ...performance.getEntriesByType("resource").map((entry) => entry.name),
    ].filter(Boolean);
    return resources.find((url) => url.includes("/assets/") && url.includes(namePart) && url.split("?")[0].endsWith(".js")) || "";
  }

  async function codexAppAssetUrlFromScriptText(namePart) {
    const scripts = Array.from(document.scripts || []).map((script) => script.src).filter(Boolean);
    for (const src of scripts) {
      if (!src.includes("/assets/") || !src.split("?")[0].endsWith(".js")) continue;
      try {
        const text = await fetch(src).then((response) => response.ok ? response.text() : "");
        const escaped = namePart.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
        const match = text.match(new RegExp(`["'](\\./assets/${escaped}[^"']+\\.js)["']`));
        if (match) return new URL(match[1], src).href;
      } catch {
      }
    }
    return "";
  }

  async function loadCodexAppModule(namePart) {
    if (!modulePromises.has(namePart)) {
      const promise = Promise.resolve().then(async () => {
        const url = codexAppAssetUrl(namePart) || await codexAppAssetUrlFromScriptText(namePart);
        if (!url) throw new Error(`Codex asset not found: ${namePart}`);
        return await import(url);
      }).catch((error) => {
        modulePromises.delete(namePart);
        throw error;
      });
      modulePromises.set(namePart, promise);
    }
    return await modulePromises.get(namePart);
  }

  async function codexSettingStorageModule() {
    const module = await loadCodexAppModule("setting-storage-");
    if (typeof module.n !== "function" || typeof module.s !== "function") {
      throw new Error("Codex setting-storage interface unavailable");
    }
    return module;
  }

  async function getCodexServiceTierSetting() {
    const settingStorage = await codexSettingStorageModule();
    return await settingStorage.n(codexDefaultServiceTierSetting);
  }

  function appServerPluginRequestMethod(method, params) {
    if (method === "send-cli-request-for-host" && params?.method) return String(params.method);
    return String(method || "");
  }

  function patchPluginMarketplaceRequestParams(method, params) {
    if (method === "list-plugins") {
      if (!params || typeof params !== "object") return params;
    } else {
      return params;
    }
    const next = { ...params };
    const hadMarketplaceKinds = Object.prototype.hasOwnProperty.call(next, "marketplaceKinds");
    const nextKinds = Array.isArray(next.marketplaceKinds)
      ? next.marketplaceKinds.map((kind) => restorePluginMarketplaceName(kind))
      : ["local"];
    if (!nextKinds.includes("vertical")) nextKinds.push("vertical");
    next.marketplaceKinds = Array.from(new Set(nextKinds));
    recordPluginUnlockDiagnostic("plugin_marketplace_request_expanded", {
      hadMarketplaceKinds,
      marketplaceKinds: next.marketplaceKinds,
      cwdCount: Array.isArray(next.cwds) ? next.cwds.length : 0,
    });
    return next;
  }

  function displayNameForPluginMarketplaceName(name, fallback) {
    if (name === "openai-bundled" || name === "codestudio-lite-openai-bundled") return "OpenAI插件1(CodeStudio)";
    if (name === "openai-curated" || name === "codestudio-lite-openai-curated") return "OpenAI插件2(CodeStudio)";
    if (name === "openai-primary-runtime" || name === "codestudio-lite-openai-primary-runtime") return "OpenAI插件3(CodeStudio)";
    if (name === "openai-api-curated" || name === "codestudio-lite-openai-api-curated") return "OpenAI插件4(CodeStudio)";
    if (name === "openai-curated-remote" || name === "codestudio-lite-openai-curated-remote") return "OpenAI插件5(CodeStudio)";
    return fallback;
  }

  function patchPluginMarketplaceObject(marketplace) {
    if (!marketplace || typeof marketplace !== "object" || marketplace.__codestudioLiteMarketplaceUnlockPatched) return false;
    const displayName = displayNameForPluginMarketplaceName(marketplace.name, marketplace.displayName || marketplace.title || marketplace.label || marketplace.name);
    if (!displayName || displayName === marketplace.name) return false;
    marketplace.displayName = displayName;
    marketplace.title = displayName;
    marketplace.label = displayName;
    if (marketplace.interface && typeof marketplace.interface === "object") {
      marketplace.interface = {
        ...marketplace.interface,
        displayName,
        name: displayName,
        title: displayName,
        label: displayName,
      };
    } else {
      marketplace.interface = { displayName, name: displayName, title: displayName, label: displayName };
    }
    marketplace.__codestudioLiteMarketplaceUnlockPatched = true;
    return true;
  }

  function cloneCodexPluginMarketplace(value) {
    if (!value || typeof value !== "object") return null;
    try {
      return JSON.parse(JSON.stringify(value));
    } catch (_) {
      return null;
    }
  }

  function pluginMarketplacePluginKey(plugin) {
    if (!plugin || typeof plugin !== "object") return "";
    return String(plugin.name || plugin.id || plugin.pluginName || "").trim();
  }

  function normalizeLocalPluginMarketplacePlugin(plugin, marketplaceName) {
    const cloned = cloneCodexPluginMarketplace(plugin);
    if (!cloned || typeof cloned !== "object") return null;
    const name = String(cloned.name || cloned.id || cloned.pluginName || "").trim();
    if (!name) return null;
    if (!cloned.name) cloned.name = name;
    if (!cloned.id) cloned.id = `${name}@${marketplaceName}`;
    if (!cloned.marketplaceName) cloned.marketplaceName = marketplaceName;
    if (!cloned.marketplacePath) cloned.marketplacePath = marketplaceName;
    if (!cloned.interface || typeof cloned.interface !== "object") cloned.interface = {};
    if (!cloned.interface.displayName) cloned.interface.displayName = name;
    if (!Array.isArray(cloned.keywords)) cloned.keywords = [];
    return cloned;
  }

  function mergePluginMarketplacePlugins(target, source) {
    if (!target || !source || !Array.isArray(source.plugins)) return 0;
    if (!Array.isArray(target.plugins)) target.plugins = [];
    const marketplaceName = restorePluginMarketplaceName(target.name || source.name || "");
    const existing = new Set(target.plugins.map(pluginMarketplacePluginKey).filter(Boolean));
    let added = 0;
    source.plugins.forEach((plugin) => {
      const key = pluginMarketplacePluginKey(plugin);
      if (!key || existing.has(key)) return;
      const cloned = normalizeLocalPluginMarketplacePlugin(plugin, marketplaceName);
      if (!cloned) return;
      target.plugins.push(cloned);
      existing.add(key);
      added += 1;
    });
    return added;
  }

  function mergeLocalPluginMarketplaces(result) {
    if (!result || typeof result !== "object" || !Array.isArray(result.marketplaces)) {
      return { addedMarketplaces: 0, addedPlugins: 0 };
    }
    const localMarketplaces = Array.isArray(window.__codestudioLitePluginMarketplaces)
      ? window.__codestudioLitePluginMarketplaces
      : [];
    if (!localMarketplaces.length) return { addedMarketplaces: 0, addedPlugins: 0 };
    const byName = new Map();
    result.marketplaces.forEach((marketplace) => {
      const name = restorePluginMarketplaceName(marketplace?.name || "");
      if (name) byName.set(name, marketplace);
    });
    let addedMarketplaces = 0;
    let addedPlugins = 0;
    localMarketplaces.forEach((marketplace) => {
      const name = restorePluginMarketplaceName(marketplace?.name || "");
      if (!name) return;
      const existing = byName.get(name);
      if (existing) {
        addedPlugins += mergePluginMarketplacePlugins(existing, marketplace);
        return;
      }
      const cloned = cloneCodexPluginMarketplace(marketplace);
      if (!cloned) return;
      cloned.plugins = Array.isArray(cloned.plugins)
        ? cloned.plugins.map((plugin) => normalizeLocalPluginMarketplacePlugin(plugin, name)).filter(Boolean)
        : [];
      result.marketplaces.push(cloned);
      byName.set(name, cloned);
      addedMarketplaces += 1;
      addedPlugins += cloned.plugins.length;
    });
    if (addedMarketplaces > 0 || addedPlugins > 0) {
      recordPluginUnlockDiagnostic("plugin_marketplace_local_merged", { addedMarketplaces, addedPlugins });
    }
    return { addedMarketplaces, addedPlugins };
  }

  function restorePluginMarketplaceName(name) {
    if (name === "codestudio-lite-openai-bundled" || name === "codex-plus-openai-bundled") return "openai-bundled";
    if (name === "codestudio-lite-openai-curated" || name === "codex-plus-openai-curated") return "openai-curated";
    if (name === "codestudio-lite-openai-primary-runtime" || name === "codex-plus-openai-primary-runtime") return "openai-primary-runtime";
    if (name === "codestudio-lite-openai-api-curated" || name === "codex-plus-openai-api-curated") return "openai-api-curated";
    if (name === "codestudio-lite-openai-curated-remote" || name === "codex-plus-openai-curated-remote") return "openai-curated-remote";
    return name;
  }

  function codexPluginOfficialMarketplaceName(name) {
    const restored = restorePluginMarketplaceName(name);
    return restored === "openai-bundled" || restored === "openai-curated" || restored === "openai-primary-runtime" || restored === "openai-api-curated" || restored === "openai-curated-remote";
  }

  function isCodexPluginBuildFlavorFilter(callback, sample) {
    if (!Array.isArray(sample) || sample.length === 0 || typeof callback !== "function") return false;
    let source = "";
    try {
      source = Function.prototype.toString.call(callback);
    } catch (_) {
      return false;
    }
    if (!source.includes("!u(e.marketplaceName)||e.marketplaceName===r")) return false;
    if (!sample.some((plugin) => codexPluginOfficialMarketplaceName(plugin?.marketplaceName))) return false;
    return sample.some((plugin) => codexPluginOfficialMarketplaceName(plugin?.marketplaceName) && !callback(plugin));
  }

  function isCodexPluginMarketplaceHiddenFilter(callback, sample) {
    if (!Array.isArray(sample) || sample.length === 0 || typeof callback !== "function") return false;
    let source = "";
    try {
      source = Function.prototype.toString.call(callback);
    } catch (_) {
      return false;
    }
    if (!source.includes("!t.includes(e.name)")) return false;
    if (!sample.some((marketplace) => codexPluginOfficialMarketplaceName(marketplace?.name))) return false;
    return sample.some((marketplace) => codexPluginOfficialMarketplaceName(marketplace?.name) && !callback(marketplace));
  }

  function installPluginBuildFlavorFilterPatch() {
    if (window.__codestudioLitePluginBuildFlavorFilterPatch === pluginMarketplaceUnlockVersion) return;
    const originalFilter = Array.prototype.__codestudioLitePluginBuildFlavorOriginalFilter || Array.prototype.filter;
    if (!Array.prototype.__codestudioLitePluginBuildFlavorOriginalFilter) {
      Object.defineProperty(Array.prototype, "__codestudioLitePluginBuildFlavorOriginalFilter", {
        value: originalFilter,
        configurable: true,
        writable: true,
      });
    }
    if (Array.prototype.filter.__codestudioLitePluginBuildFlavorPatched === pluginMarketplaceUnlockVersion) {
      window.__codestudioLitePluginBuildFlavorFilterPatch = pluginMarketplaceUnlockVersion;
      return;
    }
    const patchedFilter = function codestudioLitePluginBuildFlavorFilterPatch(callback, thisArg) {
      if (isCodexPluginBuildFlavorFilter(callback, this)) {
        recordPluginUnlockDiagnostic("plugin_build_flavor_filter_bypassed", { pluginCount: this.length });
        return Array.from(this);
      }
      if (isCodexPluginMarketplaceHiddenFilter(callback, this)) {
        recordPluginUnlockDiagnostic("plugin_marketplace_hidden_filter_bypassed", { marketplaceCount: this.length });
        return Array.from(this);
      }
      return originalFilter.call(this, callback, thisArg);
    };
    patchedFilter.__codestudioLitePluginBuildFlavorPatched = pluginMarketplaceUnlockVersion;
    Array.prototype.filter = patchedFilter;
    window.__codestudioLitePluginBuildFlavorFilterPatch = pluginMarketplaceUnlockVersion;
    recordPluginUnlockDiagnostic("plugin_build_flavor_filter_patch_installed");
  }

  function restorePluginMarketplaceRequestParams(params, method = "") {
    if (!params || typeof params !== "object") return params;
    let next = params;
    if (Array.isArray(params.marketplaceKinds)) {
      const nextKinds = params.marketplaceKinds.map((kind) => {
        if (kind === "remote:openai-curated") return "openai-curated";
        return restorePluginMarketplaceName(kind);
      });
      next = { ...next, marketplaceKinds: Array.from(new Set(nextKinds)) };
    }
    if (method === "install-plugin") {
      next = next === params ? { ...params } : { ...next };
      if (next.remoteMarketplaceName) next.remoteMarketplaceName = restorePluginMarketplaceName(next.remoteMarketplaceName);
      if (typeof next.marketplacePath === "string" && next.marketplacePath.startsWith("remote:")) {
        const remoteMarketplaceName = next.marketplacePath.slice("remote:".length);
        delete next.marketplacePath;
        next.remoteMarketplaceName = restorePluginMarketplaceName(remoteMarketplaceName);
      }
    }
    return next;
  }

  function patchPluginMarketplaceResult(method, result) {
    if (method !== "list-plugins") return result;
    let patchedCount = 0;
    try {
      if (Array.isArray(result?.marketplaces)) {
        mergeLocalPluginMarketplaces(result);
        result.marketplaces.forEach((marketplace) => {
          if (patchPluginMarketplaceObject(marketplace)) patchedCount += 1;
        });
      }
      if (patchedCount > 0) {
        recordPluginUnlockDiagnostic("plugin_marketplace_response_expanded", { patchedCount });
      }
    } catch (error) {
      recordPluginUnlockDiagnostic("plugin_marketplace_response_patch_failed", {
        errorName: error?.name || "",
        errorMessage: error?.message || String(error),
      });
    }
    return result;
  }

  function patchPluginMarketplaceRequestClient(client) {
    if (!client || typeof client.sendRequest !== "function") return false;
    if (client.__codestudioLitePluginMarketplaceUnlockPatch === pluginMarketplaceUnlockVersion) return true;
    const originalSendRequest = client.__codestudioLitePluginMarketplaceOriginalSendRequest || client.sendRequest.bind(client);
    client.__codestudioLitePluginMarketplaceOriginalSendRequest = originalSendRequest;
    client.sendRequest = async function codestudioLitePluginMarketplacePatchedSendRequest(method, params, options) {
      const requestMethod = appServerPluginRequestMethod(String(method || ""), params);
      const requestParams = patchPluginMarketplaceRequestParams(requestMethod, restorePluginMarketplaceRequestParams(params, requestMethod));
      if (requestMethod === "install-plugin") {
        recordPluginUnlockDiagnostic("plugin_install_request_debug", {
          method: String(method || ""),
          requestMarketplacePath: requestParams?.marketplacePath || null,
          requestRemoteMarketplaceName: requestParams?.remoteMarketplaceName || null,
          requestPluginName: requestParams?.pluginName || null,
        });
      }
      const result = await originalSendRequest(method, requestParams, options);
      return patchPluginMarketplaceResult(requestMethod, result);
    };
    client.__codestudioLitePluginMarketplaceUnlockPatch = pluginMarketplaceUnlockVersion;
    return true;
  }

  function installPluginMarketplaceRequestPatch() {
    if (window.__codestudioLitePluginMarketplaceUnlockInstalled === pluginMarketplaceUnlockVersion) return;
    if (window.__codestudioLitePluginMarketplaceUnlockPending) return;
    window.__codestudioLitePluginMarketplaceUnlockPending = true;
    Promise.resolve().then(async () => {
      const module = await loadCodexAppModule("app-server-manager-signals-");
      const candidates = Object.values(module).filter((value) => value && typeof value === "object");
      let patchedCount = 0;
      for (const candidate of candidates) {
        if (patchPluginMarketplaceRequestClient(candidate)) patchedCount += 1;
        if (typeof candidate.sendRequest !== "function" && typeof candidate.get === "function") {
          try {
            if (patchPluginMarketplaceRequestClient(candidate.get())) patchedCount += 1;
          } catch (_) {
          }
        }
      }
      if (patchedCount > 0) {
        window.__codestudioLitePluginMarketplaceUnlockInstalled = pluginMarketplaceUnlockVersion;
        recordPluginUnlockDiagnostic("plugin_marketplace_request_patch_installed", {
          candidateCount: candidates.length,
          patchedCount,
        });
      } else {
        recordPluginUnlockDiagnostic("plugin_marketplace_request_patch_not_found", {
          exportCount: Object.keys(module || {}).length,
          candidateCount: candidates.length,
        });
      }
    }).catch((error) => {
      recordPluginUnlockDiagnostic("plugin_marketplace_request_patch_failed", {
        errorName: error?.name || "",
        errorMessage: error?.message || String(error),
      });
    }).finally(() => {
      window.__codestudioLitePluginMarketplaceUnlockPending = false;
    });
  }

  function recordCodexEnhancementDiagnostic(event, payload = {}) {
    recordPluginUnlockDiagnostic(event, payload);
  }

  function uniqueValues(values) {
    return Array.from(new Set((values || []).map((value) => String(value || "").trim()).filter(Boolean)));
  }

  function setCodestudioLiteText(node, value) {
    const next = String(value ?? "");
    if (node.textContent !== next) node.textContent = next;
  }

  function setCodestudioLiteAttribute(node, name, value) {
    const next = String(value ?? "");
    if (node.getAttribute(name) !== next) node.setAttribute(name, next);
  }

  function setCodestudioLiteProperty(node, name, value) {
    if (node[name] !== value) node[name] = value;
  }

  function setCodestudioLiteBooleanProperty(node, name, value) {
    const next = !!value;
    if (node[name] !== next) node[name] = next;
  }

  function setCodestudioLiteDataset(node, name, value) {
    const next = String(value ?? "");
    if (node.dataset[name] !== next) node.dataset[name] = next;
  }

  function normalizeModelCatalog(value) {
    const source = value && typeof value === "object" ? value : {};
    return {
      status: source.status || "ok",
      model: String(source.model || ""),
      default_model: String(source.default_model || source.defaultModel || ""),
      model_provider: String(source.model_provider || source.modelProvider || ""),
      provider_name: String(source.provider_name || source.providerName || ""),
      models: uniqueValues(source.models || []),
      sources: Array.isArray(source.sources) ? source.sources : [],
      responses_api: source.responses_api || source.responsesApi || { status: "unknown", message: "" },
    };
  }

  function finiteNonNegativeNumber(value) {
    const numeric = Number(value);
    return Number.isFinite(numeric) && numeric >= 0 ? numeric : 0;
  }

  function validThreadScrollSessionKey(sessionId) {
    const key = String(sessionId || "").trim();
    if (!key || key === "__proto__" || key === "prototype" || key === "constructor") return "";
    return /^[A-Za-z0-9_.-]{8,128}$/.test(key) ? key : "";
  }

  function locationThreadId() {
    const source = `${window.location.pathname}${window.location.search}${window.location.hash}`;
    const match = source.match(/(?:session|conversation|thread)(?:\/|=|:|-)([A-Za-z0-9_.-]+)/i)
      || source.match(/\/([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12})(?:[/?#]|$)/)
      || source.match(/\/([A-Za-z0-9_-]{24,})(?:[/?#]|$)/);
    return match ? decodeURIComponent(match[1]) : "";
  }

  function currentSessionRef() {
    return { session_id: locationThreadId(), title: "" };
  }

  function showToast(message) {
    document.querySelectorAll(".codestudio-lite-codex-toast").forEach((node) => node.remove());
    const toast = document.createElement("div");
    toast.className = "codestudio-lite-codex-toast";
    toast.textContent = message;
    document.body.appendChild(toast);
    setTimeout(() => toast.remove(), 5000);
  }

  function pluginAutoExpandVisibleElement(el) {
    if (!(el instanceof HTMLElement) || !el.isConnected) return false;
    const style = getComputedStyle(el);
    if (style.display === "none" || style.visibility === "hidden" || style.pointerEvents === "none") return false;
    const rect = el.getBoundingClientRect();
    return rect.width > 0 && rect.height > 0;
  }

  function pluginAutoExpandPageLooksRelevant() {
    const text = String(document.body?.innerText || "");
    return /插件|Plugins?|Marketplace|市场/i.test(text) && !!document.querySelector('button, [role="button"]');
  }

  function pluginAutoExpandButtonLooksScoped(button) {
    let node = button;
    for (let depth = 0; node instanceof HTMLElement && node !== document.body && depth < 8; depth += 1, node = node.parentElement) {
      const text = String(node.innerText || "");
      if (text.length > 16000) continue;
      if (/插件|Plugins?|Marketplace|市场/i.test(text)) return true;
    }
    return false;
  }

  function pluginAutoExpandButtonText(button) {
    return String(button?.textContent || button?.getAttribute?.("aria-label") || button?.getAttribute?.("title") || "")
      .replace(/\s+/g, " ")
      .trim();
  }

  function pluginAutoExpandButtonLooksLikeMore(button) {
    const text = pluginAutoExpandButtonText(button);
    if (!text || text.length > 120) return false;
    if (/^(更多|显示更多|查看更多|加载更多|Show more|Load more|More)$/i.test(text)) return true;
    if (/^查看\s+.+以及另外\s*\d+\s*个$/i.test(text)) return true;
    if (/^View\s+.+\s+and\s+\d+\s+more$/i.test(text)) return true;
    if (/^Show\s+.+\s+and\s+\d+\s+more$/i.test(text)) return true;
    return false;
  }

  function pluginAutoExpandButtonCandidates() {
    if (!codestudioLiteSettings().pluginAutoExpand || !pluginAutoExpandPageLooksRelevant()) return [];
    return Array.from(document.querySelectorAll('button, [role="button"]'))
      .filter(pluginAutoExpandVisibleElement)
      .filter((button) => !button.disabled && button.getAttribute("aria-disabled") !== "true")
      .filter(pluginAutoExpandButtonLooksLikeMore)
      .filter(pluginAutoExpandButtonLooksScoped)
      .filter((button) => !button.closest?.(`#${styleId}, .${codexServiceTierBadgeClass}`));
  }

  function pluginAutoExpandSignature() {
    return pluginAutoExpandButtonCandidates()
      .map((button) => {
        const rect = button.getBoundingClientRect();
        return `${pluginAutoExpandButtonText(button)}:${Math.round(rect.top)}:${Math.round(rect.left)}`;
      })
      .join("|");
  }

  function schedulePluginAutoExpand(force = false) {
    if (!codestudioLiteSettings().pluginAutoExpand) return;
    if (window.__codexPluginAutoExpandRunning && !force) return;
    clearTimeout(window.__codexPluginAutoExpandTimer);
    window.__codexPluginAutoExpandTimer = setTimeout(() => runPluginAutoExpand(force), force ? 30 : 180);
  }

  function runPluginAutoExpand(force = false) {
    if (!codestudioLiteSettings().pluginAutoExpand) return;
    const currentSignature = pluginAutoExpandSignature();
    if (!force && currentSignature && currentSignature === window.__codexPluginAutoExpandLastSignature) return;
    window.__codexPluginAutoExpandLastSignature = currentSignature;
    window.__codexPluginAutoExpandRunning = true;
    window.__codexPluginAutoExpandClicks = 0;
    const clickNext = () => {
      if (!codestudioLiteSettings().pluginAutoExpand) {
        window.__codexPluginAutoExpandRunning = false;
        return;
      }
      const button = pluginAutoExpandButtonCandidates()[0];
      if (!button || window.__codexPluginAutoExpandClicks >= codexPluginAutoExpandMaxClicks) {
        window.__codexPluginAutoExpandRunning = false;
        recordCodexEnhancementDiagnostic("plugin_auto_expand_finished", {
          version: codexPluginAutoExpandVersion,
          clicks: window.__codexPluginAutoExpandClicks || 0,
          exhausted: !!button,
        });
        return;
      }
      window.__codexPluginAutoExpandClicks = (window.__codexPluginAutoExpandClicks || 0) + 1;
      button.dataset.codexPluginAutoExpandClicked = String(Date.now());
      button.click();
      setTimeout(clickNext, codexPluginAutoExpandClickDelayMs);
    };
    clickNext();
  }

  function codexPlusModelUnlockEnabled() {
    return !!codestudioLiteSettings().modelWhitelistUnlock;
  }

  function codexPlusModelNames() {
    codexModelCatalog = normalizeModelCatalog(codestudioLiteSettings().modelCatalog || codexModelCatalog);
    return uniqueValues([
      codexModelCatalog.default_model,
      codexModelCatalog.model,
      ...(Array.isArray(codexModelCatalog.models) ? codexModelCatalog.models : []),
    ]);
  }

  async function loadCodexModelCatalog(force = false) {
    if (!force && codexModelCatalogPromise) return codexModelCatalogPromise;
    if (!force && codexModelCatalogLoadedAt && Date.now() - codexModelCatalogLoadedAt < 10000) return codexModelCatalog;
    codexModelCatalogPromise = Promise.resolve().then(() => {
      codexModelCatalog = normalizeModelCatalog(codestudioLiteSettings().modelCatalog);
      codexModelCatalogLoadedAt = Date.now();
      scheduleCodexModelWhitelistRefresh();
      return codexModelCatalog;
    }).finally(() => {
      codexModelCatalogPromise = null;
    });
    return codexModelCatalogPromise;
  }

  function modelReasoningEfforts() {
    return ["minimal", "low", "medium", "high", "xhigh"].map((reasoningEffort) => ({ reasoningEffort, description: `${reasoningEffort} effort` }));
  }

  function codexPlusModelDescriptor(modelName) {
    return {
      model: modelName,
      id: modelName,
      slug: modelName,
      name: modelName,
      displayName: modelName,
      description: codexModelCatalog.provider_name || codexModelCatalog.model_provider || "Custom model",
      hidden: false,
      isDefault: (codexModelCatalog.default_model || codexModelCatalog.model) === modelName,
      defaultReasoningEffort: "medium",
      supportedReasoningEfforts: modelReasoningEfforts(),
    };
  }

  function modelArrayLooksPatchable(value, allowEmpty = false) {
    return Array.isArray(value)
      && (allowEmpty || value.length > 0)
      && value.every((item) => item && typeof item === "object" && typeof item.model === "string");
  }

  function stringArrayLooksPatchable(value) {
    return Array.isArray(value) && value.every((item) => typeof item === "string");
  }

  function patchModelNameArray(models) {
    if (!stringArrayLooksPatchable(models)) return false;
    const customModels = codexPlusModelNames();
    if (!customModels.length) return false;
    let changed = false;
    customModels.forEach((modelName) => {
      if (!models.includes(modelName)) {
        models.push(modelName);
        changed = true;
      }
    });
    return changed;
  }

  function patchModelArray(models, allowEmpty = false) {
    if (!modelArrayLooksPatchable(models, allowEmpty)) return false;
    const customModels = codexPlusModelNames();
    if (!customModels.length) return false;
    let changed = false;
    const existing = new Map(models.map((item) => [item.model, item]));
    models.forEach((item) => {
      if (customModels.includes(item.model) && item.hidden !== false) {
        item.hidden = false;
        changed = true;
      }
    });
    customModels.forEach((modelName) => {
      if (!existing.has(modelName)) {
        models.push(codexPlusModelDescriptor(modelName));
        changed = true;
      }
    });
    return changed;
  }

  function patchModelContainer(value) {
    if (!value || typeof value !== "object") return false;
    let changed = false;
    if (patchModelArray(value.models, "defaultModel" in value || "availableModels" in value)) changed = true;
    if (patchModelNameArray(value.models)) changed = true;
    if (patchModelArray(value.data)) changed = true;
    if (patchModelArray(value.result)) changed = true;
    if (patchModelArray(value.pages?.[0]?.data)) changed = true;
    if (patchModelArray(value.result?.data)) changed = true;
    if (patchModelArray(value.result?.models)) changed = true;
    if (patchModelArray(value.message?.result?.data)) changed = true;
    if (patchModelArray(value.message?.result?.models)) changed = true;
    const names = codexPlusModelNames();
    for (const key of ["availableModels", "available_models"]) {
      if (value[key] instanceof Set) {
        names.forEach((name) => {
          if (!value[key].has(name)) {
            value[key].add(name);
            changed = true;
          }
        });
      } else if (Array.isArray(value[key])) {
        names.forEach((name) => {
          if (!value[key].includes(name)) {
            value[key].push(name);
            changed = true;
          }
        });
      }
    }
    for (const key of ["hiddenModels", "hidden_models"]) {
      if (Array.isArray(value[key])) {
        const before = value[key].length;
        value[key] = value[key].filter((name) => !names.includes(name));
        if (value[key].length !== before) changed = true;
      }
    }
    const customNames = codexPlusModelUnlockEnabled() ? codexPlusModelNames() : [];
    if (value.defaultModel == null && customNames.length > 0) {
      value.defaultModel = codexPlusModelDescriptor(customNames[0]);
      changed = true;
    } else if (typeof value.defaultModel === "string" && customNames.includes(value.defaultModel) && value.model == null) {
      value.model = value.defaultModel;
      changed = true;
    }
    return changed;
  }

  function patchObjectGraphForModels(root, visited, depth = 0) {
    if (!root || typeof root !== "object" || visited.has(root) || depth > 5) return false;
    visited.add(root);
    let changed = patchModelContainer(root);
    if (root instanceof Element || root === window || root === document || root === document.body || root === document.documentElement) return changed;
    for (const key of Object.keys(root)) {
      if (key === "ownerDocument" || key === "parentElement" || key === "parentNode" || key === "children" || key === "childNodes") continue;
      let value;
      try {
        value = root[key];
      } catch {
        continue;
      }
      if (value && typeof value === "object" && patchObjectGraphForModels(value, visited, depth + 1)) changed = true;
    }
    return changed;
  }

  async function patchModelJsonResponse(payload) {
    if (!codexPlusModelUnlockEnabled()) return payload;
    if (!codexPlusModelNames().length) await loadCodexModelCatalog();
    if (!payload || typeof payload !== "object") return payload;
    try {
      patchModelContainer(payload);
      patchObjectGraphForModels(payload, new WeakSet(), 0);
    } catch (error) {
      recordCodexEnhancementDiagnostic("model_json_patch_failed", { errorMessage: String(error?.message || error) });
    }
    return payload;
  }

  function installModelJsonResponsePatch() {
    if (window.__codestudioLiteModelJsonResponsePatchInstalled === codexModelJsonResponsePatchVersion) return;
    window.__codestudioLiteModelJsonResponsePatchInstalled = codexModelJsonResponsePatchVersion;
    window.__codestudioLiteModelJsonResponseOriginals = window.__codestudioLiteModelJsonResponseOriginals || {};
    const originals = window.__codestudioLiteModelJsonResponseOriginals;
    originals.responseJson = originals.responseJson || Response.prototype.json;
    if (typeof originals.responseJson !== "function") return;
    Response.prototype.json = async function codestudioLitePatchedResponseJson(...args) {
      const payload = await originals.responseJson.apply(this, args);
      return await patchModelJsonResponse(payload);
    };
  }

  function patchStatsigModelDynamicConfig(config) {
    const names = codexPlusModelNames();
    const value = config?.value;
    if (!names.length || !value || typeof value !== "object") return config;
    const availableModels = Array.isArray(value.available_models) ? [...value.available_models] : [];
    let changed = false;
    names.forEach((name) => {
      if (!availableModels.includes(name)) {
        availableModels.push(name);
        changed = true;
      }
    });
    const nextValue = {
      ...value,
      available_models: availableModels,
      default_model: names[0] || value.default_model,
    };
    if (!changed && nextValue.default_model === value.default_model) return config;
    try {
      config.value = nextValue;
    } catch {
      return { ...config, value: nextValue };
    }
    return config;
  }

  function statsigClients() {
    const root = window.__STATSIG__ || globalThis.__STATSIG__;
    if (!root || typeof root !== "object") return [];
    const clients = [root.firstInstance, typeof root.instance === "function" ? root.instance() : null];
    if (root.instances && typeof root.instances === "object") clients.push(...Object.values(root.instances));
    return clients.filter((client, index, array) => client && typeof client === "object" && array.indexOf(client) === index);
  }

  function patchStatsigModelWhitelist() {
    statsigClients().forEach((client) => {
      if (typeof client.getDynamicConfig !== "function") return;
      if (client.__codestudioLiteModelWhitelistPatched !== codexStatsigModelWhitelistPatchVersion) {
        const originalGetDynamicConfig = client.getDynamicConfig.bind(client);
        client.getDynamicConfig = (name, options) => {
          const result = originalGetDynamicConfig(name, options);
          return patchStatsigModelDynamicConfig(result);
        };
        client.__codestudioLiteModelWhitelistPatched = codexStatsigModelWhitelistPatchVersion;
      }
      try {
        patchStatsigModelDynamicConfig(client.getDynamicConfig("107580212", { disableExposureLog: true }));
      } catch {
      }
    });
  }

  function reactFiberKeys(element) {
    return Object.keys(element || {}).filter((key) => key.startsWith("__reactFiber") || key.startsWith("__reactInternalInstance") || key.startsWith("__reactProps"));
  }

  function patchReactModelStateNodes() {
    const selector = "[role='menu'], [role='dialog'], [role='listbox'], [data-radix-popper-content-wrapper]";
    return [document.body, ...document.querySelectorAll(selector)].filter(Boolean);
  }

  function patchReactModelState() {
    const visited = new WeakSet();
    let changed = false;
    for (const node of patchReactModelStateNodes().slice(0, 220)) {
      for (const key of reactFiberKeys(node)) {
        if (patchObjectGraphForModels(node[key], visited)) changed = true;
      }
    }
    return changed;
  }

  function shouldScheduleReactModelStatePatch(mutations) {
    if (!codexPlusModelUnlockEnabled() || !codexPlusModelNames().length || !mutations) return false;
    const selector = "[role='menu'], [role='dialog'], [role='listbox'], [data-radix-popper-content-wrapper]";
    return mutations.some((mutation) => [...mutation.addedNodes].some((node) => (
      node.nodeType === 1 && (!!node.matches?.(selector) || !!node.querySelector?.(selector))
    )));
  }

  function patchAppServerModelMessages() {
    if (window.__codestudioLiteModelMessagePatchInstalled === codexModelMessagePatchVersion) return;
    window.__codestudioLiteModelMessagePatchInstalled = codexModelMessagePatchVersion;
    const originalDispatchEvent = window.dispatchEvent;
    window.dispatchEvent = function patchedCodestudioLiteDispatchEvent(event) {
      try {
        const detail = event?.detail;
        const request = detail?.request;
        if (event?.type === "codex-message-from-view" && detail?.type === "mcp-request" && request?.method === "model/list") {
          request.params = { ...(request.params || {}), includeHidden: true };
          if (request.id != null) codexModelListRequestIds.add(String(request.id));
        }
        if (event?.type === "message") patchMcpModelResponseData(event.data);
      } catch (error) {
        recordCodexEnhancementDiagnostic("model_message_patch_failed", { errorMessage: String(error?.message || error) });
      }
      return originalDispatchEvent.call(this, event);
    };
    window.addEventListener("message", (event) => {
      try {
        patchMcpModelResponseData(event?.data);
      } catch {
      }
    }, true);
  }

  function patchMcpModelResponseData(data) {
    if (data?.type !== "mcp-response") return false;
    const message = data.message || data.response;
    const requestId = message?.id != null ? String(message.id) : "";
    if (codexModelListRequestIds.size > 0 && !codexModelListRequestIds.has(requestId)) return false;
    codexModelListRequestIds.delete(requestId);
    return patchModelContainer(data) || patchModelContainer(message) || patchModelContainer(message?.result) || patchModelContainer(message?.result?.data);
  }

  function appServerModelRequestMethod(method, params) {
    if (method === "send-cli-request-for-host" && params?.method) return String(params.method);
    if (method === "vscode://codex/list-plugins") return "list-plugins";
    if (method === "vscode://codex/plugin/install") return "install-plugin";
    if (method === "vscode://codex/plugin/uninstall") return "uninstall-plugin";
    if (method === "plugin/list") return "list-plugins";
    if (method === "plugin/install") return "install-plugin";
    if (method === "plugin/uninstall") return "uninstall-plugin";
    return String(method || "");
  }

  function patchAppServerModelResult(method, result) {
    if (method !== "list-models-for-host") return result;
    try {
      if (Array.isArray(result)) patchModelArray(result, true);
      if (Array.isArray(result?.data)) patchModelArray(result.data, true);
      if (Array.isArray(result?.models)) patchModelArray(result.models, true);
      patchModelContainer(result);
      patchObjectGraphForModels(result, new WeakSet(), 0);
    } catch (error) {
      recordCodexEnhancementDiagnostic("model_app_server_result_patch_failed", { errorMessage: String(error?.message || error) });
    }
    return result;
  }

  function patchAppServerModelRequestClient(client) {
    if (!client || typeof client.sendRequest !== "function") return false;
    if (client.__codestudioLiteModelRequestPatch === codexAppServerModelRequestPatchVersion) return true;
    const originalSendRequest = client.__codestudioLiteModelOriginalSendRequest || client.sendRequest.bind(client);
    client.__codestudioLiteModelOriginalSendRequest = originalSendRequest;
    client.sendRequest = async function codestudioLiteModelPatchedSendRequest(method, params, options) {
      const result = await originalSendRequest(method, params, options);
      if (!codexPlusModelUnlockEnabled()) return result;
      if (!codexPlusModelNames().length) await loadCodexModelCatalog();
      return patchAppServerModelResult(appServerModelRequestMethod(String(method || ""), params), result);
    };
    client.__codestudioLiteModelRequestPatch = codexAppServerModelRequestPatchVersion;
    return true;
  }

  function installAppServerModelRequestPatch() {
    if (window.__codestudioLiteAppServerModelRequestPatchInstalled === codexAppServerModelRequestPatchVersion) return;
    const patch = async () => {
      try {
        const module = await loadCodexAppModule("app-server-manager-signals-");
        const candidates = Object.values(module).filter((value) => value && typeof value === "object");
        let patchedCount = 0;
        for (const candidate of candidates) {
          if (patchAppServerModelRequestClient(candidate)) patchedCount += 1;
          if (typeof candidate.sendRequest !== "function" && typeof candidate.get === "function") {
            try {
              if (patchAppServerModelRequestClient(candidate.get())) patchedCount += 1;
            } catch {
            }
          }
        }
        if (patchedCount > 0) window.__codestudioLiteAppServerModelRequestPatchInstalled = codexAppServerModelRequestPatchVersion;
      } catch (error) {
        recordCodexEnhancementDiagnostic("model_app_server_request_patch_failed", { errorMessage: String(error?.message || error) });
      }
    };
    void patch();
  }

  function ensureCodexModelWhitelistInstalls() {
    if (!codexPlusModelUnlockEnabled()) return;
    installModelJsonResponsePatch();
    patchAppServerModelMessages();
    installAppServerModelRequestPatch();
  }

  function runCodexModelWhitelistRefreshPass() {
    if (!codexPlusModelUnlockEnabled() || !codexPlusModelNames().length) return false;
    let changed = false;
    try {
      patchStatsigModelWhitelist();
      if (patchReactModelState()) changed = true;
      installAppServerModelRequestPatch();
    } catch (error) {
      recordCodexEnhancementDiagnostic("model_whitelist_refresh_failed", { errorMessage: String(error?.message || error) });
    }
    return changed;
  }

  function scheduleCodexModelWhitelistRefresh(durationMs = 2500) {
    if (!codexPlusModelUnlockEnabled()) return;
    codexModelWhitelistRefreshUntil = Math.max(codexModelWhitelistRefreshUntil, Date.now() + durationMs);
    if (codexModelWhitelistRefreshTimer) return;
    const tick = () => {
      codexModelWhitelistRefreshTimer = 0;
      runCodexModelWhitelistRefreshPass();
      if (Date.now() < codexModelWhitelistRefreshUntil) {
        codexModelWhitelistRefreshTimer = window.setTimeout(tick, 120);
      }
    };
    tick();
  }

  function patchCodexModelWhitelist(mutations = null) {
    ensureCodexModelWhitelistInstalls();
    if (!codexPlusModelNames().length) {
      void loadCodexModelCatalog();
      return;
    }
    if (shouldScheduleReactModelStatePatch(mutations)) {
      scheduleCodexModelWhitelistRefresh();
    } else {
      runCodexModelWhitelistRefreshPass();
    }
  }

  function refreshCodexModelWhitelistFromScan(mutations) {
    patchCodexModelWhitelist(mutations);
  }

  function normalizeCodexServiceTierModelName(model) {
    return String(model || "").trim().toLowerCase();
  }

  function isFastServiceTierValue(value) {
    const normalized = String(value || "").trim().toLowerCase();
    return normalized === "fast" || normalized === "priority";
  }

  function codexFastServiceTierValue() {
    return codexServiceTierState.fastTierValue || codexServiceTierFallbackFastValue;
  }

  function codexServiceTierFastModelListLabel() {
    return Array.from(codexServiceTierSupportedFastModels).join(" / ");
  }

  function codexServiceTierModelFromValue(value, visited = new WeakSet(), depth = 0) {
    if (typeof value === "string") return value.trim();
    if (!value || typeof value !== "object" || visited.has(value) || depth > 3) return "";
    visited.add(value);
    for (const key of ["model", "modelId", "model_id", "selectedModel", "selected_model", "defaultModel", "default_model"]) {
      const model = codexServiceTierModelFromValue(value[key], visited, depth + 1);
      if (model) return model;
    }
    for (const key of ["params", "request", "payload", "body", "config", "options"]) {
      const model = codexServiceTierModelFromValue(value[key], visited, depth + 1);
      if (model) return model;
    }
    return "";
  }

  function codexServiceTierCurrentModelName() {
    return codexServiceTierModelFromValue(codexModelCatalog.model) || codexServiceTierModelFromValue(codexModelCatalog.default_model);
  }

  function codexServiceTierModelForRequest(params, modelHint = "") {
    return codexServiceTierModelFromValue(params) || codexServiceTierModelFromValue(modelHint) || codexServiceTierCurrentModelName();
  }

  function codexServiceTierFastSupportedForModel(modelName) {
    return codexServiceTierSupportedFastModels.has(normalizeCodexServiceTierModelName(modelName));
  }

  function codexServiceTierMaybeLoadModelCatalog(force = false) {
    if (codexModelCatalogPromise) return;
    if (!force && codexModelCatalog.status === "failed") return;
    if (!force && codexModelCatalogLoadedAt && Date.now() - codexModelCatalogLoadedAt < 10000) return;
    loadCodexModelCatalog(force).then(() => {
      refreshCodexServiceTierControls();
    }).catch(() => {
      refreshCodexServiceTierControls();
    });
  }

  function codexServiceTierFastAvailability(modelName = codexServiceTierCurrentModelName()) {
    const normalizedModel = normalizeCodexServiceTierModelName(modelName);
    return {
      modelName: modelName || "",
      supported: !!normalizedModel && codexServiceTierSupportedFastModels.has(normalizedModel),
    };
  }

  function codexServiceTierFastUnsupportedMessage(modelName = codexServiceTierCurrentModelName()) {
    const modelText = modelName ? `当前模型 ${modelName} 不支持` : "当前模型未读取";
    return `Fast 仅支持 ${codexServiceTierFastModelListLabel()}，${modelText}`;
  }

  function codexServiceTierValueForMode(mode) {
    if (mode === "fast") return codexFastServiceTierValue();
    if (mode === "standard") return null;
    return codexServiceTierState.serviceTier || null;
  }

  function codexServiceTierDefaultModeForControlMode(controlMode, fallback = "inherit") {
    if (controlMode === "global-fast") return "fast";
    if (controlMode === "global-standard") return "standard";
    if (controlMode === "inherit") return "inherit";
    return normalizeCodexThreadServiceTierMode(fallback);
  }

  function codexServiceTierEffectiveThreadMode(threadMode = "inherit", defaultMode = "inherit") {
    const normalizedThreadMode = normalizeCodexThreadServiceTierMode(threadMode);
    if (normalizedThreadMode !== "inherit") return normalizedThreadMode;
    return normalizeCodexThreadServiceTierMode(defaultMode);
  }

  function codexServiceTierValueForControlMode(controlMode, threadMode = "inherit", defaultMode = "inherit") {
    if (controlMode === "global-fast") return codexFastServiceTierValue();
    if (controlMode === "global-standard") return null;
    if (controlMode === "custom") return codexServiceTierValueForMode(codexServiceTierEffectiveThreadMode(threadMode, defaultMode));
    return codexServiceTierState.serviceTier || null;
  }

  function codexServiceTierEffectiveMode(value) {
    return isFastServiceTierValue(value) ? "fast" : "standard";
  }

  function normalizeCodexThreadServiceTierMode(mode) {
    const normalized = String(mode || "").trim().toLowerCase();
    return codexThreadServiceTierModes.has(normalized) ? normalized : "inherit";
  }

  function normalizeCodexServiceTierControlMode(mode) {
    const normalized = String(mode || "").trim().toLowerCase();
    return codexServiceTierControlModes.has(normalized) ? normalized : "inherit";
  }

  function serviceTierGlobalStatusMessage(serviceTier) {
    if (isFastServiceTierValue(serviceTier)) return "Fast 已开启";
    if (!serviceTier) return "默认服务模式";
    return `当前：${serviceTier}`;
  }

  function serviceTierStatusMessage(
    controlMode = codexServiceTierState.controlMode || "inherit",
    threadMode = codexServiceTierState.threadMode || "inherit",
    effectiveMode = codexServiceTierState.effectiveMode || "standard",
    defaultMode = codexServiceTierState.defaultMode || "inherit"
  ) {
    if (codexServiceTierState.status === "loading") return "正在读取…";
    if (codexServiceTierState.status === "failed") return "读取失败";
    if (controlMode === "inherit") return `继承 config.toml：${effectiveMode}`;
    if (controlMode === "global-standard") return "全局 Standard";
    if (controlMode === "global-fast") return "全局 Fast";
    if (threadMode === "inherit") return `自定义：默认 ${defaultMode}`;
    return `自定义：当前 thread ${threadMode}`;
  }

  function readThreadServiceTierState() {
    try {
      const parsed = JSON.parse(localStorage.getItem(codexThreadServiceTierKey) || "{}");
      const rawEntries = parsed?.version === codexThreadServiceTierVersion && parsed?.entries && typeof parsed.entries === "object"
        ? parsed.entries
        : {};
      const entries = Object.create(null);
      Object.entries(rawEntries).forEach(([key, value]) => {
        const safeKey = validThreadScrollSessionKey(key);
        const mode = normalizeCodexThreadServiceTierMode(value?.mode);
        if (safeKey && mode !== "inherit") entries[safeKey] = { mode, at: finiteNonNegativeNumber(value?.at) || Date.now() };
      });
      const draft = normalizeThreadServiceTierDraft(parsed?.draft);
      const hasCustomState = !!draft || Object.keys(entries).length > 0;
      const mode = parsed?.mode ? normalizeCodexServiceTierControlMode(parsed.mode) : (hasCustomState ? "custom" : "inherit");
      return {
        mode,
        defaultMode: normalizeCodexThreadServiceTierMode(parsed?.defaultMode || codexServiceTierDefaultModeForControlMode(mode)),
        entries,
        draft,
      };
    } catch (_) {
      return { mode: "inherit", defaultMode: "inherit", entries: Object.create(null), draft: null };
    }
  }

  function writeThreadServiceTierState(state) {
    const mode = normalizeCodexServiceTierControlMode(state?.mode);
    const defaultMode = normalizeCodexThreadServiceTierMode(state?.defaultMode || codexServiceTierDefaultModeForControlMode(mode));
    const rawEntries = state?.entries && typeof state.entries === "object" ? state.entries : {};
    const entries = Object.create(null);
    Object.entries(rawEntries)
      .map(([key, value]) => {
        const safeKey = validThreadScrollSessionKey(key);
        const mode = normalizeCodexThreadServiceTierMode(value?.mode);
        return safeKey && mode !== "inherit" ? [safeKey, { mode, at: finiteNonNegativeNumber(value?.at) || Date.now() }] : null;
      })
      .filter(Boolean)
      .sort((left, right) => right[1].at - left[1].at)
      .slice(0, codexThreadServiceTierMaxEntries)
      .forEach(([key, value]) => {
        entries[key] = value;
      });
    const draft = normalizeThreadServiceTierDraft(state?.draft);
    try {
      localStorage.setItem(codexThreadServiceTierKey, JSON.stringify({
        version: codexThreadServiceTierVersion,
        mode,
        defaultMode,
        entries,
        ...(draft ? { draft } : {}),
      }));
    } catch (_) {}
  }

  function normalizeThreadServiceTierDraft(value) {
    if (!value || typeof value !== "object") return null;
    const mode = normalizeCodexThreadServiceTierMode(value.mode);
    if (mode === "inherit") return null;
    const at = finiteNonNegativeNumber(value.at) || Date.now();
    return { mode, at };
  }

  function codexThreadServiceTierOverride(threadId) {
    const key = validThreadScrollSessionKey(threadId);
    if (!key) return null;
    const entry = readThreadServiceTierState().entries[key];
    const mode = normalizeCodexThreadServiceTierMode(entry?.mode);
    return mode === "inherit" ? null : { mode, at: finiteNonNegativeNumber(entry?.at) || 0 };
  }

  function codexThreadServiceTierDraft() {
    const draft = readThreadServiceTierState().draft;
    if (!draft) return null;
    if (Date.now() - draft.at > codexThreadServiceTierDraftBindWindowMs) return null;
    return draft;
  }

  function setCodexThreadServiceTierOverride(threadId, mode) {
    const normalizedMode = normalizeCodexThreadServiceTierMode(mode);
    const state = readThreadServiceTierState();
    state.mode = "custom";
    const key = validThreadScrollSessionKey(threadId);
    if (key) {
      if (normalizedMode === "inherit") {
        delete state.entries[key];
      } else {
        state.entries[key] = { mode: normalizedMode, at: Date.now() };
      }
    } else if (normalizedMode === "inherit") {
      state.draft = null;
    } else {
      state.draft = { mode: normalizedMode, at: Date.now() };
    }
    writeThreadServiceTierState(state);
  }

  function bindDraftServiceTierToThread(threadId) {
    const key = validThreadScrollSessionKey(threadId);
    const draft = codexThreadServiceTierDraft();
    if (!key || !draft) return false;
    const state = readThreadServiceTierState();
    if (normalizeCodexServiceTierControlMode(state.mode) !== "custom") {
      state.draft = null;
      writeThreadServiceTierState(state);
      return false;
    }
    if (!state.entries[key]) state.entries[key] = { mode: draft.mode, at: Date.now() };
    state.draft = null;
    writeThreadServiceTierState(state);
    return true;
  }

  function setCodexServiceTierControlMode(mode) {
    const normalizedMode = normalizeCodexServiceTierControlMode(mode);
    if (normalizedMode === "global-fast") {
      const fastAvailability = codexServiceTierFastAvailability();
      if (!fastAvailability.supported) {
        codexServiceTierMaybeLoadModelCatalog(true);
        showToast(codexServiceTierFastUnsupportedMessage(fastAvailability.modelName));
        refreshCodexServiceTierControls();
        return;
      }
    }
    const state = readThreadServiceTierState();
    state.mode = normalizedMode;
    if (normalizedMode !== "custom") {
      state.defaultMode = codexServiceTierDefaultModeForControlMode(normalizedMode);
      state.entries = Object.create(null);
      state.draft = null;
    } else {
      state.defaultMode = normalizeCodexThreadServiceTierMode(state.defaultMode);
    }
    writeThreadServiceTierState(state);
    refreshCodexServiceTierControls();
    const labels = {
      inherit: "继承 config.toml",
      "global-standard": "全局 Standard",
      "global-fast": "全局 Fast",
      custom: "自定义",
    };
    showToast(`服务模式：${labels[normalizedMode] || normalizedMode}`);
  }

  function syncCodexServiceTierEffectiveState() {
    if (!codestudioLiteSettings().serviceTierControls) {
      codexServiceTierState = {
        ...codexServiceTierState,
        activeThreadId: "",
        threadMode: "inherit",
        effectiveServiceTier: codexServiceTierState.serviceTier || null,
        effectiveMode: codexServiceTierEffectiveMode(codexServiceTierState.serviceTier),
        message: "未启用",
      };
      return;
    }
    const activeThreadId = validThreadScrollSessionKey(currentSessionRef().session_id);
    if (activeThreadId) bindDraftServiceTierToThread(activeThreadId);
    const storedState = readThreadServiceTierState();
    const controlMode = normalizeCodexServiceTierControlMode(storedState.mode);
    const defaultMode = normalizeCodexThreadServiceTierMode(storedState.defaultMode);
    const override = activeThreadId ? codexThreadServiceTierOverride(activeThreadId) : codexThreadServiceTierDraft();
    const threadMode = normalizeCodexThreadServiceTierMode(override?.mode);
    const effectiveServiceTier = codexServiceTierValueForControlMode(controlMode, threadMode, defaultMode);
    const effectiveMode = codexServiceTierEffectiveMode(effectiveServiceTier);
    const fastAvailability = codexServiceTierFastAvailability();
    const message = effectiveMode === "fast" && !fastAvailability.supported
      ? codexServiceTierFastUnsupportedMessage(fastAvailability.modelName)
      : serviceTierStatusMessage(controlMode, threadMode, effectiveMode, defaultMode);
    codexServiceTierState = {
      ...codexServiceTierState,
      controlMode,
      defaultMode,
      activeThreadId,
      threadMode,
      effectiveServiceTier,
      effectiveMode,
      fastModelName: fastAvailability.modelName,
      fastSupported: fastAvailability.supported,
      message,
    };
  }

  function codexServiceTierBadgeState() {
    if (codexServiceTierState.status === "loading") return { tier: "loading", label: "...", disabled: true, title: "服务模式：正在读取" };
    if (codexServiceTierState.status === "failed") return { tier: "failed", label: "?", title: "服务模式：读取失败" };
    const fastAvailability = codexServiceTierFastAvailability();
    const effectiveMode = codexServiceTierState.effectiveMode || "standard";
    const scope = codexServiceTierState.controlMode === "custom" && codexServiceTierState.threadMode !== "inherit"
      ? `当前 thread：${codexServiceTierState.threadMode}`
      : serviceTierStatusMessage(codexServiceTierState.controlMode, codexServiceTierState.threadMode, effectiveMode, codexServiceTierState.defaultMode);
    const title = [
      `服务模式：${scope}`,
      "Standard：使用标准处理；不在请求上设置 priority。",
      `Fast：仅支持 ${codexServiceTierFastModelListLabel()}；对支持模型使用 service_tier=\"priority\"，官方说明其延迟更低且更一致，但会按更高价格计费；rate limit 与 Standard 共享，流量快速上涨时可能回落到 Standard。`,
    ].join("\n");
    if (effectiveMode === "fast" && !fastAvailability.supported) {
      return { tier: "unsupported", label: "不支持", title: `${title}\n${codexServiceTierFastUnsupportedMessage(fastAvailability.modelName)}；当前请求会按 Standard 发送。` };
    }
    if (effectiveMode === "fast") return { tier: "fast", label: "fast", title };
    return { tier: "standard", label: "standard", title };
  }

  function refreshCodexServiceTierBadges() {
    const state = codexServiceTierBadgeState();
    document.querySelectorAll(`[data-codex-service-tier-badge="true"]`).forEach((node) => {
      setCodestudioLiteDataset(node, "tier", state.tier);
      setCodestudioLiteDataset(node, "disabled", String(!!state.disabled));
      setCodestudioLiteText(node, state.label);
      setCodestudioLiteProperty(node, "title", state.title);
      setCodestudioLiteAttribute(node, "aria-label", state.title);
    });
  }

  function refreshCodexServiceTierControls() {
    syncCodexServiceTierEffectiveState();
    if (codestudioLiteSettings().serviceTierControls) codexServiceTierMaybeLoadModelCatalog();
    const fastAvailability = codexServiceTierFastAvailability();
    const fastDisabled = !codestudioLiteSettings().serviceTierControls || codexServiceTierState.status === "loading" || !fastAvailability.supported;
    const fastTitle = fastAvailability.supported
      ? "Fast：使用 service_tier=\"priority\""
      : codexServiceTierFastUnsupportedMessage(fastAvailability.modelName);
    const fastUnsupportedActive = codexServiceTierState.effectiveMode === "fast" && !fastAvailability.supported;
    document.querySelectorAll("[data-codex-service-tier-controls]").forEach((node) => {
      setCodestudioLiteBooleanProperty(node, "hidden", !codestudioLiteSettings().serviceTierControls);
    });
    document.querySelectorAll("[data-codex-service-tier-status]").forEach((node) => {
      setCodestudioLiteDataset(node, "status", fastUnsupportedActive ? "unsupported" : (codexServiceTierState.status || "loading"));
      setCodestudioLiteText(node, codexServiceTierState.message || "未读取");
    });
    document.querySelectorAll("[data-codex-service-tier-inherit]").forEach((button) => {
      setCodestudioLiteBooleanProperty(button, "disabled", !codestudioLiteSettings().serviceTierControls || codexServiceTierState.status === "loading");
      setCodestudioLiteDataset(button, "active", String(codexServiceTierState.controlMode === "inherit"));
    });
    document.querySelectorAll("[data-codex-service-tier-standard]").forEach((button) => {
      setCodestudioLiteBooleanProperty(button, "disabled", !codestudioLiteSettings().serviceTierControls || codexServiceTierState.status === "loading");
      setCodestudioLiteDataset(button, "active", String(codexServiceTierState.controlMode === "global-standard"));
    });
    document.querySelectorAll("[data-codex-service-tier-fast]").forEach((button) => {
      setCodestudioLiteBooleanProperty(button, "disabled", fastDisabled);
      setCodestudioLiteDataset(button, "active", String(codexServiceTierState.controlMode === "global-fast"));
      setCodestudioLiteProperty(button, "title", fastTitle);
    });
    document.querySelectorAll("[data-codex-service-tier-custom]").forEach((button) => {
      setCodestudioLiteBooleanProperty(button, "disabled", !codestudioLiteSettings().serviceTierControls || codexServiceTierState.status === "loading");
      setCodestudioLiteDataset(button, "active", String(codexServiceTierState.controlMode === "custom"));
    });
    document.querySelectorAll("[data-codex-service-tier-thread-inherit]").forEach((button) => {
      setCodestudioLiteBooleanProperty(button, "disabled", !codestudioLiteSettings().serviceTierControls || codexServiceTierState.status === "loading");
      setCodestudioLiteDataset(button, "active", String(codexServiceTierState.controlMode === "custom" && codexServiceTierState.threadMode === "inherit"));
      setCodestudioLiteProperty(button, "title", `当前 thread 不单独覆盖，继承自定义默认 ${codexServiceTierState.defaultMode || "inherit"}`);
    });
    document.querySelectorAll("[data-codex-service-tier-thread-standard]").forEach((button) => {
      setCodestudioLiteBooleanProperty(button, "disabled", !codestudioLiteSettings().serviceTierControls || codexServiceTierState.status === "loading");
      setCodestudioLiteDataset(button, "active", String(codexServiceTierState.controlMode === "custom" && codexServiceTierState.threadMode === "standard"));
    });
    document.querySelectorAll("[data-codex-service-tier-thread-fast]").forEach((button) => {
      setCodestudioLiteBooleanProperty(button, "disabled", fastDisabled);
      setCodestudioLiteDataset(button, "active", String(codexServiceTierState.controlMode === "custom" && codexServiceTierState.threadMode === "fast"));
      setCodestudioLiteProperty(button, "title", fastTitle);
    });
    refreshCodexServiceTierBadges();
  }

  async function loadCodexServiceTierState() {
    if (!codestudioLiteSettings().serviceTierControls) {
      codexServiceTierState = { ...codexServiceTierState, status: "idle", message: "未启用" };
      refreshCodexServiceTierControls();
      return;
    }
    codexServiceTierState = { ...codexServiceTierState, status: "loading", message: "正在读取…" };
    refreshCodexServiceTierControls();
    try {
      const serviceTier = await getCodexServiceTierSetting();
      codexServiceTierState = {
        ...codexServiceTierState,
        status: "ok",
        serviceTier,
        message: serviceTierGlobalStatusMessage(serviceTier),
      };
    } catch (error) {
      codexServiceTierState = {
        ...codexServiceTierState,
        status: "failed",
        message: "读取失败",
      };
      recordCodexEnhancementDiagnostic("service_tier_read_failed", { errorMessage: String(error?.message || error) });
    } finally {
      refreshCodexServiceTierControls();
    }
  }

  function ensureCodexServiceTierStateLoaded() {
    if (!codestudioLiteSettings().serviceTierControls) {
      codexServiceTierStateLoadStarted = false;
      return;
    }
    if (codexServiceTierStateLoadStarted) return;
    codexServiceTierStateLoadStarted = true;
    void loadCodexServiceTierState();
  }

  function setCodexThreadServiceTierMode(mode) {
    const normalizedMode = normalizeCodexThreadServiceTierMode(mode);
    if (normalizedMode === "fast") {
      const fastAvailability = codexServiceTierFastAvailability();
      if (!fastAvailability.supported) {
        codexServiceTierMaybeLoadModelCatalog(true);
        showToast(codexServiceTierFastUnsupportedMessage(fastAvailability.modelName));
        refreshCodexServiceTierControls();
        return;
      }
    }
    const threadId = validThreadScrollSessionKey(currentSessionRef().session_id);
    setCodexThreadServiceTierOverride(threadId, normalizedMode);
    refreshCodexServiceTierControls();
    const target = threadId ? "当前 thread" : "新 thread 草稿";
    showToast(`${target}服务模式：${normalizedMode === "inherit" ? "继承" : normalizedMode}`);
  }

  function toggleCodexServiceTierFromBadge() {
    syncCodexServiceTierEffectiveState();
    const nextMode = codexServiceTierState.effectiveMode === "fast" ? "standard" : "fast";
    if (nextMode === "fast") {
      const fastAvailability = codexServiceTierFastAvailability();
      if (!fastAvailability.supported) {
        codexServiceTierMaybeLoadModelCatalog(true);
        showToast(codexServiceTierFastUnsupportedMessage(fastAvailability.modelName));
        refreshCodexServiceTierControls();
        return;
      }
    }
    setCodexThreadServiceTierMode(nextMode);
  }

  function codexServiceTierBadgeVisibleElement(element) {
    if (!(element instanceof HTMLElement) || !element.isConnected) return false;
    const style = getComputedStyle(element);
    if (style.display === "none" || style.visibility === "hidden") return false;
    const rect = element.getBoundingClientRect();
    return rect.width > 0 && rect.height > 0;
  }

  function codexServiceTierBadgeText(element) {
    const text = String(element?.textContent || "");
    return (text.length > 4000 ? text.slice(-4000) : text).replace(/\s+/g, " ").trim();
  }

  function codexServiceTierKnownProviderNames() {
    return uniqueValues([
      codexModelCatalog.provider_name,
      codexModelCatalog.model_provider,
    ]).map((value) => value.toLowerCase());
  }

  function codexServiceTierLooksLikeProviderButton(button, providerNames) {
    const text = codexServiceTierBadgeText(button);
    if (!text || text.length > 32) return false;
    const lower = text.toLowerCase();
    if (providerNames.includes(lower)) return true;
    if (/\s/.test(text)) return false;
    if (!/[a-z]/i.test(text)) return false;
    if (!/^[a-z0-9][a-z0-9._-]{1,31}$/i.test(text)) return false;
    if (/^(local|remote|cloud|standard|default|fast|worktree|new|send|stop|codex)$/i.test(text)) return false;
    if (/^(gpt|o[1-9]|claude|gemini|deepseek|qwen|kimi|moonshot|mistral|llama|sonnet|opus|haiku)[a-z0-9._-]*$/i.test(text)) return false;
    return true;
  }

  function codexServiceTierBadgeButtonCandidates(composer) {
    const composerRect = composer.getBoundingClientRect();
    return Array.from(composer.querySelectorAll("button, [role='button']"))
      .filter((button) => !button.closest?.(`[data-codex-service-tier-badge="true"]`))
      .filter(codexServiceTierBadgeVisibleElement)
      .filter((button) => {
        const rect = button.getBoundingClientRect();
        return rect.bottom >= composerRect.top + composerRect.height * 0.35;
      })
      .sort((left, right) => {
        const leftRect = left.getBoundingClientRect();
        const rightRect = right.getBoundingClientRect();
        return (rightRect.bottom - leftRect.bottom) || (leftRect.left - rightRect.left);
      });
  }

  function codexServiceTierVisibleComposerFooters(root = document) {
    const footers = [
      ...(root?.matches?.(".composer-footer") ? [root] : []),
      ...Array.from(root?.querySelectorAll?.(".composer-footer") || []),
    ];
    return footers
      .filter(codexServiceTierBadgeVisibleElement)
      .sort((left, right) => {
        const leftRect = left.getBoundingClientRect();
        const rightRect = right.getBoundingClientRect();
        return (rightRect.bottom - leftRect.bottom) || (rightRect.width - leftRect.width);
      });
  }

  function codexServiceTierComposerScore(composer) {
    const text = codexServiceTierBadgeText(composer).toLowerCase();
    const providerNames = codexServiceTierKnownProviderNames();
    let score = 0;
    if (providerNames.some((name) => name && text.includes(name))) score += 40;
    if (/完全访问权限|full access|model|超高|high|sub2api|provider/i.test(text)) score += 20;
    if (/本地模式|local mode|worktree|branch|codex\//i.test(text)) score -= 30;
    if (composer.matches?.(".composer-footer")) score += 4;
    if (composer.querySelector?.(".composer-footer")) score += 8;
    const buttons = Array.from(composer.querySelectorAll?.("button, [role='button']") || []).filter(codexServiceTierBadgeVisibleElement);
    if (buttons.some((button) => codexServiceTierLooksLikeProviderButton(button, providerNames))) score += 30;
    score += Math.min(10, buttons.length);
    return score;
  }

  function codexServiceTierComposerCandidates() {
    const candidates = new Set();
    codexServiceTierVisibleComposerFooters().forEach((footer) => {
      candidates.add(footer);
      let node = footer.parentElement;
      for (let depth = 0; node instanceof HTMLElement && depth < 6; depth += 1, node = node.parentElement) {
        if (codexServiceTierBadgeVisibleElement(node)) candidates.add(node);
      }
    });
    Array.from(document.querySelectorAll("form, textarea, [role='textbox'], [contenteditable='true']"))
      .filter(codexServiceTierBadgeVisibleElement)
      .forEach((node) => {
        candidates.add(node);
        let parent = node.parentElement;
        for (let depth = 0; parent instanceof HTMLElement && depth < 4; depth += 1, parent = parent.parentElement) {
          if (codexServiceTierBadgeVisibleElement(parent)) candidates.add(parent);
        }
      });
    if (!candidates.size) {
      Array.from(document.querySelectorAll("main"))
        .filter(codexServiceTierBadgeVisibleElement)
        .slice(-2)
        .forEach((node) => candidates.add(node));
    }
    return Array.from(candidates);
  }

  function codexServiceTierBestComposerFooter(root = document) {
    return codexServiceTierVisibleComposerFooters(root)
      .map((footer, index) => ({ footer, index, score: codexServiceTierComposerScore(footer) }))
      .sort((left, right) => (right.score - left.score) || (left.index - right.index))[0]?.footer || null;
  }

  function codexServiceTierFindComposerEl() {
    const now = Date.now();
    if (codexServiceTierComposerCache.element?.isConnected && now < codexServiceTierComposerCache.expiresAt) {
      return codexServiceTierComposerCache.element;
    }
    const composer = codexServiceTierComposerCandidates()
      .map((composer, index) => ({ composer, index, score: codexServiceTierComposerScore(composer) }))
      .sort((left, right) => (right.score - left.score) || (left.index - right.index))[0]?.composer || null;
    codexServiceTierComposerCache = { element: composer, expiresAt: composer ? now + 1500 : 0 };
    return composer;
  }

  function codexServiceTierBadgeAnchor(composer) {
    const providerNames = codexServiceTierKnownProviderNames();
    const buttons = codexServiceTierBadgeButtonCandidates(composer);
    const exact = buttons.find((button) => providerNames.includes(codexServiceTierBadgeText(button).toLowerCase()));
    if (exact) return exact;
    const composerRect = composer.getBoundingClientRect();
    return buttons.find((button) => {
      const rect = button.getBoundingClientRect();
      return rect.left >= composerRect.left + composerRect.width * 0.42 && codexServiceTierLooksLikeProviderButton(button, providerNames);
    }) || null;
  }

  function codexServiceTierComposerFooter(composer) {
    if (composer?.matches?.(".composer-footer")) return composer;
    return codexServiceTierBestComposerFooter(composer) || codexServiceTierBestComposerFooter() || null;
  }

  function codexServiceTierBadgeFooterGroup(composer) {
    const footer = codexServiceTierComposerFooter(composer);
    if (!footer) return null;
    const children = Array.from(footer.children).filter(codexServiceTierBadgeVisibleElement);
    if (!children.length) return footer;
    const providerNames = codexServiceTierKnownProviderNames();
    const providerGroup = children.find((child) => {
      const text = codexServiceTierBadgeText(child).toLowerCase();
      return providerNames.some((name) => name && text.includes(name));
    });
    return providerGroup || children[children.length - 1] || footer;
  }

  function codexServiceTierBadgePlacement(composer) {
    const anchor = composer ? codexServiceTierBadgeAnchor(composer) : null;
    if (anchor?.parentElement) return { parent: anchor.parentElement, before: anchor };
    const group = composer ? codexServiceTierBadgeFooterGroup(composer) : null;
    if (group) return { parent: group, before: group.firstChild };
    return null;
  }

  function wireCodexServiceTierBadge(badge) {
    if (!badge || badge.dataset.codexServiceTierBadgeWired === codexServiceTierBadgeVersion) return;
    badge.dataset.codexServiceTierBadgeWired = codexServiceTierBadgeVersion;
    badge.setAttribute("role", "button");
    badge.setAttribute("tabindex", "0");
    badge.addEventListener("click", (event) => {
      event.preventDefault();
      event.stopPropagation();
      if (codexServiceTierState.status === "loading") return;
      toggleCodexServiceTierFromBadge();
    });
    badge.addEventListener("keydown", (event) => {
      if (event.key !== "Enter" && event.key !== " ") return;
      event.preventDefault();
      event.stopPropagation();
      if (codexServiceTierState.status === "loading") return;
      toggleCodexServiceTierFromBadge();
    });
  }

  function installCodexServiceTierBadge() {
    if (!codestudioLiteSettings().serviceTierControls) {
      removeCodexServiceTierBadges();
      return;
    }
    const composer = codexServiceTierFindComposerEl();
    const placement = composer ? codexServiceTierBadgePlacement(composer) : null;
    const existingBadges = Array.from(document.querySelectorAll(`[data-codex-service-tier-badge="true"]`));
    if (!composer || !placement?.parent) {
      existingBadges.forEach((badge) => badge.remove());
      return;
    }
    let badge = existingBadges.find((node) => node.closest?.(".composer-footer") || node.closest?.("button") == null) || existingBadges[0];
    existingBadges.forEach((node) => {
      if (node !== badge) node.remove();
    });
    if (!badge || badge.dataset.codexServiceTierBadgeVersion !== codexServiceTierBadgeVersion) {
      badge?.remove();
      badge = document.createElement("span");
      badge.className = codexServiceTierBadgeClass;
      badge.dataset.codexServiceTierBadge = "true";
      badge.dataset.codexServiceTierBadgeVersion = codexServiceTierBadgeVersion;
    }
    wireCodexServiceTierBadge(badge);
    const before = placement.before?.parentElement === placement.parent ? placement.before : null;
    if (badge.parentElement !== placement.parent || badge.nextSibling !== before) {
      placement.parent.insertBefore(badge, before);
    }
    refreshCodexServiceTierBadges();
  }

  function removeCodexServiceTierBadges() {
    document.querySelectorAll(`[data-codex-service-tier-badge="true"]`).forEach((badge) => badge.remove());
  }

  function codexServiceTierRequestMethods() {
    return new Set(["thread/start", "thread/resume", "turn/start"]);
  }

  function codexServiceTierThreadIdForRequest(method, params, threadIdHint = "") {
    if (method === "thread/start") return validThreadScrollSessionKey(params?.threadId || threadIdHint);
    return validThreadScrollSessionKey(params?.threadId || params?.conversationId || threadIdHint || currentSessionRef().session_id);
  }

  function codexServiceTierOverrideResult(method, params, threadIdHint, mode, requestedServiceTier, modelHint = "") {
    const threadId = codexServiceTierThreadIdForRequest(method, params, threadIdHint);
    const requestedFast = isFastServiceTierValue(requestedServiceTier);
    const modelName = codexServiceTierModelForRequest(params, modelHint);
    const fastSupported = !requestedFast || codexServiceTierFastSupportedForModel(modelName);
    return {
      threadId,
      mode,
      serviceTier: requestedFast && fastSupported ? codexFastServiceTierValue() : null,
      requestedServiceTier: requestedServiceTier || null,
      modelName,
      fastSupported,
      fastBlocked: requestedFast && !fastSupported,
    };
  }

  function codexServiceTierOverrideForRequest(method, params, threadIdHint = "") {
    if (!codestudioLiteSettings().serviceTierControls) return null;
    if (!codexServiceTierRequestMethods().has(method) || !params || typeof params !== "object") return null;
    const state = readThreadServiceTierState();
    const controlMode = normalizeCodexServiceTierControlMode(state.mode);
    const defaultMode = normalizeCodexThreadServiceTierMode(state.defaultMode);
    if (controlMode === "inherit") {
      const inheritedServiceTier = params.serviceTier ?? params.service_tier ?? codexServiceTierState.serviceTier;
      const override = codexServiceTierOverrideResult(method, params, threadIdHint, "inherit", inheritedServiceTier);
      return override.fastBlocked ? override : null;
    }
    if (controlMode === "global-standard" || controlMode === "global-fast") {
      return codexServiceTierOverrideResult(
        method,
        params,
        threadIdHint,
        controlMode,
        controlMode === "global-fast" ? codexFastServiceTierValue() : null
      );
    }
    const threadId = codexServiceTierThreadIdForRequest(method, params, threadIdHint);
    const override = threadId ? codexThreadServiceTierOverride(threadId) : codexThreadServiceTierDraft();
    const mode = codexServiceTierEffectiveThreadMode(override?.mode, defaultMode);
    if (mode === "inherit") {
      const inheritedServiceTier = params.serviceTier ?? params.service_tier ?? codexServiceTierState.serviceTier;
      const inheritedOverride = codexServiceTierOverrideResult(method, params, threadIdHint, "inherit", inheritedServiceTier);
      return inheritedOverride.fastBlocked ? { ...inheritedOverride, threadId, mode } : null;
    }
    return {
      ...codexServiceTierOverrideResult(method, params, threadIdHint, mode, mode === "fast" ? codexFastServiceTierValue() : null),
      threadId,
      mode,
    };
  }

  function applyCodexServiceTierRequestOverride(method, params, threadIdHint = "") {
    const override = codexServiceTierOverrideForRequest(method, params, threadIdHint);
    if (!override) return params;
    const nextParams = { ...(params || {}), serviceTier: override.serviceTier };
    if (Object.prototype.hasOwnProperty.call(nextParams, "service_tier") || override.fastBlocked) {
      nextParams.service_tier = override.serviceTier;
    }
    recordCodexEnhancementDiagnostic("service_tier_request_override_applied", {
      method,
      threadId: override.threadId || "",
      mode: override.mode,
      serviceTier: override.serviceTier || "standard",
      model: override.modelName || "",
      fastSupported: override.fastSupported !== false,
      fastBlocked: !!override.fastBlocked,
    });
    return nextParams;
  }

  function codexServiceTierRequestOverride(message) {
    if (!codestudioLiteSettings().serviceTierControls || !message || typeof message !== "object") return message;
    if (message.type === "send-cli-request-for-host") {
      const method = String(message.method || "");
      const params = applyCodexServiceTierRequestOverride(method, message.params);
      return params === message.params ? message : { ...message, params };
    }
    if (message.type === "mcp-request" && message.request && typeof message.request === "object") {
      const method = String(message.request.method || "");
      const params = applyCodexServiceTierRequestOverride(method, message.request.params);
      if (params === message.request.params) return message;
      return { ...message, request: { ...message.request, params } };
    }
    if (message.type === "worker-request" && message.request && typeof message.request === "object") {
      const method = String(message.request.method || "");
      const params = applyCodexServiceTierRequestOverride(method, message.request.params);
      if (params === message.request.params) return message;
      return { ...message, request: { ...message.request, params } };
    }
    if (message.type === "thread-prewarm-start" && message.request && typeof message.request === "object") {
      const params = applyCodexServiceTierRequestOverride("thread/start", message.request.params);
      if (params === message.request.params) return message;
      return { ...message, request: { ...message.request, params } };
    }
    if (message.type === "start-conversation") {
      const nextMessage = applyCodexServiceTierRequestOverride("thread/start", message);
      return nextMessage === message ? message : nextMessage;
    }
    if (message.type === "prewarm-thread-start-for-host" && message.params && typeof message.params === "object") {
      const params = applyCodexServiceTierRequestOverride("thread/start", message.params);
      return params === message.params ? message : { ...message, params };
    }
    if (message.type === "start-thread-for-host") {
      const params = applyCodexServiceTierRequestOverride("thread/start", message);
      return params === message ? message : params;
    }
    if (message.type === "start-turn-for-host" && message.params && typeof message.params === "object") {
      const params = applyCodexServiceTierRequestOverride("turn/start", message.params, message.conversationId);
      return params === message.params ? message : { ...message, params };
    }
    return message;
  }

  function installCodexServiceTierDispatcherPatch() {
    if (window.__codestudioLiteServiceTierRequestOverrideInstalled === codexServiceTierRequestOverrideVersion) return;
    const patch = async () => {
      try {
        const module = await loadCodexAppModule("setting-storage-");
        const dispatcherClass = typeof module.v === "function" && String(module.v).includes("dispatchMessage") ? module.v : null;
        const dispatcher = dispatcherClass?.getInstance?.();
        if (!dispatcher || typeof dispatcher.dispatchMessage !== "function") throw new Error("Codex dispatcher unavailable");
        if (dispatcher.__codestudioLiteServiceTierOriginalDispatchMessage) {
          window.__codestudioLiteServiceTierRequestOverrideInstalled = codexServiceTierRequestOverrideVersion;
          return;
        }
        dispatcher.__codestudioLiteServiceTierOriginalDispatchMessage = dispatcher.dispatchMessage.bind(dispatcher);
        dispatcher.dispatchMessage = (type, payload) => {
          const message = codexServiceTierRequestOverride({ ...(payload || {}), type });
          const nextType = message?.type || type;
          const { type: _type, ...nextPayload } = message || {};
          return dispatcher.__codestudioLiteServiceTierOriginalDispatchMessage(nextType, nextPayload);
        };
        window.__codestudioLiteServiceTierRequestOverrideInstalled = codexServiceTierRequestOverrideVersion;
      } catch (error) {
        recordCodexEnhancementDiagnostic("service_tier_dispatcher_patch_failed", { errorMessage: String(error?.message || error) });
      }
    };
    void patch();
  }

  function codestudioLiteOwnedMutationNode(node) {
    if (!node) return false;
    const element = node.nodeType === Node.ELEMENT_NODE ? node : node.parentElement;
    return !!element?.closest?.(`[data-codex-service-tier-badge="true"], .codestudio-lite-codex-toast, #${styleId}`);
  }

  function codestudioLiteMutationTouchesOnlyOwnNodes(mutation) {
    const changedNodes = [...(mutation.addedNodes || []), ...(mutation.removedNodes || [])];
    if (changedNodes.length > 0) {
      return codestudioLiteOwnedMutationNode(mutation.target) || changedNodes.every(codestudioLiteOwnedMutationNode);
    }
    return codestudioLiteOwnedMutationNode(mutation.target);
  }

  function shouldIgnoreCodestudioLiteMutations(mutations) {
    return Array.isArray(mutations) && mutations.length > 0 && mutations.every(codestudioLiteMutationTouchesOnlyOwnNodes);
  }

  function refresh(mutations = null) {
    ensureStyle();
    const settings = codestudioLiteSettings();
    if (settings.pluginMarketplaceUnlock) {
      installPluginBuildFlavorFilterPatch();
      installPluginMarketplaceRequestPatch();
    }
    if (settings.pluginAutoExpand) {
      schedulePluginAutoExpand();
    } else {
      clearTimeout(window.__codexPluginAutoExpandTimer);
      window.__codexPluginAutoExpandTimer = null;
      window.__codexPluginAutoExpandRunning = false;
    }
    if (settings.modelWhitelistUnlock) {
      patchCodexModelWhitelist(mutations);
    }
    if (settings.serviceTierControls) {
      ensureCodexServiceTierStateLoaded();
      installCodexServiceTierDispatcherPatch();
      installCodexServiceTierBadge();
      refreshCodexServiceTierControls();
    } else {
      codexServiceTierStateLoadStarted = false;
      removeCodexServiceTierBadges();
    }
  }

  function runCodestudioLiteRefresh(mutations = null) {
    const now = Date.now();
    if (now < codestudioLiteRefreshDisabledUntil) return;
    const started = typeof performance !== "undefined" && performance.now ? performance.now() : now;
    try {
      refresh(mutations);
    } finally {
      const ended = typeof performance !== "undefined" && performance.now ? performance.now() : Date.now();
      const elapsed = ended - started;
      if (elapsed > 50) {
        codestudioLiteSlowRefreshCount += 1;
        if (codestudioLiteSlowRefreshCount === 1 || codestudioLiteSlowRefreshCount === 3) {
          recordCodexEnhancementDiagnostic("enhancement_refresh_slow", { elapsedMs: Math.round(elapsed), count: codestudioLiteSlowRefreshCount });
        }
        if (codestudioLiteSlowRefreshCount >= 5) {
          codestudioLiteRefreshDisabledUntil = Date.now() + 5000;
          codestudioLiteSlowRefreshCount = 0;
          recordCodexEnhancementDiagnostic("enhancement_refresh_temporarily_throttled", { disabledMs: 5000 });
        }
      } else {
        codestudioLiteSlowRefreshCount = 0;
      }
    }
  }

  function scheduleCodestudioLiteRefresh(mutations = null) {
    if (shouldIgnoreCodestudioLiteMutations(mutations)) return;
    if (Array.isArray(mutations) && mutations.length > 0) {
      codestudioLitePendingMutations = [...(codestudioLitePendingMutations || []), ...mutations].slice(-80);
    }
    if (codestudioLiteRefreshScheduled) return;
    codestudioLiteRefreshScheduled = true;
    const scheduleFrame = window.requestAnimationFrame || ((callback) => window.setTimeout(callback, 16));
    scheduleFrame(() => {
      codestudioLiteRefreshScheduled = false;
      const pending = codestudioLitePendingMutations;
      codestudioLitePendingMutations = null;
      runCodestudioLiteRefresh(pending);
    });
  }

  window.__codestudioLiteCodexEnhancementsRefresh = () => scheduleCodestudioLiteRefresh();
  runCodestudioLiteRefresh();
  if (!window.__codestudioLiteCodexEnhancementsTimer) {
    window.__codestudioLiteCodexEnhancementsTimer = setInterval(() => scheduleCodestudioLiteRefresh(), 1000);
  }
  if (!window.__codestudioLiteCodexEnhancementsObserver) {
    const observer = new MutationObserver((mutations) => scheduleCodestudioLiteRefresh(mutations));
    observer.observe(document.documentElement, { childList: true, subtree: true, attributes: true, attributeFilter: ["disabled", "aria-disabled", "class", "style"] });
    window.__codestudioLiteCodexEnhancementsObserver = observer;
  }
  return true;
})()
