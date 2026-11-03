<script lang="ts">
  import "../app.css";
  import { fly, fade } from "svelte/transition";
  import { cubicOut } from "svelte/easing";
  import { api } from "$lib/api";
  import Icon from "$lib/components/Icon.svelte";
  import Sheet from "$lib/components/Sheet.svelte";
  import Rules from "$lib/screens/Rules.svelte";
  import Bypass from "$lib/screens/Bypass.svelte";
  import Diagnose from "$lib/screens/Diagnose.svelte";
  import Advanced from "$lib/screens/Advanced.svelte";
  import Home from "$lib/screens/Home.svelte";
  import Logs from "$lib/screens/Logs.svelte";
  import Nodes from "$lib/screens/Nodes.svelte";
  import Onboarding from "$lib/screens/Onboarding.svelte";
  import Profiles from "$lib/screens/Profiles.svelte";
  import Settings from "$lib/screens/Settings.svelte";
  import { app } from "$lib/store.svelte";

  $effect(() => {
    app.init();
    // Surface unexpected UI errors instead of failing silently.
    const onError = (event: ErrorEvent) => app.showToast(`界面出错：${event.message}`, "error");
    const onRejection = (event: PromiseRejectionEvent) =>
      app.showToast(`界面出错：${String(event.reason?.message ?? event.reason)}`, "error");
    window.addEventListener("error", onError);
    window.addEventListener("unhandledrejection", onRejection);
    return () => {
      window.removeEventListener("error", onError);
      window.removeEventListener("unhandledrejection", onRejection);
    };
  });

  const onboarding = $derived(app.view !== null && app.view.profiles.length === 0);
  const isWindows = navigator.userAgent.includes("Windows");

  let installing = $state(false);
  let importing = $state(false);

  async function install() {
    installing = true;
    await app.run(() => api.installTunSupport());
    installing = false;
  }

  async function confirmImport() {
    const link = app.pendingImport;
    if (!link) return;
    importing = true;
    const view = await app.run(() => api.addSubscription(link.url, link.name));
    importing = false;
    app.pendingImport = null;
    if (view) {
      app.showToast("订阅已导入");
      app.back("home");
    }
  }
  const slide = (node: Element) =>
    fly(node, { x: app.forward ? 40 : -40, duration: 260, easing: cubicOut, opacity: 0 });
</script>

<div class="app">
  <div class="glow" class:on={app.view?.phase === "connected"}></div>

  {#if app.view}
    {#key onboarding ? "welcome" : app.screen}
      <div class="page" in:slide>
        {#if onboarding}
          <Onboarding />
        {:else if app.screen === "nodes"}
          <Nodes />
        {:else if app.screen === "settings"}
          <Settings />
        {:else if app.screen === "profiles"}
          <Profiles />
        {:else if app.screen === "add"}
          <Onboarding onBack={() => app.back()} />
        {:else if app.screen === "diagnose"}
          <Diagnose />
        {:else if app.screen === "bypass"}
          <Bypass />
        {:else if app.screen === "rules"}
          <Rules />
        {:else if app.screen === "advanced"}
          <Advanced />
        {:else if app.screen === "logs"}
          <Logs />
        {:else}
          <Home />
        {/if}
      </div>
    {/key}
  {/if}

  {#if app.view?.needsInstall}
    <Sheet
      title={app.view.needsInstall === "outdated" ? "更新增强模式组件" : "开启增强模式"}
      onclose={() => api.dismissError()}
    >
      <p>增强模式会接管电脑上<b>所有程序</b>的网络流量，包括不支持系统代理的游戏和命令行工具。</p>
      <p>
        {#if isWindows}
          需要安装一个后台组件，Windows 会弹出一次管理员授权，之后开关就不再需要授权。
        {:else}
          需要为内核授予一次网络权限，系统会请求一次管理员密码。
        {/if}
      </p>
      {#snippet actions()}
        <button class="ghost" onclick={() => api.dismissError()} disabled={installing}>取消</button>
        <button class="primary" onclick={install} disabled={installing}>
          {installing ? "等待授权…" : "授权并开启"}
        </button>
      {/snippet}
    </Sheet>
  {/if}

  {#if app.pendingImport}
    <Sheet title="导入订阅" onclose={() => (app.pendingImport = null)}>
      <p>网页请求导入一个订阅{app.pendingImport.name ? `「${app.pendingImport.name}」` : ""}：</p>
      <p class="link">{app.pendingImport.url}</p>
      <p>只导入你信任的服务商提供的订阅。</p>
      {#snippet actions()}
        <button class="ghost" onclick={() => (app.pendingImport = null)} disabled={importing}>
          取消
        </button>
        <button class="primary" onclick={confirmImport} disabled={importing}>
          {importing ? "正在导入…" : "导入并使用"}
        </button>
      {/snippet}
    </Sheet>
  {/if}

  {#if app.toast}
    <div class="toast {app.toast.tone}" transition:fly={{ y: 16, duration: 220 }}>
      {#if app.toast.tone === "error"}<Icon name="alert" size={16} />{/if}
      <span>{app.toast.text}</span>
      <button onclick={() => (app.toast = null)} aria-label="关闭"><Icon name="close" size={14} /></button>
    </div>
  {/if}
</div>

<style>
  .app {
    position: relative;
    height: 100vh;
    overflow: hidden;
    isolation: isolate;
  }
  /* Ambient warm light at the top; brightens when connected. */
  .glow {
    position: absolute;
    inset: -30% -20% auto;
    height: 70%;
    z-index: -1;
    background: radial-gradient(ellipse at 50% 30%, var(--bg-glow), transparent 65%);
    opacity: 0.55;
    transition: opacity 0.8s var(--ease);
    pointer-events: none;
  }
  .glow.on {
    opacity: 1;
  }
  .page {
    position: absolute;
    inset: 0;
  }

  .link {
    padding: 8px 10px;
    border-radius: 10px;
    background: var(--surface-2);
    font-size: 12px;
    word-break: break-all;
    user-select: text;
  }

  .toast {
    position: absolute;
    left: 16px;
    right: 16px;
    bottom: 16px;
    z-index: 10;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 10px 12px 14px;
    border-radius: 14px;
    background: var(--text);
    color: var(--bg);
    font-size: 13px;
    line-height: 1.45;
    box-shadow: 0 12px 32px rgb(0 0 0 / 0.25);
  }
  .toast.error {
    background: var(--bad);
    color: #fff;
  }
  .toast span {
    flex: 1;
    user-select: text;
  }
  .toast button {
    display: grid;
    opacity: 0.7;
  }
  .toast button:hover {
    opacity: 1;
  }
</style>
