<script lang="ts">
  import { createEventDispatcher, onDestroy } from "svelte";
  import { css, cx } from "../../styled-system/css";
  import { noticeRecipe } from "../../styled-system/recipes";
  import { t } from "../lib/i18n";
  import AppIcon from "./AppIcon.svelte";

  export let tone: "success" | "error" = "success";
  export let message = "";
  // Auto-dismiss is off by default so users can read and copy the message at
  // their own pace. A caller can opt back in by passing a positive timeoutMs.
  export let timeoutMs = 0;

  const dispatch = createEventDispatcher<{ dismiss: void }>();
  const noticeTextClass = css({
    minWidth: 0,
    overflowWrap: "anywhere"
  });
  const noticeDismissClass = css({
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    flex: "0 0 auto",
    width: "28px",
    minHeight: "28px",
    border: "1px solid currentColor",
    borderRadius: "7px",
    background: "transparent",
    color: "inherit",
    padding: 0,
    fontSize: "12px",
    fontWeight: "800",
    opacity: 0.78,
    transition: "background var(--motion-quick), opacity var(--motion-quick), transform var(--motion-smooth)",
    _hover: {
      background: "color-mix(in srgb, currentColor 10%, transparent)",
      opacity: 1,
      transform: "translateY(-1px)"
    }
  });
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
  <div class={noticeRecipe({ tone })} role={tone === "error" ? "alert" : "status"} aria-live={tone === "error" ? "assertive" : "polite"}>
    <span class={noticeTextClass}>{message}</span>
    <button
      type="button"
      class={cx(noticeDismissClass)}
      aria-label={$t("common.close")}
      title={$t("common.close")}
      on:click={dismiss}
    >
      <AppIcon name="close" size={16} />
    </button>
  </div>
{/if}
