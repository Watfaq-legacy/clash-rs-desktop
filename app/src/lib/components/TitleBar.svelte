<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { t } from "$lib/i18n.svelte";
  import { app } from "$lib/store.svelte";
  import Icon from "./Icon.svelte";
  import { currentPane } from "./Pane.svelte";

  let {
    title = "",
    onBack,
    onSettings,
  }: { title?: string; onBack?: () => void; onSettings?: () => void } = $props();

  const win = getCurrentWindow();
  const pane = currentPane();
  // Side by side, the window buttons belong to the right half, and "back"
  // to the home screen is pointless because home is always visible.
  const controls = $derived(pane.kind !== "main");
  const showBack = $derived(!!onBack && !(pane.kind === "side" && app.backTarget === "home"));
</script>

<header data-tauri-drag-region>
  <div class="left" data-tauri-drag-region>
    {#if showBack}
      <button class="icon" onclick={onBack} aria-label={t("返回", "Back")}><Icon name="back" /></button>
      <span class="title" data-tauri-drag-region>{title}</span>
    {:else if pane.kind === "side"}
      <span class="title flush" data-tauri-drag-region>{title}</span>
    {:else}
      <span class="brand" data-tauri-drag-region>
        <img class="logo" src="/brand/logo.png" alt="" draggable="false" />Choclite
      </span>
    {/if}
  </div>
  <div class="right">
    {#if onSettings}
      <button class="icon" onclick={onSettings} aria-label={t("设置", "Settings")}><Icon name="settings" /></button>
    {/if}
    {#if controls}
      <button class="icon" onclick={() => win.minimize()} aria-label={t("最小化", "Minimize")}>
        <Icon name="minimize" size={16} />
      </button>
      <!-- The backend turns close into "hide to tray" and keeps the connection. -->
      <button class="icon close" onclick={() => win.close()} aria-label={t("关闭", "Close")}>
        <Icon name="close" size={16} />
      </button>
    {/if}
  </div>
</header>

<style>
  header {
    height: 48px;
    padding: 0 8px 0 12px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-shrink: 0;
  }
  .left,
  .right {
    display: flex;
    align-items: center;
    gap: 2px;
    height: 100%;
  }
  .left {
    flex: 1;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    font-weight: 650;
    font-size: 15px;
    letter-spacing: 0.2px;
    padding-left: 6px;
  }
  .logo {
    width: 20px;
    height: 20px;
    pointer-events: none;
  }
  .title {
    font-weight: 600;
    font-size: 15px;
    margin-left: 2px;
  }
  .title.flush {
    padding-left: 8px;
  }
  .icon {
    width: 34px;
    height: 34px;
    border-radius: 9px;
    display: grid;
    place-items: center;
    color: var(--text-2);
    transition: background 0.15s, color 0.15s;
  }
  .icon:hover {
    background: var(--surface-2);
    color: var(--text);
  }
  .close:hover {
    background: var(--bad);
    color: #fff;
  }
</style>
