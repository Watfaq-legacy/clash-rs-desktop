<script lang="ts">
  import "../app.css";
  import { fly, fade } from "svelte/transition";
  import { cubicOut } from "svelte/easing";
  import Icon from "$lib/components/Icon.svelte";
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
  });

  const onboarding = $derived(app.view !== null && app.view.profiles.length === 0);
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
