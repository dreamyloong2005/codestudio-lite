<script lang="ts" context="module">
  import arrowLeft from "@iconify-icons/solar/arrow-left-bold-duotone";
  import arrowRight from "@iconify-icons/solar/arrow-right-bold-duotone";
  import checkCircle from "@iconify-icons/solar/check-circle-bold-duotone";
  import checkSquare from "@iconify-icons/solar/check-square-bold-duotone";
  import clockCircle from "@iconify-icons/solar/clock-circle-bold-duotone";
  import codeSquare from "@iconify-icons/solar/code-square-bold-duotone";
  import copy from "@iconify-icons/solar/copy-bold-duotone";
  import dangerCircle from "@iconify-icons/solar/danger-circle-bold-duotone";
  import dangerTriangle from "@iconify-icons/solar/danger-triangle-bold-duotone";
  import downloadSquare from "@iconify-icons/solar/download-square-bold-duotone";
  import eye from "@iconify-icons/solar/eye-bold-duotone";
  import eyeClosed from "@iconify-icons/solar/eye-closed-bold-duotone";
  import folderOpen from "@iconify-icons/solar/folder-open-bold-duotone";
  import infoCircle from "@iconify-icons/solar/info-circle-bold-duotone";
  import keyMinimalistic from "@iconify-icons/solar/key-minimalistic-bold-duotone";
  import magicStick from "@iconify-icons/solar/magic-stick-3-bold-duotone";
  import pen from "@iconify-icons/solar/pen-new-square-bold-duotone";
  import playCircle from "@iconify-icons/solar/play-circle-bold-duotone";
  import power from "@iconify-icons/solar/power-bold-duotone";
  import programming from "@iconify-icons/solar/programming-bold-duotone";
  import refreshCircle from "@iconify-icons/solar/refresh-circle-bold-duotone";
  import restartCircle from "@iconify-icons/solar/restart-circle-bold-duotone";
  import rocket from "@iconify-icons/solar/rocket-bold-duotone";
  import settings from "@iconify-icons/solar/settings-bold-duotone";
  import stopCircle from "@iconify-icons/solar/stop-circle-bold-duotone";
  import sun from "@iconify-icons/solar/sun-2-bold-duotone";
  import translation from "@iconify-icons/solar/translation-bold-duotone";
  import trash from "@iconify-icons/solar/trash-bin-minimalistic-bold-duotone";
  import tuning from "@iconify-icons/solar/tuning-square-bold-duotone";
  import uploadSquare from "@iconify-icons/solar/upload-square-bold-duotone";
  import user from "@iconify-icons/solar/user-rounded-bold-duotone";
  import usersGroup from "@iconify-icons/solar/users-group-rounded-bold-duotone";
  import widget from "@iconify-icons/solar/widget-5-bold-duotone";
  import windowFrame from "@iconify-icons/solar/window-frame-bold-duotone";

  type SolarIcon = typeof infoCircle;

  export const appIcons = {
    apply: checkSquare,
    arrowLeft,
    arrowRight,
    check: checkCircle,
    clock: clockCircle,
    codexClient: windowFrame,
    copy,
    dashboard: widget,
    delete: trash,
    download: downloadSquare,
    edit: pen,
    error: dangerCircle,
    externalLink: arrowRight,
    folder: folderOpen,
    info: infoCircle,
    install: codeSquare,
    key: keyMinimalistic,
    language: translation,
    loading: refreshCircle,
    play: playCircle,
    power,
    profiles: usersGroup,
    repair: tuning,
    restart: restartCircle,
    rocket,
    settings,
    stop: stopCircle,
    system: programming,
    theme: sun,
    update: uploadSquare,
    upload: uploadSquare,
    user,
    warning: dangerTriangle,
    wizard: magicStick,
    eye,
    eyeOff: eyeClosed
  } satisfies Record<string, SolarIcon>;

  export type AppIconName = keyof typeof appIcons;
</script>

<script lang="ts">
  import Icon from "@iconify/svelte/dist/OfflineIcon.svelte";

  export let name: AppIconName | string = "info";
  export let size = 18;
  export let title: string | undefined = undefined;

  let className = "";
  export { className as class };

  $: icon = appIcons[name as AppIconName] ?? appIcons.info;
</script>

<Icon
  {icon}
  width={size}
  height={size}
  class={className}
  role={title ? "img" : undefined}
  aria-label={title}
  aria-hidden={title ? undefined : "true"}
/>
