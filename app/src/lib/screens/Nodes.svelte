<script lang="ts">
  import { api, type NodeItem, type Nodes } from "$lib/api";
  import Icon from "$lib/components/Icon.svelte";
  import TitleBar from "$lib/components/TitleBar.svelte";
  import * as fmt from "$lib/format";
  import { app } from "$lib/store.svelte";

  const BUILTIN: Record<string, string> = { DIRECT: "直连（不使用代理）", REJECT: "拒绝" };

  let nodes = $state<Nodes | null>(null);
  let testing = $state(false);
  let switching = $state<string | null>(null);

  $effect(() => {
    app.run(() => api.getNodes()).then((n) => n && (nodes = n));
  });

  // If the connection drops while browsing, there is nothing to pick from.
  $effect(() => {
    if (app.view?.phase !== "connected") app.back();
  });

  const autos = $derived(nodes?.items.filter((i) => i.auto) ?? []);
  const manual = $derived(nodes?.items.filter((i) => !i.auto) ?? []);

  async function pick(item: NodeItem) {
    if (!nodes?.group || item.name === nodes.selected || switching) return;
    switching = item.name;
    const next = await app.run(() => api.selectNode(nodes!.group!, item.name));
    switching = null;
    if (next) nodes = next;
  }

  async function test() {
    if (!nodes?.group || testing) return;
    testing = true;
    const next = await app.run(() => api.testDelays(nodes!.group!));
    testing = false;
    if (next) nodes = next;
  }
</script>

{#snippet row(item: NodeItem)}
  {@const selected = item.name === nodes?.selected}
  {@const label = fmt.splitFlag(item.name)}
  <button class="row" class:selected onclick={() => pick(item)} disabled={!!switching}>
    <span class="lead">
      {#if item.auto}
        <Icon name="bolt" size={16} />
      {:else if label.flag}
        <span class="flag">{label.flag}</span>
      {:else}
        <Icon name="globe" size={16} />
      {/if}
    </span>
    <span class="name">
      <span>{item.auto ? item.name : (BUILTIN[item.name] ?? label.label)}</span>
      {#if item.auto && item.resolved}
        <small>当前：{item.resolved}</small>
      {/if}
    </span>
    {#if switching === item.name}
      <span class="spinner"></span>
    {:else}
      <span class="delay num {fmt.delayTone(item.delay)}">
        {item.delay ? `${item.delay} ms` : testing ? "…" : "—"}
      </span>
    {/if}
    <span class="tick"><Icon name="check" size={16} stroke={2.6} /></span>
  </button>
{/snippet}

<div class="screen">
  <TitleBar title="选择节点" onBack={() => app.back()} />

  <div class="toolbar">
    <span class="group">{nodes?.group ?? ""}</span>
    <button class="test" onclick={test} disabled={testing || !nodes?.group}>
      <span class:spin={testing}><Icon name="refresh" size={14} /></span>
      {testing ? "测速中" : "测速"}
    </button>
  </div>

  <div class="list">
    {#if !nodes}
      {#each { length: 6 } as _}
        <div class="row skeleton"></div>
      {/each}
    {:else if nodes.items.length === 0}
      <p class="empty">这个订阅没有可选的节点</p>
    {:else}
      {#if autos.length}
        <div class="section">
          {#each autos as item (item.name)}{@render row(item)}{/each}
        </div>
      {/if}
      {#if manual.length}
        <div class="caption">手动选择</div>
        <div class="section">
          {#each manual as item (item.name)}{@render row(item)}{/each}
        </div>
      {/if}
    {/if}
  </div>
</div>

<style>
  .screen {
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  .toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 20px 10px;
  }
  .group {
    font-size: 12px;
    color: var(--text-3);
  }
  .test {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    border-radius: 999px;
    font-size: 12.5px;
    color: var(--accent);
    background: var(--accent-soft);
  }
  .test:disabled {
    opacity: 0.7;
  }
  .spin {
    display: grid;
    animation: spin 0.9s linear infinite;
  }

  .list {
    flex: 1;
    overflow-y: auto;
    padding: 0 16px 20px 20px;
    scrollbar-gutter: stable;
  }
  .caption {
    margin: 18px 4px 8px;
    font-size: 12px;
    color: var(--text-3);
  }
  .section {
    border-radius: var(--radius);
    background: var(--surface);
    box-shadow: var(--shadow);
    overflow: hidden;
  }
  .row {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 14px;
    text-align: left;
    transition: background 0.15s;
  }
  .row + .row {
    box-shadow: inset 0 1px 0 var(--line);
  }
  .row:not(:disabled):hover {
    background: var(--surface-2);
  }
  .lead {
    width: 24px;
    display: grid;
    place-items: center;
    color: var(--accent);
  }
  .flag {
    font-size: 18px;
    line-height: 1;
  }
  .name {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .name span,
  .name small {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .name small {
    font-size: 11.5px;
    color: var(--text-3);
    margin-top: 1px;
  }
  .selected .name span {
    font-weight: 600;
    color: var(--accent);
  }
  .delay {
    font-size: 12px;
    color: var(--text-3);
  }
  .delay.good {
    color: var(--good);
  }
  .delay.ok {
    color: var(--ok);
  }
  .delay.bad {
    color: var(--bad);
  }
  .tick {
    width: 16px;
    display: grid;
    color: var(--accent);
    opacity: 0;
    transform: scale(0.6);
    transition: opacity 0.2s, transform 0.2s var(--ease);
  }
  .selected .tick {
    opacity: 1;
    transform: scale(1);
  }
  .skeleton {
    height: 46px;
    margin-bottom: 1px;
    background: var(--surface);
    animation: pulse 1.2s ease-in-out infinite;
  }
  .empty {
    text-align: center;
    color: var(--text-3);
    margin-top: 60px;
  }
  .spinner {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    border: 2px solid var(--accent-soft);
    border-top-color: var(--accent);
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  @keyframes pulse {
    50% {
      opacity: 0.5;
    }
  }
</style>
