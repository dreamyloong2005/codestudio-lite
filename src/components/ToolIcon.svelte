<script lang="ts">
  import AppIcon from "./AppIcon.svelte";

  export let toolId: string | null | undefined = null;
  export let label = "";
  export let category: "ai_tool" | "system" | string | null | undefined = null;
  export let variant: "card" | "choice" | "heading" = "card";

  type IconDef = {
    src: string;
    tone: string;
  };

  const iconMap: Record<string, IconDef> = {
    codex: { src: "/tool-icons/codex.svg", tone: "codex" },
    "codex-app": { src: "/tool-icons/codex-app.png", tone: "codex-app" },
    "codex-vscode": { src: "/tool-icons/codex-vscode.svg", tone: "codex" },
    claude: { src: "/tool-icons/claude-code.svg", tone: "claude" },
    "claude-desktop": { src: "/tool-icons/claude-desktop.svg", tone: "claude" },
    "claude-vscode": { src: "/tool-icons/claude-vscode.svg", tone: "light" },
    gemini: { src: "/tool-icons/gemini-cli.svg", tone: "gemini" },
    "gemini-code-assist": { src: "/tool-icons/gemini-code-assist.svg", tone: "gemini" },
    opencode: { src: "/tool-icons/opencode.svg", tone: "light" },
    openclaw: { src: "/tool-icons/openclaw.svg", tone: "openclaw" },
    hermes: { src: "/tool-icons/hermes.png", tone: "hermes" },
    node: { src: "/tool-icons/nodejs.svg", tone: "light" },
    git: { src: "/tool-icons/git.svg", tone: "light" },
    npm: { src: "/tool-icons/npm.svg", tone: "light" },
    pnpm: { src: "/tool-icons/pnpm.svg", tone: "light" },
    bun: { src: "/tool-icons/bun.svg", tone: "light" }
  };

  function canonicalIconId(id: string | null | undefined) {
    switch (id) {
      case "codex":
      case "codex-cli":
        return "codex";
      case "codex-app":
      case "codex-client":
      case "codex-desktop":
        return "codex-app";
      case "codex-vscode":
      case "codex-code-vscode":
      case "codex-vs-code":
        return "codex-vscode";
      case "claude":
      case "claude-code":
        return "claude";
      case "claude-desktop":
      case "claude-app":
      case "claude-client":
        return "claude-desktop";
      case "claude-vscode":
      case "claude-code-vscode":
      case "claude-vs-code":
        return "claude-vscode";
      case "hermes":
      case "hermes-agent":
        return "hermes";
      case "gemini":
      case "gemini-cli":
        return "gemini";
      case "gemini-code-assist":
      case "gemini-vscode":
      case "gemini-code-vscode":
      case "gemini-vs-code":
        return "gemini-code-assist";
      case "node":
      case "nodejs":
        return "node";
      default:
        return id ?? "";
    }
  }

  $: iconKey = canonicalIconId(toolId);
  $: icon = iconMap[iconKey];
  $: accessibleLabel = label || iconKey || "Tool";
</script>

<span
  class:tool-icon-card={variant === "card"}
  class:tool-icon-choice={variant === "choice"}
  class:tool-icon-heading={variant === "heading"}
  class:tool-icon-brand={Boolean(icon)}
  class:tool-icon-fallback={!icon}
  class={`tool-icon ${icon ? `tool-icon-${icon.tone}` : ""}`}
  aria-label={accessibleLabel}
  title={accessibleLabel}
>
  {#if icon}
    <img src={icon.src} alt="" aria-hidden="true" />
  {:else if category === "system"}
    <AppIcon name="system" size={18} />
  {:else}
    <span aria-hidden="true">{accessibleLabel.slice(0, 2).toUpperCase()}</span>
  {/if}
</span>
