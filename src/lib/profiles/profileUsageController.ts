import { writable, type Readable } from "svelte/store";
import type {
  ProfileDraft,
  UsageQueryResult,
  UsageScriptSaveRequest,
  UsageScriptState,
  UsageScriptTemplateType
} from "../../types";
import { canonicalProfileToolId } from "./catalog.js";
import { providerIsOfficial } from "./presentation.js";

export type ProfileUsageStatus =
  | "closed"
  | "loading"
  | "ready"
  | "saving"
  | "testing"
  | "querying"
  | "deleting";

export interface ProfileUsageForm {
  enabled: boolean;
  templateType: UsageScriptTemplateType;
  code: string;
  apiKey: string;
  baseUrl: string;
  accessToken: string;
  userId: string;
  timeoutSeconds: number;
  autoQueryIntervalMinutes: number;
}

export interface ProfileUsageViewState {
  status: ProfileUsageStatus;
  profile: ProfileDraft | null;
  loaded: UsageScriptState | null;
  form: ProfileUsageForm;
  result: UsageQueryResult | null;
  officialOAuth: boolean;
  error: string | null;
  notice: "saved" | "tested" | "queried" | "deleted" | null;
}

export interface UsageApi {
  load(profileId: string): Promise<UsageScriptState>;
  save(request: UsageScriptSaveRequest): Promise<UsageScriptState>;
  test(request: UsageScriptSaveRequest): Promise<UsageQueryResult>;
  query(profileId: string): Promise<UsageQueryResult>;
  remove(profileId: string): Promise<UsageScriptState>;
}

export interface UsageScheduler {
  setInterval(callback: () => void, milliseconds: number): unknown;
  clearInterval(handle: unknown): void;
}

export interface ProfileUsageController extends Readable<ProfileUsageViewState> {
  open(profile: ProfileDraft): Promise<void>;
  close(): boolean;
  updateForm(patch: Partial<ProfileUsageForm>): void;
  selectTemplate(template: UsageScriptTemplateType): void;
  save(): Promise<void>;
  test(): Promise<void>;
  query(): Promise<void>;
  remove(): Promise<void>;
  dispose(): void;
}

const emptyForm = (): ProfileUsageForm => ({
  enabled: false,
  templateType: "general",
  code: "",
  apiKey: "",
  baseUrl: "",
  accessToken: "",
  userId: "",
  timeoutSeconds: 10,
  autoQueryIntervalMinutes: 0
});

const formFrom = (profile: ProfileDraft, loaded: UsageScriptState | null): ProfileUsageForm => {
  const config = loaded?.config;
  return {
    enabled: config?.enabled ?? false,
    templateType: config?.templateType ?? "general",
    code: config?.code || loaded?.defaultCode || "",
    apiKey: "",
    baseUrl: config?.baseUrl ?? profile.baseUrl,
    accessToken: "",
    userId: config?.userId ?? "",
    timeoutSeconds: config?.timeoutSeconds ?? 10,
    autoQueryIntervalMinutes: config?.autoQueryIntervalMinutes ?? 0
  };
};

const profileUsesOfficialOAuth = (profile: ProfileDraft): boolean =>
  canonicalProfileToolId(profile.app) === "codex" && providerIsOfficial(profile.provider);

const normalizeBaseUrl = (value: string) => value.trim().replace(/\/+$/, "");

const requestFrom = (profile: ProfileDraft, form: ProfileUsageForm): UsageScriptSaveRequest => ({
  profileId: profile.id,
  enabled: form.enabled,
  templateType: form.templateType,
  code: form.code,
  apiKey: form.apiKey.trim() ? form.apiKey : null,
  baseUrl: form.baseUrl.trim() ? normalizeBaseUrl(form.baseUrl) : null,
  accessToken: form.accessToken.trim() ? form.accessToken : null,
  userId: form.userId.trim() ? form.userId : null,
  timeoutSeconds: Number(form.timeoutSeconds),
  autoQueryIntervalMinutes: Number(form.autoQueryIntervalMinutes)
});

const codeForTemplate = (
  templateType: UsageScriptTemplateType,
  loaded: UsageScriptState | null
): string => {
  if (loaded?.config?.templateType === templateType && loaded.config.code.trim()) {
    return loaded.config.code;
  }
  if (!loaded?.config && loaded?.defaultCode && templateType === "general") {
    return loaded.defaultCode;
  }
  if (templateType === "newapi") {
    return `({
  request: {
    url: "{{baseUrl}}/api/user/self",
    method: "GET",
    headers: {
      "Content-Type": "application/json",
      "Authorization": "Bearer {{accessToken}}",
      "User-Agent": "codestudio-lite/1.0",
      "New-Api-User": "{{userId}}"
    }
  },
  extractor: function(response) {
    if (response.success && response.data) {
      return {
        planName: response.data.group || "Default",
        remaining: response.data.quota / 500000,
        used: response.data.used_quota / 500000,
        total: (response.data.quota + response.data.used_quota) / 500000,
        unit: "USD"
      };
    }
    return { isValid: false, invalidMessage: response.message || "Query failed" };
  }
})`;
  }
  if (templateType === "balance") {
    return `({
  request: {
    url: "{{baseUrl}}/dashboard/billing/credit_grants",
    method: "GET",
    headers: {
      "Authorization": "Bearer {{apiKey}}",
      "User-Agent": "codestudio-lite/1.0"
    }
  },
  extractor: function(response) {
    var total = response.total_granted || response.total_available || response.balance || 0;
    var used = response.total_used || 0;
    return {
      remaining: response.total_available !== undefined ? response.total_available : Math.max(total - used, 0),
      used: used,
      total: total,
      unit: "USD"
    };
  }
})`;
  }
  if (templateType === "token_plan") {
    return `({
  request: {
    url: "{{baseUrl}}/api/user/self",
    method: "GET",
    headers: {
      "Authorization": "Bearer {{apiKey}}",
      "User-Agent": "codestudio-lite/1.0"
    }
  },
  extractor: function(response) {
    var data = response.data || response;
    var total = data.total || data.quota || data.entitlement || 0;
    var used = data.used || data.used_quota || 0;
    return {
      planName: data.plan || data.plan_name || data.group || "Token plan",
      remaining: data.remaining !== undefined ? data.remaining : Math.max(total - used, 0),
      used: used,
      total: total,
      unit: data.unit || "tokens"
    };
  }
})`;
  }
  return `({
  request: {
    url: "{{baseUrl}}/user/balance",
    method: "GET",
    headers: {
      "Authorization": "Bearer {{apiKey}}",
      "User-Agent": "codestudio-lite/1.0"
    }
  },
  extractor: function(response) {
    return {
      isValid: response.is_active !== false,
      remaining: response.balance,
      unit: "USD"
    };
  }
})`;
};

const closedState = (): ProfileUsageViewState => ({
  status: "closed",
  profile: null,
  loaded: null,
  form: emptyForm(),
  result: null,
  officialOAuth: false,
  error: null,
  notice: null
});

export function createProfileUsageController({
  api,
  scheduler
}: {
  api: UsageApi;
  scheduler: UsageScheduler;
}): ProfileUsageController {
  const store = writable<ProfileUsageViewState>(closedState());
  let current = closedState();
  let generation = 0;
  let timer: unknown = null;

  const set = (state: ProfileUsageViewState) => {
    current = state;
    store.set(state);
  };

  const clearTimer = () => {
    if (timer === null) return;
    scheduler.clearInterval(timer);
    timer = null;
  };

  const configureTimer = () => {
    clearTimer();
    const minutes = current.loaded?.config?.enabled
      ? current.loaded.config.autoQueryIntervalMinutes
      : 0;
    if (current.status !== "ready" || minutes <= 0) return;
    timer = scheduler.setInterval(() => void query(), minutes * 60_000);
  };

  const open = async (profile: ProfileDraft) => {
    clearTimer();
    const requestGeneration = ++generation;
    set({
      status: "loading",
      profile,
      loaded: null,
      form: formFrom(profile, null),
      result: null,
      officialOAuth: profileUsesOfficialOAuth(profile),
      error: null,
      notice: null
    });
    try {
      const loaded = await api.load(profile.id);
      if (generation !== requestGeneration || current.profile?.id !== profile.id) return;
      set({
        ...current,
        status: "ready",
        loaded,
        form: formFrom(profile, loaded),
        result: loaded.lastResult
      });
      configureTimer();
    } catch (error) {
      if (generation !== requestGeneration || current.profile?.id !== profile.id) return;
      set({
        ...current,
        status: "ready",
        error: error instanceof Error ? error.message : String(error)
      });
    }
  };

  const close = () => {
    if (current.status !== "ready" && current.status !== "closed") return false;
    generation += 1;
    clearTimer();
    set(closedState());
    return true;
  };

  const runOperation = async <T>(
    status: Exclude<ProfileUsageStatus, "closed" | "loading" | "ready">,
    operation: (profile: ProfileDraft, form: ProfileUsageForm) => Promise<T>,
    apply: (state: ProfileUsageViewState, value: T) => ProfileUsageViewState
  ) => {
    if (current.status !== "ready" || !current.profile) return;
    const requestGeneration = generation;
    const profile = current.profile;
    const form = current.form;
    set({ ...current, status, error: null, notice: null });
    try {
      const value = await operation(profile, form);
      if (generation !== requestGeneration || current.profile?.id !== profile.id) return;
      set(apply({ ...current, status: "ready" }, value));
      configureTimer();
    } catch (error) {
      if (generation !== requestGeneration || current.profile?.id !== profile.id) return;
      set({
        ...current,
        status: "ready",
        error: error instanceof Error ? error.message : String(error)
      });
      configureTimer();
    }
  };

  const save = () =>
    runOperation(
      "saving",
      (profile, form) => api.save(requestFrom(profile, form)),
      (state, loaded) => ({
        ...state,
        loaded,
        form: formFrom(state.profile!, loaded),
        result: loaded.lastResult,
        notice: "saved"
      })
    );

  const test = () =>
    runOperation(
      "testing",
      (profile, form) => api.test(requestFrom(profile, form)),
      (state, result) => ({ ...state, result, notice: "tested" })
    );

  const query = () =>
    runOperation(
      "querying",
      (profile) => api.query(profile.id),
      (state, result) => ({ ...state, result, notice: "queried" })
    );

  const remove = () =>
    runOperation(
      "deleting",
      (profile) => api.remove(profile.id),
      (state, loaded) => ({
        ...state,
        loaded,
        form: formFrom(state.profile!, loaded),
        result: loaded.lastResult,
        notice: "deleted"
      })
    );

  return {
    subscribe: store.subscribe,
    open,
    close,
    updateForm(patch) {
      if (current.status !== "ready") return;
      set({ ...current, form: { ...current.form, ...patch }, error: null, notice: null });
    },
    selectTemplate(templateType) {
      if (current.status !== "ready") return;
      set({
        ...current,
        form: {
          ...current.form,
          templateType,
          code: codeForTemplate(templateType, current.loaded)
        },
        error: null,
        notice: null
      });
    },
    save,
    test,
    query,
    remove,
    dispose() {
      generation += 1;
      clearTimer();
      set(closedState());
    }
  };
}
