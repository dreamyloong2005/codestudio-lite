<script lang="ts">
  import { createEventDispatcher, onDestroy } from "svelte";
  import { t } from "../lib/i18n";
  import AppIcon from "./AppIcon.svelte";

  export let tone: "success" | "error" = "success";
  export let message = "";
  // Auto-dismiss is off by default so users can read and copy the message at
  // their own pace. A caller can opt back in by passing a positive timeoutMs.
  export let timeoutMs = 0;

  const dispatch = createEventDispatcher<{ dismiss: void }>();
  let timer: ReturnType<typeof setTimeout> | null = null;
  let scheduledKey = "";

  $: {
    const nextKey = message ? `${message}:${timeoutMs}` : "";
    if (nextKey !== scheduledKey) {
      scheduledKey = nextKey;
      scheduleDismiss(message, timeoutMs);
    }
  }

  onDestroy(() => {
    clearDismissTimer();
  });

  function clearDismissTimer() {
    if (timer !== null) {
      clearTimeout(timer);
      timer = null;
    }
  }

  function scheduleDismiss(nextMessage: string, nextTimeoutMs: number) {
    clearDismissTimer();
    if (!nextMessage || nextTimeoutMs <= 0) {
      return;
    }
    timer = setTimeout(() => {
      dispatch("dismiss");
    }, nextTimeoutMs);
  }

  function dismiss() {
    clearDismissTimer();
    dispatch("dismiss");
  }
</script>

{#if message}
  <div class={`notice inline-${tone}`} role={tone === "error" ? "alert" : "status"} aria-live={tone === "error" ? "assertive" : "polite"}>
    <span>{message}</span>
    <button
      type="button"
      class="notice-dismiss"
      aria-label={$t("common.close")}
      title={$t("common.close")}
      on:click={dismiss}
    >
      <AppIcon name="close" size={16} />
    </button>
  </div>
{/if}
