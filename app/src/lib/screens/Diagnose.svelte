<script lang="ts">
  import { fade } from "svelte/transition";
  import { api, type Finding, type Fix } from "$lib/api";
  import Icon from "$lib/components/Icon.svelte";
  import TitleBar from "$lib/components/TitleBar.svelte";
  import { t } from "$lib/i18n.svelte";
  import { app } from "$lib/store.svelte";

  let findings = $state<Finding[] | null>(null);
  let running = $state(false);
  let fixing = $state<Fix | null>(null);

  const FIX_LABEL: Record<Fix, string> = $derived({
    reconnect: t("重新连接", "Reconnect"),
    "reapply-system-proxy": t("恢复代理设置", "Restore proxy settings"),
    "restore-system-proxy": t("关闭失效的代理", "Turn off broken proxy"),
    "flush-dns": t("清理 DNS 缓存", "Flush DNS cache"),
    "enable-tun": t("开启增强模式", "Turn on Enhanced Mode"),
    "enable-tun-ipv6": t("一键开启 IPv6 接管", "Route IPv6 too"),
  });

  async function run() {
    running = true;
    findings = await app.run(() => api.runDiagnostics());
    running = false;
  }

  async function fix(kind: Fix) {
    fixing = kind;
    const ok = await app.run(() => api.applyFix(kind));
    fixing = null;
    if (ok) {
      app.showToast(t("已修复，正在重新检测", "Fixed. Checking again…"));
      await run();
    }
  }

  $effect(() => {
    run();
  });

  const summary = $derived.by(() => {
    if (!findings) return null;
    const errors = findings.filter((f) => f.level === "error").length;
    const warns = findings.filter((f) => f.level === "warn").length;
    if (errors)
      return {
        tone: "error",
        text: t(`发现 ${errors} 个问题`, `Found ${errors} problem${errors === 1 ? "" : "s"}`),
      };
    if (warns)
      return {
        tone: "warn",
        text: t(`有 ${warns} 项需要注意`, `${warns} item${warns === 1 ? " needs" : "s need"} attention`),
      };
    return { tone: "ok", text: t("一切正常", "All good") };
  });
</script>

<div class="screen">
  <TitleBar title={t("网络诊断", "Diagnostics")} onBack={() => app.back()} />

  <div class="scroll">
    <div class="head">
      {#if running}
        <span class="pulse"></span>
        <span class="head-text">{t("正在检查网络、路由和 DNS…", "Checking network, routing, and DNS…")}</span>
      {:else if summary}
        <span class="badge {summary.tone}">
          <Icon name={summary.tone === "ok" ? "check" : "alert"} size={16} stroke={2.4} />
        </span>
        <span class="head-text">{summary.text}</span>
        <button class="again" onclick={run}>{t("重新检测", "Check again")}</button>
      {/if}
    </div>

    {#if findings && !running}
      <div class="list" in:fade={{ duration: 200 }}>
        {#each findings as finding (finding.id)}
          <article class={finding.level}>
            <span class="mark"></span>
            <div class="body">
              <h3>{finding.title}</h3>
              <p>{finding.detail}</p>
              {#if finding.fix}
                <button class="fix" onclick={() => fix(finding.fix!)} disabled={!!fixing}>
                  {fixing === finding.fix ? t("正在处理…", "Working…") : FIX_LABEL[finding.fix]}
                </button>
              {/if}
            </div>
          </article>
        {/each}
      </div>
      <p class="foot">
        {t(
          "诊断只读取网络状态，不会改动任何设置；只有点击修复按钮才会执行操作。",
          "Diagnostics only read network state and change nothing. Actions run only when you tap a fix button.",
        )}
      </p>
    {/if}
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
    padding: 0 16px 20px 20px;
    scrollbar-gutter: stable;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 44px;
    padding: 0 4px 12px;
  }
  .head-text {
    flex: 1;
    font-size: 15px;
    font-weight: 600;
  }
  .pulse {
    width: 12px;
    height: 12px;
    margin: 0 4px;
    border-radius: 50%;
    background: var(--accent);
    animation: pulse 1.1s ease-in-out infinite;
  }
  .badge {
    width: 28px;
    height: 28px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    color: #fff;
  }
  .badge.ok {
    background: var(--good);
  }
  .badge.warn {
    background: var(--ok);
  }
  .badge.error {
    background: var(--bad);
  }
  .again {
    padding: 6px 12px;
    border-radius: 999px;
    font-size: 12px;
    font-weight: 600;
    color: var(--accent);
    background: var(--accent-soft);
  }

  .list {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  article {
    display: flex;
    gap: 12px;
    padding: 14px;
    border-radius: var(--radius);
    background: var(--surface);
    box-shadow: var(--shadow);
  }
  .mark {
    width: 8px;
    height: 8px;
    margin-top: 6px;
    border-radius: 50%;
    flex-shrink: 0;
    background: var(--text-3);
  }
  .ok .mark {
    background: var(--good);
  }
  .info .mark {
    background: var(--accent);
  }
  .warn .mark {
    background: var(--ok);
  }
  .error .mark {
    background: var(--bad);
  }
  .body {
    flex: 1;
    min-width: 0;
  }
  h3 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
  }
  p {
    margin: 4px 0 0;
    font-size: 12.5px;
    line-height: 1.6;
    color: var(--text-2);
    user-select: text;
  }
  .fix {
    margin-top: 10px;
    padding: 6px 14px;
    border-radius: 10px;
    font-size: 12.5px;
    font-weight: 600;
    color: var(--on-accent);
    background: linear-gradient(145deg, var(--accent-2), var(--accent));
    transition: opacity 0.2s;
  }
  .fix:disabled {
    opacity: 0.6;
  }
  .foot {
    margin: 14px 4px 0;
    font-size: 11.5px;
    color: var(--text-3);
  }
  @keyframes pulse {
    50% {
      transform: scale(0.6);
      opacity: 0.5;
    }
  }
</style>
