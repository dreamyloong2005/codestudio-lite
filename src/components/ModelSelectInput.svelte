<script lang="ts">
  import { createEventDispatcher, onMount, tick } from "svelte";
  import { css, cx } from "../../styled-system/css";
  import { iconButtonRecipe } from "../../styled-system/recipes";
  import type { ProfileModelOption } from "../types";
  import AppIcon from "./AppIcon.svelte";

  export let id: string;
  export let value = "";
  export let options: ProfileModelOption[] = [];
  export let disabled = false;
  export let placeholder = "";
  export let toggleTitle = "";
  export let optionLabel: (option: ProfileModelOption) => string = defaultOptionLabel;

  const dispatch = createEventDispatcher<{
    input: { value: string; option: ProfileModelOption | null };
    select: { value: string; option: ProfileModelOption };
    blur: { value: string };
  }>();

  const pickerClass = css({
    position: "relative",
    display: "grid",
    minWidth: 0,
    width: "100%"
  });
  const inputFrameClass = css({
    display: "grid",
    gridTemplateColumns: "minmax(0, 1fr) 38px",
    alignItems: "center",
    minWidth: 0,
    minHeight: "38px",
    overflow: "hidden",
    border: "1px solid var(--border)",
    borderRadius: "8px",
    background: "var(--surface)",
    transition: "border-color 160ms ease, box-shadow 160ms ease, background 160ms ease",
    _focusWithin: {
      borderColor: "color-mix(in srgb, var(--accent) 70%, var(--border))",
      boxShadow: "0 0 0 3px color-mix(in srgb, var(--accent) 16%, transparent)"
    },
    "& input": {
      width: "100%",
      minWidth: 0,
      height: "36px",
      border: 0,
      outline: 0,
      background: "transparent",
      color: "var(--text)",
      fontSize: "13px",
      fontWeight: 700,
      padding: "0 10px"
    },
    "& input:disabled": {
      color: "var(--text-muted)",
      cursor: "not-allowed"
    },
    "&[data-disabled='true']": {
      opacity: 0.72
    }
  });
  const toggleButtonClass = css({
    height: "36px",
    minHeight: "36px",
    width: "38px",
    borderWidth: "0 0 0 1px",
    borderRadius: 0,
    borderColor: "var(--border)",
    background: "transparent",
    color: "var(--text-soft)",
    _hover: {
      background: "var(--surface-hover)"
    },
    "&[aria-expanded='true'] svg": {
      transform: "rotate(180deg)"
    },
    "& svg": {
      transition: "transform 140ms ease"
    }
  });
  const optionListClass = css({
    position: "absolute",
    zIndex: 30,
    top: "calc(100% + 4px)",
    left: 0,
    right: 0,
    display: "grid",
    gap: "2px",
    maxHeight: "260px",
    overflowY: "auto",
    padding: "6px",
    border: "1px solid var(--border)",
    borderRadius: "8px",
    background: "var(--surface)",
    boxShadow: "0 18px 44px color-mix(in srgb, var(--shadow) 22%, transparent)"
  });
  const optionButtonClass = css({
    display: "grid",
    justifyItems: "start",
    width: "100%",
    minWidth: 0,
    minHeight: "34px",
    padding: "7px 9px",
    border: "1px solid transparent",
    borderRadius: "6px",
    background: "transparent",
    color: "var(--text)",
    textAlign: "left",
    fontSize: "12px",
    fontWeight: 800,
    cursor: "pointer",
    overflowWrap: "anywhere",
    _hover: {
      background: "var(--surface-hover)"
    },
    "&[data-active='true']": {
      background: "color-mix(in srgb, var(--accent) 12%, transparent)",
      borderColor: "color-mix(in srgb, var(--accent) 24%, transparent)"
    },
    "&[data-selected='true']": {
      color: "var(--accent-strong)"
    }
  });

  let rootEl: HTMLDivElement | null = null;
  let inputEl: HTMLInputElement | null = null;
  let open = false;
  let activeIndex = -1;

  $: hasOptions = options.length > 0;
  $: selectedValue = value.trim();
  $: listboxId = `${id}-listbox`;
  $: if (!hasOptions || disabled) {
    open = false;
    activeIndex = -1;
  }
  $: if (activeIndex >= options.length) {
    activeIndex = options.length - 1;
  }

  onMount(() => {
    const closeOnOutsidePointer = (event: PointerEvent) => {
      const target = event.target;
      if (target instanceof Node && rootEl?.contains(target)) {
        return;
      }
      closeList();
    };
    document.addEventListener("pointerdown", closeOnOutsidePointer);
    return () => document.removeEventListener("pointerdown", closeOnOutsidePointer);
  });

  function defaultOptionLabel(option: ProfileModelOption) {
    const label = option.name && option.name !== option.id ? `${option.id} - ${option.name}` : option.id;
    return option.supports1m ? `${label} (1M)` : label;
  }

  function activeOptionId(index: number) {
    return `${listboxId}-${index}`;
  }

  function selectedOptionIndex() {
    return options.findIndex((option) => option.id === selectedValue);
  }

  function openList() {
    if (disabled || !hasOptions) {
      return;
    }
    open = true;
    const index = selectedOptionIndex();
    activeIndex = index >= 0 ? index : 0;
  }

  function closeList() {
    open = false;
    activeIndex = -1;
  }

  function toggleList() {
    if (open) {
      closeList();
    } else {
      openList();
      void tick().then(() => inputEl?.focus());
    }
  }

  function emitInput(nextValue: string, option: ProfileModelOption | null = null) {
    value = nextValue;
    dispatch("input", { value: nextValue, option });
  }

  function handleInput(event: Event) {
    emitInput((event.currentTarget as HTMLInputElement).value);
    openList();
  }

  function selectOption(option: ProfileModelOption) {
    emitInput(option.id, option);
    dispatch("select", { value: option.id, option });
    closeList();
    void tick().then(() => inputEl?.focus());
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      if (!open) {
        openList();
        return;
      }
      activeIndex = Math.min(options.length - 1, activeIndex + 1);
      return;
    }
    if (event.key === "ArrowUp") {
      event.preventDefault();
      if (!open) {
        openList();
        return;
      }
      activeIndex = Math.max(0, activeIndex - 1);
      return;
    }
    if (event.key === "Enter" && open && activeIndex >= 0 && options[activeIndex]) {
      event.preventDefault();
      selectOption(options[activeIndex]);
      return;
    }
    if (event.key === "Escape") {
      closeList();
    }
  }
</script>

<div class={pickerClass} bind:this={rootEl}>
  <div class={inputFrameClass} data-disabled={disabled}>
    <input
      {id}
      bind:this={inputEl}
      value={value}
      {disabled}
      {placeholder}
      autocomplete="off"
      aria-autocomplete="none"
      aria-controls={hasOptions ? listboxId : undefined}
      aria-expanded={open}
      aria-activedescendant={open && activeIndex >= 0 ? activeOptionId(activeIndex) : undefined}
      role="combobox"
      on:input={handleInput}
      on:focus={openList}
      on:keydown={handleKeydown}
      on:blur={() => dispatch("blur", { value })}
    />
    <button
      class={cx(iconButtonRecipe(), toggleButtonClass)}
      type="button"
      disabled={disabled || !hasOptions}
      title={toggleTitle}
      aria-label={toggleTitle}
      aria-expanded={open}
      aria-controls={hasOptions ? listboxId : undefined}
      on:click={toggleList}
    >
      <AppIcon name="chevronDown" size={16} />
    </button>
  </div>
  {#if open && hasOptions}
    <div class={optionListClass} id={listboxId} role="listbox">
      {#each options as option, index}
        <button
          class={optionButtonClass}
          id={activeOptionId(index)}
          type="button"
          role="option"
          title={optionLabel(option)}
          aria-selected={option.id === selectedValue}
          data-active={index === activeIndex}
          data-selected={option.id === selectedValue}
          on:mouseenter={() => (activeIndex = index)}
          on:pointerdown|preventDefault
          on:click={() => selectOption(option)}
        >
          {optionLabel(option)}
        </button>
      {/each}
    </div>
  {/if}
</div>
