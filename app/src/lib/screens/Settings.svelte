<script lang="ts">
  import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
  import { api, type RoutingMode } from "$lib/api";
  import Group from "$lib/components/Group.svelte";
  import Row from "$lib/components/Row.svelte";
  import Segmented from "$lib/components/Segmented.svelte";
  import Switch from "$lib/components/Switch.svelte";
  import TitleBar from "$lib/components/TitleBar.svelte";
  import { app } from "$lib/store.svelte";

  const view = $derived(app.view!);
  let launchAtLogin = $state(false);

  $effect(() => {
    isEnabled()
      .then((on) => (launchAtLogin = on))
      .catch(() => {});
  });

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

  async function setLaunchAtLogin(on: boolean) {
    const ok = await app.run(() => (on ? enable() : disable()));
    if (ok !== null) launchAtLogin = on;
  }
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
      <Row label="管理订阅" hint="{view.profiles.length} 个订阅" onclick={() => app.go("profiles")} />
    </Group>

    <Group title="高级" footer="这些选项面向熟悉 Clash 配置的用户。">
      <Row label="自定义配置" hint="在订阅之上追加规则或修改设置" onclick={() => app.go("advanced")} />
      <Row label="运行日志" onclick={() => app.go("logs")} />
    </Group>

    <Group title="关于">
      <Row label="版本">
        {#snippet trailing()}<span class="muted num">0.1.0</span>{/snippet}
      </Row>
      {#if view.port}
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
  .muted {
    color: var(--text-3);
    font-size: 13px;
  }
  .credit {
    text-align: center;
    font-size: 11px;
    color: var(--text-3);
    margin: 4px 0 0;
  }
</style>
