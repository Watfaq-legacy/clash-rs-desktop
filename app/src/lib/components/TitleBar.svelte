<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import Icon from "./Icon.svelte";

  let {
    title = "",
    onBack,
    onSettings,
  }: { title?: string; onBack?: () => void; onSettings?: () => void } = $props();

  const win = getCurrentWindow();
</script>

<header data-tauri-drag-region>
  <div class="left" data-tauri-drag-region>
    {#if onBack}
      <button class="icon" onclick={onBack} aria-label="返回"><Icon name="back" /></button>
      <span class="title" data-tauri-drag-region>{title}</span>
    {:else}
      <span class="brand" data-tauri-drag-region>
        <img class="logo" src="/brand/logo.png" alt="" draggable="false" />Choclite
      </span>
    {/if}
  </div>
  <div class="right">
    {#if onSettings}
      <button class="icon" onclick={onSettings} aria-label="设置"><Icon name="settings" /></button>
    {/if}
    <button class="icon" onclick={() => win.minimize()} aria-label="最小化">
      <Icon name="minimize" size={16} />
    </button>
    <!-- Hides to tray; the backend keeps the connection alive. -->
    <button class="icon close" onclick={() => win.hide()} aria-label="关闭">
      <Icon name="close" size={16} />
    </button>
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
