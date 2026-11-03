<script lang="ts">
  import { api } from "$lib/api";
  import Icon from "$lib/components/Icon.svelte";
  import TitleBar from "$lib/components/TitleBar.svelte";
  import { isEnglish, t } from "$lib/i18n.svelte";
  import { app } from "$lib/store.svelte";

  const entries = $derived(app.view?.bypass ?? []);
  let value = $state("");
  let busy = $state(false);

  async function save(next: string[], done?: string) {
    busy = true;
    const result = await app.run(() => api.setBypass(next));
    busy = false;
    if (result && done) app.showToast(done);
    return result;
  }

  async function add(event: SubmitEvent) {
    event.preventDefault();
    const text = value.trim();
    if (!text || busy) return;
    // Allow pasting several at once, separated by spaces, commas or lines.
    const items = text.split(/[\s,，;；]+/).filter(Boolean);
    if (await save([...items, ...entries], t("已添加", "Added"))) value = "";
  }
</script>

<div class="screen">
  <TitleBar title={t("不走代理的地址", "Bypass list")} onBack={() => app.back()} />

  <div class="scroll">
    <p class="intro">
      {t(
        "这些地址始终直接连接，不经过代理。适合公司内网、家里的 NAS、路由器管理页，或者需要用真实 IP 访问的网站。",
        "These addresses always connect directly, never through the proxy. Useful for office intranets, a home NAS, router admin pages, or sites that need your real IP.",
      )}
    </p>

    <form class="card" onsubmit={add}>
      <label class="field">
        <input
          bind:value
          placeholder={t("域名或 IP 段，例如 nas.example.com", "Domain or IP range, e.g. nas.example.com")}
          spellcheck="false"
          autocomplete="off"
          disabled={busy}
        />
      </label>
      <button class="primary" type="submit" disabled={!value.trim() || busy}>
        <Icon name="plus" size={16} /> {t("添加", "Add")}
      </button>
    </form>

    {#if entries.length}
      <div class="list">
        {#each entries as entry (entry)}
          <div class="row">
            <span class="value">{entry}</span>
            <button
              class="del"
              onclick={() => save(entries.filter((e) => e !== entry))}
              disabled={busy}
              aria-label={t("删除", "Delete")}
            >
              <Icon name="trash" size={15} />
            </button>
          </div>
        {/each}
      </div>
    {/if}

    <div class="auto">
      <h3>{t("已自动处理", "Handled automatically")}</h3>
      {#if isEnglish()}
        <ul>
          <li>LAN addresses (192.168.x.x, 10.x.x.x, etc.) and Tailscale ranges</li>
          <li>Local domains such as <code>.lan</code>, <code>.local</code>, and <code>.home.arpa</code></li>
          <li>DNS suffixes from your network settings (usually set by office networks or VPNs)</li>
        </ul>
        <p>
          In Enhanced Mode, these domains are resolved by your computer's own DNS, so local names keep working. A
          domain also covers all its subdomains.
        </p>
      {:else}
        <ul>
          <li>局域网地址（192.168.x.x、10.x.x.x 等）和 Tailscale 网段</li>
          <li><code>.lan</code>、<code>.local</code>、<code>.home.arpa</code> 等内网域名</li>
          <li>电脑网络设置里的 DNS 后缀（公司网络、VPN 通常会自动设置）</li>
        </ul>
        <p>增强模式下，这些域名会交给电脑原本的 DNS 解析，内网名字照常能打开。填一个域名会同时包含它的所有子域名。</p>
      {/if}
    </div>
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
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .intro {
    margin: 0 4px;
    font-size: 12.5px;
    line-height: 1.6;
    color: var(--text-2);
  }
  .card {
    display: flex;
    gap: 8px;
    padding: 10px;
    border-radius: var(--radius);
    background: var(--surface);
    box-shadow: var(--shadow);
  }
  .field {
    flex: 1;
    display: flex;
    align-items: center;
    padding: 0 12px;
    border-radius: 11px;
    background: var(--surface-2);
    transition: box-shadow 0.2s;
  }
  .field:focus-within {
    box-shadow: inset 0 0 0 2px var(--accent);
  }
  input {
    flex: 1;
    min-width: 0;
    height: 38px;
    border: 0;
    outline: none;
    background: none;
  }
  input::placeholder {
    color: var(--text-3);
  }
  .primary {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 0 14px;
    border-radius: 11px;
    font-weight: 600;
    background: linear-gradient(145deg, var(--accent-2), var(--accent));
    color: var(--on-accent);
    transition: opacity 0.2s;
  }
  .primary:disabled {
    opacity: 0.45;
  }
  .list {
    border-radius: var(--radius);
    background: var(--surface);
    box-shadow: var(--shadow);
    overflow: hidden;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 8px 8px 14px;
  }
  .row + .row {
    box-shadow: inset 0 1px 0 var(--line);
  }
  .value {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    user-select: text;
  }
  .del {
    width: 30px;
    height: 30px;
    border-radius: 8px;
    display: grid;
    place-items: center;
    color: var(--text-3);
  }
  .del:hover {
    background: var(--surface-2);
    color: var(--bad);
  }
  .auto {
    padding: 12px 14px;
    border-radius: var(--radius);
    background: var(--accent-soft);
    font-size: 12.5px;
    line-height: 1.6;
    color: var(--text-2);
  }
  .auto h3 {
    margin: 0 0 4px;
    font-size: 13px;
    color: var(--text);
  }
  .auto ul {
    margin: 0;
    padding-left: 18px;
  }
  .auto p {
    margin: 6px 0 0;
  }
  code {
    font-family: "Cascadia Code", Consolas, monospace;
    font-size: 11.5px;
  }
</style>
