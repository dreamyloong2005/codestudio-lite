<script lang="ts">
  import { Eye, EyeOff } from "@lucide/svelte";

  export let value = "";
  export let placeholder = "sk-...";

  let visible = false;
</script>

<div class="secret-input">
  <input
    bind:value
    type={visible ? "text" : "password"}
    autocomplete="off"
    spellcheck="false"
    {placeholder}
  />
  <button type="button" class="icon-button" title={visible ? "Hide API key" : "Show API key"} on:click={() => (visible = !visible)}>
    {#if visible}
      <EyeOff size={16} />
    {:else}
      <Eye size={16} />
    {/if}
  </button>
</div>
