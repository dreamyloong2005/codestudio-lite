<script lang="ts" context="module">
  import arrowLeft from "@iconify-icons/ph/arrow-left";
  import arrowRight from "@iconify-icons/ph/arrow-right";
  import plusCircle from "@iconify-icons/ph/plus-circle";
  import checkCircle from "@iconify-icons/ph/check-circle";
  import checkSquare from "@iconify-icons/ph/check-square";
  import chartBar from "@iconify-icons/ph/chart-bar";
  import clock from "@iconify-icons/ph/clock";
  import copy from "@iconify-icons/ph/copy";
  import caretDown from "@iconify-icons/ph/caret-down";
  import x from "@iconify-icons/ph/x";
  import appWindow from "@iconify-icons/ph/app-window";
  import downloadSimple from "@iconify-icons/ph/download-simple";
  import dotsSixVertical from "@iconify-icons/ph/dots-six-vertical";
  import eye from "@iconify-icons/ph/eye";
  import eyeSlash from "@iconify-icons/ph/eye-slash";
  import folderOpen from "@iconify-icons/ph/folder-open";
  import gearSix from "@iconify-icons/ph/gear-six";
  import globeHemisphereWest from "@iconify-icons/ph/globe-hemisphere-west";
  import info from "@iconify-icons/ph/info";
  import key from "@iconify-icons/ph/key";
  import magicWand from "@iconify-icons/ph/magic-wand";
  import pencilSimple from "@iconify-icons/ph/pencil-simple";
  import playCircle from "@iconify-icons/ph/play-circle";
  import power from "@iconify-icons/ph/power";
  import rocketLaunch from "@iconify-icons/ph/rocket-launch";
  import arrowClockwise from "@iconify-icons/ph/arrow-clockwise";
  import arrowCounterClockwise from "@iconify-icons/ph/arrow-counter-clockwise";
  import arrowsClockwise from "@iconify-icons/ph/arrows-clockwise";
  import slidersHorizontal from "@iconify-icons/ph/sliders-horizontal";
  import squaresFour from "@iconify-icons/ph/squares-four";
  import stackSimple from "@iconify-icons/ph/stack-simple";
  import stopCircle from "@iconify-icons/ph/stop-circle";
  import sun from "@iconify-icons/ph/sun";
  import terminalWindow from "@iconify-icons/ph/terminal-window";
  import translate from "@iconify-icons/ph/translate";
  import trash from "@iconify-icons/ph/trash";
  import uploadSimple from "@iconify-icons/ph/upload-simple";
  import user from "@iconify-icons/ph/user";
  import warning from "@iconify-icons/ph/warning";
  import warningCircle from "@iconify-icons/ph/warning-circle";
  import wrench from "@iconify-icons/ph/wrench";

  type UiIcon = typeof info;

  export const appIcons = {
    add: plusCircle,
    apply: checkSquare,
    arrowLeft,
    arrowRight,
    check: checkCircle,
    chevronDown: caretDown,
    clock,
    close: x,
    claudeDesktop: appWindow,
    chatgptDesktop: appWindow,
    copy,
    dashboard: squaresFour,
    delete: trash,
    download: downloadSimple,
    drag: dotsSixVertical,
    edit: pencilSimple,
    error: warningCircle,
    externalLink: arrowRight,
    folder: folderOpen,
    gateway: globeHemisphereWest,
    info,
    install: downloadSimple,
    key,
    language: translate,
    loading: arrowClockwise,
    play: playCircle,
    power,
    profiles: stackSimple,
    repair: wrench,
    refresh: arrowClockwise,
    restart: arrowCounterClockwise,
    rocket: rocketLaunch,
    settings: gearSix,
    stats: chartBar,
    stop: stopCircle,
    system: terminalWindow,
    theme: sun,
    update: arrowsClockwise,
    upload: uploadSimple,
    user,
    warning,
    wizard: magicWand,
    eye,
    eyeOff: eyeSlash
  } satisfies Record<string, UiIcon>;

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
