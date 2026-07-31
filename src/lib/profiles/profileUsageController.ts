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
  api
}: {
  api: UsageApi;
  scheduler: UsageScheduler;
}): ProfileUsageController {
  const store = writable<ProfileUsageViewState>(closedState());
  let current = closedState();
  let generation = 0;

  const set = (state: ProfileUsageViewState) => {
    current = state;
    store.set(state);
  };

  const open = async (profile: ProfileDraft) => {
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
    set(closedState());
    return true;
  };

  return {
    subscribe: store.subscribe,
    open,
    close,
    updateForm(patch) {
      if (current.status !== "ready") return;
      set({ ...current, form: { ...current.form, ...patch }, error: null, notice: null });
    },
    selectTemplate() {},
    async save() {},
    async test() {},
    async query() {},
    async remove() {},
    dispose() {
      generation += 1;
      set(closedState());
    }
  };
}
