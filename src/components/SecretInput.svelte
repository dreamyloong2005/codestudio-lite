<script lang="ts">
  import { iconButtonRecipe, secretInputRecipe } from "../../styled-system/recipes";
  import AppIcon from "./AppIcon.svelte";

  export let value = "";
  export let placeholder = "sk-...";

  let visible = false;
</script>

<div class={secretInputRecipe()}>
  <input
    bind:value
    type={visible ? "text" : "password"}
    autocomplete="off"
    spellcheck="false"
    {placeholder}
  />
  <button
    type="button"
    class={iconButtonRecipe()}
    title={visible ? "Hide API key" : "Show API key"}
    aria-label={visible ? "Hide API key" : "Show API key"}
    on:click={() => (visible = !visible)}
  >
    {#if visible}
      <AppIcon name="eyeOff" size={16} />
    {:else}
      <AppIcon name="eye" size={16} />
    {/if}
  </button>
</div>
