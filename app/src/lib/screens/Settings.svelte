<script lang="ts">
  import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
  import { api, type CoreUpdateInfo, type RoutingMode } from "$lib/api";
  import Group from "$lib/components/Group.svelte";
  import Row from "$lib/components/Row.svelte";
  import Segmented from "$lib/components/Segmented.svelte";
  import Switch from "$lib/components/Switch.svelte";
  import TitleBar from "$lib/components/TitleBar.svelte";
  import { app, type Theme } from "$lib/store.svelte";

  const view = $derived(app.view!);
  let launchAtLogin = $state(false);
  let update = $state<CoreUpdateInfo | null>(null);
  let updating = $state<"checking" | "downloading" | null>(null);

  $effect(() => {
    isEnabled()
      .then((on) => (launchAtLogin = on))
      .catch(() => {});
  });

  const TUN_HINT = "接管所有程序的流量，包括游戏和命令行工具。首次开启需要一次管理员授权。";
  const MODES: { value: RoutingMode; label: string }[] = [
    { value: "rule", label: "智能分流" },
    { value: "global", label: "全部代理" },
    { value: "direct", label: "全部直连" },
  ];
  const MODE_HINTS: Record<RoutingMode, string> = {
    rule: "按订阅规则分流：国内网站直连，其余走代理。推荐。",
    global: "所有流量都经过代理节点。",
    direct: "所有流量都不经过代理，相当于暂停使用。",
  };
  const THEMES: { value: Theme; label: string }[] = [
    { value: "system", label: "跟随系统" },
    { value: "light", label: "浅色" },
    { value: "dark", label: "深色" },
  ];

  async function setLaunchAtLogin(on: boolean) {
    const ok = await app.run(() => (on ? enable() : disable()));
    if (ok !== null) launchAtLogin = on;
  }

  async function checkUpdate() {
    updating = "checking";
    update = await app.run(() => api.checkCoreUpdate());
    updating = null;
    if (update && !update.available) app.showToast("已是最新版本");
  }

  async function installUpdate() {
    updating = "downloading";
    const result = await app.run(() => api.updateCore());
    updating = null;
    if (result) {
      update = null;
      app.showToast(`内核已更新到 ${result.coreVersion}`);
    }
  }

  const reconnectNote = $derived(
    view.phase === "connected" && view.connectedMode && view.connectedMode !== view.connectMode,
  );
</script>

<div class="screen">
  <TitleBar title="设置" onBack={() => app.back()} />

  <div class="scroll">
    <Group title="分流模式" footer={MODE_HINTS[view.routingMode]}>
      <div class="pad">
        <Segmented
          options={MODES}
          value={view.routingMode}
          onchange={(mode) => app.run(() => api.setRoutingMode(mode))}
        />
      </div>
    </Group>

    <Group title="启动">
      <Row label="开机时启动">
        {#snippet trailing()}
          <Switch label="开机时启动" checked={launchAtLogin} onchange={setLaunchAtLogin} />
        {/snippet}
      </Row>
      <Row label="打开后自动连接">
        {#snippet trailing()}
          <Switch
            label="打开后自动连接"
            checked={view.autoConnect}
            onchange={(on) => app.run(() => api.setAutoConnect(on))}
          />
        {/snippet}
      </Row>
    </Group>

    <Group title="订阅">
      <Row
        label="管理订阅"
        hint={`${view.profiles.length} 个订阅${view.profiles.some((p) => p.merged) ? `，已合并 ${view.profiles.filter((p) => p.merged).length} 个` : ""}`}
        onclick={() => app.go("profiles")}
      />
      <Row label="自动更新订阅" hint="按服务商建议的间隔，默认每 12 小时">
        {#snippet trailing()}
          <Switch
            label="自动更新订阅"
            checked={view.autoUpdateSubscriptions}
            onchange={(on) => app.run(() => api.setAutoUpdateSubscriptions(on))}
          />
        {/snippet}
      </Row>
    </Group>

    <Group title="外观">
      <div class="pad">
        <Segmented options={THEMES} value={app.theme} onchange={(theme) => app.setTheme(theme)} />
      </div>
    </Group>

    <Group title="网络">
      <Row
        label="不走代理的地址"
        hint={view.bypass.length ? `${view.bypass.length} 条，局域网和内网域名已自动处理` : "局域网和内网域名已自动处理"}
        onclick={() => app.go("bypass")}
      />
      <Row label="网络诊断" hint="上不了网时，检查是网络、路由还是节点的问题" onclick={() => app.go("diagnose")} />
    </Group>

    <Group title="高级" footer="这些选项面向熟悉 Clash 配置的用户。">
      <Row label="增强模式" hint={TUN_HINT}>
        {#snippet trailing()}
          <Switch
            label="增强模式"
            checked={view.connectMode === "tun"}
            onchange={(on) => app.run(() => api.setConnectMode(on ? "tun" : "system-proxy"))}
          />
        {/snippet}
      </Row>
      {#if reconnectNote}<p class="note">将在下次连接时生效</p>{/if}
      {#if view.connectMode === "tun"}
        <Row label="同时接管 IPv6" hint="网络有 IPv6 时开启，避免支持 IPv6 的网站绕过代理">
          {#snippet trailing()}
            <Switch
              label="同时接管 IPv6"
              checked={view.tunIpv6}
              onchange={(on) => app.run(() => api.setTunIpv6(on))}
            />
          {/snippet}
        </Row>
      {/if}
      <Row
        label="网站规则"
        hint={view.customRules.length ? `${view.customRules.length} 条规则` : "指定网站走代理、直连或拦截"}
        onclick={() => app.go("rules")}
      />
      <Row label="自定义配置" hint="在订阅之上修改任意 Clash 设置" onclick={() => app.go("advanced")} />
      <Row label="运行日志" onclick={() => app.go("logs")} />
    </Group>

    <Group title="关于">
      <Row label="Choclite">
        {#snippet trailing()}<span class="muted num">{view.appVersion}</span>{/snippet}
      </Row>
      <Row
        label="clash-rs 内核"
        hint={update?.available ? `新版本 ${update.latest} 可用` : undefined}
      >
        {#snippet trailing()}
          <span class="muted num">{view.coreVersion ?? "未安装"}</span>
          {#if update?.available}
            <button class="mini" onclick={installUpdate} disabled={!!updating}>
              {updating === "downloading" ? "更新中…" : "更新"}
            </button>
          {:else}
            <button class="mini ghost" onclick={checkUpdate} disabled={!!updating}>
              {updating === "checking" ? "检查中…" : "检查更新"}
            </button>
          {/if}
        {/snippet}
      </Row>
      {#if view.port && view.connectedMode === "system-proxy"}
        <Row label="本地代理端口" hint="供其他软件手动设置代理使用">
          {#snippet trailing()}<span class="muted num">127.0.0.1:{view.port}</span>{/snippet}
        </Row>
      {/if}
    </Group>

    <Group>
      <Row label="退出 Choclite" danger onclick={() => api.quit()} />
    </Group>

    <p class="credit">国旗图标来自 Twemoji（CC-BY 4.0）</p>
  </div>
</div>

<style>
  .screen {
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  .scroll {
    flex: 1;
    overflow-y: auto;
    padding: 4px 16px 16px 20px;
    scrollbar-gutter: stable;
  }
  .pad {
    padding: 10px;
  }
  .note {
    margin: 0;
    padding: 0 14px 10px;
    font-size: 12px;
    color: var(--accent);
  }
  .muted {
    color: var(--text-3);
    font-size: 13px;
  }
  .mini {
    padding: 5px 10px;
    border-radius: 999px;
    font-size: 12px;
    font-weight: 600;
    color: var(--on-accent);
    background: var(--accent);
  }
  .mini.ghost {
    color: var(--accent);
    background: var(--accent-soft);
  }
  .mini:disabled {
    opacity: 0.6;
  }
  .credit {
    text-align: center;
    font-size: 11px;
    color: var(--text-3);
    margin: 4px 0 0;
  }
</style>
