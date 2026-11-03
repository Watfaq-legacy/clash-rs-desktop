<script lang="ts">
  import { api } from "$lib/api";
  import TitleBar from "$lib/components/TitleBar.svelte";
  import { t } from "$lib/i18n.svelte";
  import { app } from "$lib/store.svelte";

  let lines = $state<string[]>([]);
  let box = $state<HTMLDivElement>();

  $effect(() => {
    const load = async () => {
      const next = await api.coreLogs().catch(() => null);
      if (!next) return;
      const atBottom = !box || box.scrollHeight - box.scrollTop - box.clientHeight < 24;
      lines = next;
      // Follow new output only if the user hasn't scrolled up.
      if (atBottom) requestAnimationFrame(() => box?.scrollTo({ top: box.scrollHeight }));
    };
    load();
    const timer = setInterval(load, 2000);
    return () => clearInterval(timer);
  });

  function level(line: string) {
    if (/\bERROR\b/.test(line)) return "error";
    if (/\bWARN\b/.test(line)) return "warn";
    return "";
  }
</script>

<div class="screen">
  <TitleBar title={t("运行日志", "Logs")} onBack={() => app.back()} />
  <div class="logs" bind:this={box}>
    {#if lines.length === 0}
      <p class="empty">
        {app.view?.phase === "connected"
          ? t("暂无日志", "No logs yet")
          : t("连接后这里会显示内核日志", "Core logs appear here after you connect")}
      </p>
    {:else}
      {#each lines as line, i (i)}
        <div class="line {level(line)}" class:app={line.includes("[Choclite]")}>{line}</div>
      {/each}
    {/if}
  </div>
  <div class="bar">
    <span>{t("日志会保存到文件，最多保留约 6 MB", "Logs are saved to disk, up to about 6 MB")}</span>
    <button onclick={() => app.run(() => api.openLogsDir())}>{t("打开日志文件夹", "Open folder")}</button>
  </div>
</div>

<style>
  .screen {
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  .logs {
    flex: 1;
    margin: 0 16px 16px;
    padding: 10px 12px;
    overflow: auto;
    border-radius: var(--radius);
    background: var(--surface);
    box-shadow: var(--shadow);
    font-family: "Cascadia Code", Consolas, monospace;
    font-size: 11px;
    line-height: 1.55;
    user-select: text;
  }
  .line {
    white-space: pre-wrap;
    word-break: break-all;
    color: var(--text-2);
  }
  .line.warn {
    color: var(--ok);
  }
  .line.app {
    color: var(--accent);
  }
  .bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    margin: -4px 16px 14px;
    font-size: 12px;
    color: var(--text-3);
  }
  .bar button {
    padding: 6px 12px;
    border-radius: 999px;
    font-size: 12px;
    font-weight: 600;
    color: var(--accent);
    background: var(--accent-soft);
  }
  .line.error {
    color: var(--bad);
  }
  .empty {
    text-align: center;
    margin-top: 60px;
    color: var(--text-3);
    font-family: var(--font);
    font-size: 13px;
  }
</style>
