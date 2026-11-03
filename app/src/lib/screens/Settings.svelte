<script lang="ts">
  import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
  import { api, type CoreUpdateInfo, type Language, type RoutingMode } from "$lib/api";
  import { t } from "$lib/i18n.svelte";
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

  // $derived so the labels follow a language switch.
  const TUN_HINT = $derived(
    t(
      "接管所有程序的流量，包括游戏和命令行工具。首次开启需要一次管理员授权。",
      "Routes traffic from every app, including games and command-line tools. Needs admin approval the first time.",
    ),
  );
  const MODES: { value: RoutingMode; label: string }[] = $derived([
    { value: "rule", label: t("智能分流", "Rule-based") },
    { value: "global", label: t("全部代理", "Proxy all") },
    { value: "direct", label: t("全部直连", "Direct all") },
  ]);
  const MODE_HINTS: Record<RoutingMode, string> = $derived({
    rule: t(
      "按订阅规则分流：国内网站直连，其余走代理。推荐。",
      "Follows the subscription's rules: domestic sites go direct, the rest via proxy. Recommended.",
    ),
    global: t("所有流量都经过代理节点。", "All traffic goes through the proxy server."),
    direct: t("所有流量都不经过代理，相当于暂停使用。", "No traffic uses the proxy, like pausing it."),
  });
  const THEMES: { value: Theme; label: string }[] = $derived([
    { value: "system", label: t("跟随系统", "System") },
    { value: "light", label: t("浅色", "Light") },
    { value: "dark", label: t("深色", "Dark") },
  ]);
  const LANGUAGES: { value: Language; label: string }[] = $derived([
    { value: "system", label: t("跟随系统", "System") },
    { value: "zh", label: "中文" },
    { value: "en", label: "English" },
  ]);

  async function setLanguage(language: Language) {
    await app.run(() => api.setLanguage(language));
  }

  async function setLaunchAtLogin(on: boolean) {
    const ok = await app.run(() => (on ? enable() : disable()));
    if (ok !== null) launchAtLogin = on;
  }

  async function checkUpdate() {
    updating = "checking";
    update = await app.run(() => api.checkCoreUpdate());
    updating = null;
    if (update && !update.available) app.showToast(t("已是最新版本", "You're up to date"));
  }

  async function installUpdate() {
    updating = "downloading";
    const result = await app.run(() => api.updateCore());
    updating = null;
    if (result) {
      update = null;
      app.showToast(t(`内核已更新到 ${result.coreVersion}`, `Core updated to ${result.coreVersion}`));
    }
  }

  const mergedCount = $derived(view.profiles.filter((p) => p.merged).length);

  const reconnectNote = $derived(
    view.phase === "connected" && view.connectedMode && view.connectedMode !== view.connectMode,
  );
</script>

<div class="screen">
  <TitleBar title={t("设置", "Settings")} onBack={() => app.back()} />

  <div class="scroll">
    <Group title={t("分流模式", "Routing mode")} footer={MODE_HINTS[view.routingMode]}>
      <div class="pad">
        <Segmented
          options={MODES}
          value={view.routingMode}
          onchange={(mode) => app.run(() => api.setRoutingMode(mode))}
        />
      </div>
    </Group>

    <Group title={t("启动", "Startup")}>
      <Row label={t("开机时启动", "Launch at login")}>
        {#snippet trailing()}
          <Switch label={t("开机时启动", "Launch at login")} checked={launchAtLogin} onchange={setLaunchAtLogin} />
        {/snippet}
      </Row>
      <Row label={t("打开后自动连接", "Connect on launch")}>
        {#snippet trailing()}
          <Switch
            label={t("打开后自动连接", "Connect on launch")}
            checked={view.autoConnect}
            onchange={(on) => app.run(() => api.setAutoConnect(on))}
          />
        {/snippet}
      </Row>
    </Group>

    <Group title={t("订阅", "Subscriptions")}>
      <Row
        label={t("管理订阅", "Manage subscriptions")}
        hint={t(
          `${view.profiles.length} 个订阅${mergedCount ? `，已合并 ${mergedCount} 个` : ""}`,
          `${view.profiles.length} subscription${view.profiles.length === 1 ? "" : "s"}${mergedCount ? `, ${mergedCount} merged` : ""}`,
        )}
        onclick={() => app.go("profiles")}
      />
      <Row
        label={t("自动更新订阅", "Auto-update")}
        hint={t("按服务商建议的间隔，默认每 12 小时", "At the provider's interval, every 12 hours by default")}
      >
        {#snippet trailing()}
          <Switch
            label={t("自动更新订阅", "Auto-update subscriptions")}
            checked={view.autoUpdateSubscriptions}
            onchange={(on) => app.run(() => api.setAutoUpdateSubscriptions(on))}
          />
        {/snippet}
      </Row>
    </Group>

    <Group title={t("外观", "Appearance")}>
      <div class="pad">
        <Segmented options={THEMES} value={app.theme} onchange={(theme) => app.setTheme(theme)} />
      </div>
    </Group>

    <Group title={t("语言", "Language")}>
      <div class="pad">
        <Segmented options={LANGUAGES} value={view.language} onchange={setLanguage} />
      </div>
    </Group>

    <Group title={t("网络", "Network")}>
      <Row
        label={t("不走代理的地址", "Bypass list")}
        hint={view.bypass.length
          ? t(
              `${view.bypass.length} 条，局域网和内网域名已自动处理`,
              `${view.bypass.length} entr${view.bypass.length === 1 ? "y" : "ies"}; LAN and local domains are automatic`,
            )
          : t("局域网和内网域名已自动处理", "LAN and local domains are handled automatically")}
        onclick={() => app.go("bypass")}
      />
      <Row
        label={t("网络诊断", "Diagnostics")}
        hint={t(
          "上不了网时，检查是网络、路由还是节点的问题",
          "Can't get online? Check the network, routing, or server",
        )}
        onclick={() => app.go("diagnose")}
      />
    </Group>

    <Group
      title={t("高级", "Advanced")}
      footer={t("这些选项面向熟悉 Clash 配置的用户。", "These options are for users familiar with Clash configuration.")}
    >
      <Row label={t("增强模式", "Enhanced Mode (TUN)")} hint={TUN_HINT}>
        {#snippet trailing()}
          <Switch
            label={t("增强模式", "Enhanced Mode (TUN)")}
            checked={view.connectMode === "tun"}
            onchange={(on) => app.run(() => api.setConnectMode(on ? "tun" : "system-proxy"))}
          />
        {/snippet}
      </Row>
      {#if reconnectNote}<p class="note">{t("将在下次连接时生效", "Takes effect next time you connect")}</p>{/if}
      {#if view.connectMode === "tun"}
        <Row
          label={t("同时接管 IPv6", "Also route IPv6")}
          hint={t(
            "网络有 IPv6 时开启，避免支持 IPv6 的网站绕过代理",
            "Turn on if your network has IPv6, so IPv6 sites don't bypass the proxy",
          )}
        >
          {#snippet trailing()}
            <Switch
              label={t("同时接管 IPv6", "Also route IPv6")}
              checked={view.tunIpv6}
              onchange={(on) => app.run(() => api.setTunIpv6(on))}
            />
          {/snippet}
        </Row>
      {/if}
      <Row
        label={t("网站规则", "Site rules")}
        hint={view.customRules.length
          ? t(
              `${view.customRules.length} 条规则`,
              `${view.customRules.length} rule${view.customRules.length === 1 ? "" : "s"}`,
            )
          : t("指定网站走代理、直连或拦截", "Send sites via proxy, direct, or block them")}
        onclick={() => app.go("rules")}
      />
      <Row
        label={t("自定义配置", "Custom configuration")}
        hint={t("在订阅之上修改任意 Clash 设置", "Override any Clash setting on top of subscriptions")}
        onclick={() => app.go("advanced")}
      />
      <Row label={t("运行日志", "Logs")} onclick={() => app.go("logs")} />
    </Group>

    <Group title={t("关于", "About")}>
      <Row label="Choclite">
        {#snippet trailing()}<span class="muted num">{view.appVersion}</span>{/snippet}
      </Row>
      <Row
        label={t("clash-rs 内核", "clash-rs core")}
        hint={update?.available
          ? t(`新版本 ${update.latest} 可用`, `Version ${update.latest} available`)
          : undefined}
      >
        {#snippet trailing()}
          <span class="muted num">{view.coreVersion ?? t("未安装", "Not installed")}</span>
          {#if update?.available}
            <button class="mini" onclick={installUpdate} disabled={!!updating}>
              {updating === "downloading" ? t("更新中…", "Updating…") : t("更新", "Update")}
            </button>
          {:else}
            <button class="mini ghost" onclick={checkUpdate} disabled={!!updating}>
              {updating === "checking" ? t("检查中…", "Checking…") : t("检查更新", "Check")}
            </button>
          {/if}
        {/snippet}
      </Row>
      {#if view.port && view.connectedMode === "system-proxy"}
        <Row
          label={t("本地代理端口", "Local proxy port")}
          hint={t("供其他软件手动设置代理使用", "For apps that need a manual proxy setting")}
        >
          {#snippet trailing()}<span class="muted num">127.0.0.1:{view.port}</span>{/snippet}
        </Row>
      {/if}
    </Group>

    <Group>
      <Row label={t("退出 Choclite", "Quit Choclite")} danger onclick={() => api.quit()} />
    </Group>

    <p class="credit">{t("国旗图标来自 Twemoji（CC-BY 4.0）", "Flag icons by Twemoji (CC-BY 4.0)")}</p>
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
