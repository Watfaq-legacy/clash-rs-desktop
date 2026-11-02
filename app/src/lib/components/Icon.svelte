<script lang="ts" module>
  // 24x24 stroke icons (Lucide-style paths).
  const PATHS = {
    power: "M12 3v9 M18.36 6.64a9 9 0 1 1-12.73 0",
    settings:
      "M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z M12 9a3 3 0 1 0 0 6 3 3 0 1 0 0-6z",
    back: "M15 18l-6-6 6-6",
    chevron: "M9 18l6-6-6-6",
    minimize: "M5 12h14",
    close: "M18 6 6 18 M6 6l12 12",
    refresh: "M21 12a9 9 0 1 1-2.64-6.36L21 8 M21 3v5h-5",
    trash: "M3 6h18 M8 6V4h8v2 M19 6l-1 14H6L5 6",
    plus: "M12 5v14 M5 12h14",
    check: "M20 6 9 17l-5-5",
    globe: "M12 2a10 10 0 1 0 0 20 10 10 0 1 0 0-20z M2 12h20 M12 2a15 15 0 0 1 0 20 M12 2a15 15 0 0 0 0 20",
    bolt: "M13 2 3 14h9l-1 8 10-12h-9l1-8z",
    clipboard: "M9 2h6v4H9z M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2",
    up: "M12 19V5 M5 12l7-7 7 7",
    down: "M12 5v14 M19 12l-7 7-7-7",
    code: "M16 18l6-6-6-6 M8 6l-6 6 6 6",
    list: "M8 6h13 M8 12h13 M8 18h13 M3 6h.01 M3 12h.01 M3 18h.01",
    layers: "M12 2 2 7l10 5 10-5-10-5z M2 17l10 5 10-5 M2 12l10 5 10-5",
    exit: "M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4 M16 17l5-5-5-5 M21 12H9",
    alert: "M12 2a10 10 0 1 0 0 20 10 10 0 1 0 0-20z M12 8v4 M12 16h.01",
  } as const;

  export type IconName = keyof typeof PATHS;
</script>

<script lang="ts">
  let { name, size = 18, stroke = 2 }: { name: IconName; size?: number; stroke?: number } =
    $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width={stroke}
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  <path d={PATHS[name]} />
</svg>
