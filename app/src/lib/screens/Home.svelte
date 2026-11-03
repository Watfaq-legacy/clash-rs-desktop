<script lang="ts">
  import { api, type Nodes } from "$lib/api";
  import Icon from "$lib/components/Icon.svelte";
  import { fly } from "svelte/transition";
  import PowerButton from "$lib/components/PowerButton.svelte";
  import Sheet from "$lib/components/Sheet.svelte";
  import TitleBar from "$lib/components/TitleBar.svelte";
  import * as fmt from "$lib/format";
  import { isEnglish, t } from "$lib/i18n.svelte";
  import { app } from "$lib/store.svelte";

  const view = $derived(app.view!);
  const phase = $derived(view.phase);
  const profile = $derived(app.activeProfile);

  let now = $state(Date.now());
  let nodes = $state<Nodes | null>(null);

  // Tick the connected timer.
  $effect(() => {
    if (phase !== "connected") return;
    const timer = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(timer);
  });

  // Fetch the current node when connected; refresh periodically since
  // automatic groups can switch on their own.
  $effect(() => {
    // Global mode reads a different group, so refetch when the mode changes.
    void view.routingMode;
    if (phase !== "connected") {
      nodes = null;
      return;
    }
    // Right after connecting the controller may not answer yet; retry
    // quickly until the first list arrives instead of waiting a full period.
    let retry: ReturnType<typeof setTimeout> | undefined;
    const load = (attempt = 0) =>
      api
        .getNodes()
        .then((n) => (nodes = n))
        .catch(() => {})
        .finally(() => {
          if (!nodes?.selected && attempt < 10) retry = setTimeout(() => load(attempt + 1), 1000);
        });
    load();
    const timer = setInterval(load, 15_000);
    return () => {
      clearInterval(timer);
      clearTimeout(retry);
    };
  });

  const status = $derived.by(() => {
    switch (phase) {
      case "connecting":
        return { title: t("正在连接", "Connecting"), sub: view.step ?? t("马上就好…", "Almost there…") };
      case "disconnecting":
        return {
          title: t("正在断开", "Disconnecting"),
          sub: t("正在恢复网络设置…", "Restoring network settings…"),
        };
      case "connected":
        return {
          title: t("已连接", "Connected"),
          sub: view.connectedSince ? fmt.duration(now - view.connectedSince) : "",
        };
      default:
        return { title: t("未连接", "Disconnected"), sub: t("点击按钮开始使用", "Tap the button to connect") };
    }
  });

  const selectedNode = $derived.by(() => {
    if (!nodes?.selected) return null;
    const item = nodes.items.find((i) => i.name === nodes!.selected);
    return {
      auto: item?.auto ?? false,
      name: nodes.resolved ?? nodes.selected,
      delay: nodes.resolvedDelay,
    };
  });

  const usage = $derived.by(() => {
    const info = profile?.subscription;
    if (!info || !info.total) return null;
    const used = info.upload + info.download;
    return { used, total: info.total, ratio: Math.min(1, used / info.total) };
  });

  const expiry = $derived.by(() => {
    const expire = profile?.subscription?.expire;
    if (!expire) return null;
    const days = fmt.daysLeft(expire);
    return { days, text: fmt.expiryText(days) };
  });

  async function toggle() {
    await app.run(() => api.toggle());
  }

  const mergedCount = $derived(view.profiles.filter((p) => p.merged).length);

  // Only after the connection has been up a minute: never greet the user
  // with a warning the moment they connect.
  const showLeaks = $derived(
    phase === "connected" &&
      view.connectedMode === "system-proxy" &&
      view.leaks.length > 0 &&
      !!view.connectedSince &&
      now - view.connectedSince > 60_000,
  );
  let leakSheet = $state(false);
  let leakBusy = $state(false);

  async function useTun() {
    leakBusy = true;
    await app.run(() => api.setConnectMode("tun"));
    leakBusy = false;
    leakSheet = false;
  }

  async function ignoreAll() {
    leakBusy = true;
    await app.run(async () => {
      for (const leak of view.leaks) await api.ignoreLeak(leak);
    });
    leakBusy = false;
    leakSheet = false;
    app.showToast(t("以后不会再提示这些程序", "You won't be reminded about these apps again"));
  }
</script>

<div class="screen">
  <TitleBar onSettings={() => app.go("settings")} />

  <main>
    <section class="hero">
      <div class="status" class:on={phase === "connected"}>
        <h1>{status.title}</h1>
        <p class="num">
          {#if phase === "connected" && view.connectedMode === "tun"}
            <span class="chip" title={t("所有程序的流量都经过 Choclite", "All app traffic goes through Choclite")}>
              {t("增强模式", "Enhanced Mode")}
            </span>
          {/if}
          {status.sub}
        </p>
      </div>

      <PowerButton {phase} onclick={toggle} />

      <div class="speeds num" class:hidden={phase !== "connected"}>
        <span><Icon name="up" size={14} />{fmt.speed(app.traffic.up)}</span>
        <span class="divider"></span>
        <span><Icon name="down" size={14} />{fmt.speed(app.traffic.down)}</span>
      </div>
    </section>

    <section class="cards">
      {#if phase === "connected" && view.healthIssue}
        <button class="notice issue" onclick={() => app.go("diagnose")} in:fly={{ y: 8, duration: 400 }}>
          <span class="dot"></span>
          <span class="notice-text">{t(`${view.healthIssue}，点此诊断`, `${view.healthIssue}. Tap to diagnose`)}</span>
          <span class="chev"><Icon name="chevron" size={14} /></span>
        </button>
      {:else if showLeaks}
        <button class="notice" onclick={() => (leakSheet = true)} in:fly={{ y: 8, duration: 400 }}>
          <span class="dot"></span>
          <span class="notice-text">{t(
            `${view.leaks.length} 个程序未经过代理`,
            `${view.leaks.length} app${view.leaks.length === 1 ? " is" : "s are"} bypassing the proxy`,
          )}</span>
          <span class="chev"><Icon name="chevron" size={14} /></span>
        </button>
      {/if}
      {#if view.routingMode === "direct"}
      <!-- No traffic reaches a node in this mode; showing one would mislead. -->
      <button class="card" onclick={() => app.go("settings")}>
        <span class="badge"><Icon name="globe" size={17} /></span>
        <span class="body">
          <span class="label">{t("分流模式", "Routing mode")}</span>
          <span class="value">{t("全部直连", "Direct all")}</span>
          <span class="meta">{t("不经过代理节点，可在设置中更改", "No traffic uses a server. Change in Settings")}</span>
        </span>
        <span class="chev"><Icon name="chevron" size={16} /></span>
      </button>
      {:else}
      <button
        class="card"
        onclick={() => app.go("nodes")}
        disabled={phase !== "connected"}
        title={phase === "connected" ? "" : t("连接后可以切换节点", "Connect to switch servers")}
      >
        <span class="badge"><Icon name={selectedNode?.auto ? "bolt" : "globe"} size={17} /></span>
        <span class="body">
          <span class="label">{selectedNode?.auto ? t("自动选择", "Auto select") : t("当前节点", "Server")}</span>
          <span class="value">
            {#if selectedNode}
              {selectedNode.name}
            {:else if phase === "connected"}
              …
            {:else}
              {t("连接后显示当前节点", "Shown after connecting")}
            {/if}
          </span>
        </span>
        {#if selectedNode?.delay}
          <span class="delay num {fmt.delayTone(selectedNode.delay)}">{selectedNode.delay} ms</span>
        {/if}
        <span class="chev"><Icon name="chevron" size={16} /></span>
      </button>
      {/if}

      <button class="card" onclick={() => app.go("profiles")}>
        <span class="badge"><Icon name="layers" size={17} /></span>
        <span class="body">
          <span class="label">{t("订阅", "Subscription")}</span>
          <span class="value">
            {profile?.name ?? t("未选择", "None selected")}
            {#if mergedCount}<span class="merged">{t(`+ ${mergedCount} 个合并订阅`, `+ ${mergedCount} merged`)}</span>{/if}
          </span>
          {#if usage}
            <span class="meter"><span style="width: {usage.ratio * 100}%"></span></span>
            <span class="meta num">
              {fmt.bytes(usage.used)} / {fmt.bytes(usage.total, 0)}
              {#if expiry}<span class:warn={expiry.days <= 7}> · {expiry.text}</span>{/if}
            </span>
          {:else if expiry}
            <span class="meta" class:warn={expiry.days <= 7}>{expiry.text}</span>
          {:else if profile?.updatedAt}
            <span class="meta">{t(`更新于 ${fmt.relative(profile.updatedAt)}`, `Updated ${fmt.relative(profile.updatedAt)}`)}</span>
          {/if}
        </span>
        <span class="chev"><Icon name="chevron" size={16} /></span>
      </button>
    </section>
  </main>
</div>

{#if leakSheet}
  <Sheet title={t("这些程序没有经过代理", "These apps bypass the proxy")} onclose={() => (leakSheet = false)}>
    <p>
      {t(
        "它们会直接连接网络，不使用系统代理设置。常见的有游戏、启动器和命令行工具。",
        "They connect directly and ignore the system proxy. Common examples are games, launchers, and command-line tools.",
      )}
    </p>
    <ul class="apps">
      {#each view.leaks as leak (leak)}
        <li>{leak.replace(/\.exe$/i, "")}</li>
      {/each}
    </ul>
    {#if isEnglish()}
      <p>
        To route them through the proxy too, turn on <b>Enhanced Mode</b> (requires administrator
        approval once). If they don't need the proxy, just ignore them.
      </p>
    {:else}
      <p>如果希望它们也经过代理，可以开启<b>增强模式</b>（需要一次管理员授权）。如果它们本来就不需要代理，忽略即可。</p>
    {/if}
    {#snippet actions()}
      <button class="ghost" onclick={ignoreAll} disabled={leakBusy}>{t("忽略这些程序", "Ignore these apps")}</button>
      <button class="primary" onclick={useTun} disabled={leakBusy}>{t("开启增强模式", "Turn on Enhanced Mode")}</button>
    {/snippet}
  </Sheet>
{/if}

<style>
  .screen {
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  main {
    flex: 1;
    display: flex;
    flex-direction: column;
    padding: 0 20px 20px;
  }

  .hero {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 26px;
  }

  .status {
    text-align: center;
  }
  h1 {
    margin: 0;
    font-size: 26px;
    font-weight: 700;
    letter-spacing: 0.5px;
    transition: color 0.4s;
  }
  .status.on h1 {
    color: var(--accent);
  }
  .status p {
    margin: 6px 0 0;
    min-height: 20px;
    color: var(--text-2);
    font-size: 13px;
  }

  .chip {
    display: inline-block;
    margin-right: 6px;
    padding: 1px 8px;
    border-radius: 999px;
    font-size: 11.5px;
    font-weight: 600;
    color: var(--accent);
    background: var(--accent-soft);
    vertical-align: 1px;
  }

  .speeds {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 8px 16px;
    border-radius: 999px;
    background: var(--surface);
    box-shadow: inset 0 0 0 1px var(--line);
    color: var(--text-2);
    font-size: 12.5px;
    transition: opacity 0.4s var(--ease), transform 0.4s var(--ease);
  }
  .speeds span {
    display: flex;
    align-items: center;
    gap: 5px;
    min-width: 92px;
    justify-content: center;
  }
  .speeds .divider {
    min-width: 0;
    width: 1px;
    height: 12px;
    background: var(--line);
  }
  .speeds.hidden {
    opacity: 0;
    transform: translateY(6px);
  }

  .cards {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  /* Quiet, card-styled hint; details live in a sheet. */
  .notice {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 14px;
    border-radius: 12px;
    background: var(--surface);
    box-shadow: inset 0 0 0 1px var(--line);
    font-size: 12.5px;
    color: var(--text-2);
    text-align: left;
    transition: background 0.15s, color 0.15s;
  }
  .notice:hover {
    background: var(--surface-2);
    color: var(--text);
  }
  .notice .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--ok);
    flex-shrink: 0;
  }
  .notice-text {
    flex: 1;
  }
  .merged {
    margin-left: 6px;
    font-size: 12px;
    font-weight: 500;
    color: var(--accent);
  }
  .notice.issue .dot {
    background: var(--bad);
  }
  .notice.issue {
    color: var(--text);
  }
  .apps {
    margin: 4px 0 10px;
    padding: 0;
    list-style: none;
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .apps li {
    padding: 4px 10px;
    border-radius: 999px;
    background: var(--surface-2);
    color: var(--text);
    font-size: 12.5px;
  }
  .card {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 14px;
    border-radius: var(--radius);
    background: var(--surface);
    box-shadow: var(--shadow);
    text-align: left;
    transition: transform 0.18s var(--ease), box-shadow 0.18s;
  }
  .card:not(:disabled):hover {
    transform: translateY(-1px);
  }
  .card:not(:disabled):active {
    transform: translateY(0) scale(0.99);
  }
  .card:disabled .chev {
    opacity: 0;
  }
  .badge {
    width: 36px;
    height: 36px;
    border-radius: 11px;
    display: grid;
    place-items: center;
    flex-shrink: 0;
    color: var(--accent);
    background: var(--accent-soft);
  }
  .body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .label {
    font-size: 12px;
    color: var(--text-3);
  }
  .value {
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .meter {
    height: 4px;
    margin: 6px 0 3px;
    border-radius: 2px;
    background: var(--surface-2);
    overflow: hidden;
  }
  .meter span {
    display: block;
    height: 100%;
    border-radius: 2px;
    background: linear-gradient(90deg, var(--accent-2), var(--accent));
  }
  .meta {
    font-size: 12px;
    color: var(--text-2);
  }
  .warn {
    color: var(--bad);
  }
  .chev {
    color: var(--text-3);
    display: grid;
  }

  .delay {
    font-size: 12px;
    padding: 3px 8px;
    border-radius: 999px;
    background: var(--surface-2);
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
</style>
