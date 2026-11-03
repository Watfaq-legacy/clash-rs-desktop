<script lang="ts">
  import { readText } from "@tauri-apps/plugin-clipboard-manager";
  import { open } from "@tauri-apps/plugin-dialog";
  import { api } from "$lib/api";
  import Icon from "$lib/components/Icon.svelte";
  import Sheet from "$lib/components/Sheet.svelte";
  import TitleBar from "$lib/components/TitleBar.svelte";
  import { isEnglish, t } from "$lib/i18n.svelte";
  import { app } from "$lib/store.svelte";

  let { onBack }: { onBack?: () => void } = $props();

  let url = $state("");
  let busy = $state(false);
  let help = $state(false);

  const NODE_LINK = /^(ss|trojan|vmess|vless|hysteria2|hy2):\/\//i;
  const accepted = (text: string) => /^https?:\/\/\S+$/i.test(text) || NODE_LINK.test(text);
  const valid = $derived(accepted(url.trim()));
  const isNodeLink = $derived(NODE_LINK.test(url.trim()));

  async function paste() {
    const text = (await readText().catch(() => ""))?.trim() ?? "";
    if (accepted(text)) url = text;
    else app.showToast(t("剪贴板里没有订阅链接或节点链接", "No subscription or server link in the clipboard"));
  }

  function done(message: string) {
    url = "";
    app.showToast(message);
    onBack?.();
  }

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    if (!valid || busy) return;
    busy = true;
    const view = await app.run(() => api.addSubscription(url.trim()));
    busy = false;
    if (view) done(isNodeLink ? t("节点已添加", "Server added") : t("订阅已添加", "Subscription added"));
  }

  async function importFile() {
    const path = await open({
      multiple: false,
      directory: false,
      title: t("选择配置文件", "Choose a config file"),
      filters: [
        { name: t("Clash 配置或节点列表", "Clash config or server list"), extensions: ["yaml", "yml", "txt", "conf"] },
      ],
    }).catch(() => null);
    if (typeof path !== "string" || busy) return;
    busy = true;
    const view = await app.run(() => api.importFile(path));
    busy = false;
    if (view) done(t("配置已导入", "Config imported"));
  }
</script>

<div class="screen">
  <TitleBar title={onBack ? t("添加订阅", "Add subscription") : ""} {onBack} />

  <main>
    {#if !onBack}
      <div class="hello">
        <img class="logo" src="/brand/logo.png" alt="" draggable="false" />
        <h1>{t("欢迎使用 Choclite", "Welcome to Choclite")}</h1>
        <p>{t("粘贴你的订阅链接，一键开始", "Paste your subscription link to get started")}</p>
      </div>
    {:else}
      <div class="hello compact">
        <p>{t("粘贴服务商提供的订阅链接，或者节点链接", "Paste a subscription link from your provider, or a server link")}</p>
      </div>
    {/if}

    <form onsubmit={submit}>
      <label class="field">
        <input
          bind:value={url}
          placeholder={t("订阅链接 https://… 或节点链接 ss://…", "Subscription https://… or server ss://…")}
          spellcheck="false"
          autocomplete="off"
          disabled={busy}
        />
        <button
          type="button"
          class="paste"
          onclick={paste}
          disabled={busy}
          title={t("从剪贴板粘贴", "Paste from clipboard")}
        >
          <Icon name="clipboard" size={16} />
        </button>
      </label>
      <button class="primary" type="submit" disabled={!valid || busy}>
        {#if busy}<span class="spinner"></span>{t("正在添加…", "Adding…")}{:else}{isNodeLink
            ? t("添加节点", "Add server")
            : t("添加订阅", "Add subscription")}{/if}
      </button>
      <div class="links">
        <button type="button" onclick={importFile} disabled={busy}>{t("导入配置文件", "Import config file")}</button>
        <span>·</span>
        <button type="button" onclick={() => (help = true)}>{t("还没有订阅？", "No subscription yet?")}</button>
      </div>
    </form>
  </main>
</div>

{#if help}
  <Sheet title={t("什么是订阅？", "What's a subscription?")} onclose={() => (help = false)}>
    {#if isEnglish()}
      <p>
        Choclite doesn't provide servers itself. You need a "subscription": a link from your provider
        that lists available servers and is kept up to date.
      </p>
      <p>
        <b>Where to get one:</b> after purchasing a plan, find "Subscription link" or "Copy Clash subscription"
        on your provider's website or dashboard, and paste it here. Many sites also have a one-click import button
        that opens Choclite directly.
      </p>
      <p>
        <b>Have your own server?</b> Paste a server link (ss://, trojan://, vmess://, vless://, hysteria2://), or import
        a Clash config file.
      </p>
      <p class="muted">Choose a provider you trust and follow local laws and regulations.</p>
    {:else}
      <p>
        Choclite 本身不提供网络节点。你需要一个「订阅」，它是服务商提供的一个链接，里面包含可用的节点，服务商会持续更新它。
      </p>
      <p><b>从哪里获得：</b>购买服务后，在服务商的网站或用户中心里找到「订阅链接」或「复制 Clash 订阅」，粘贴到这里即可。很多网站也有「一键导入」按钮，可以直接打开 Choclite。</p>
      <p><b>自己有服务器？</b>直接粘贴节点链接（ss://、trojan://、vmess://、vless://、hysteria2://），或者导入 Clash 配置文件。</p>
      <p class="muted">请选择可信的服务商，并遵守当地法律法规。</p>
    {/if}
    {#snippet actions()}
      <button class="primary" onclick={() => (help = false)}>{t("知道了", "Got it")}</button>
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
    justify-content: center;
    padding: 0 28px 64px;
    gap: 32px;
  }
  .hello {
    text-align: center;
  }
  .links {
    display: flex;
    justify-content: center;
    gap: 8px;
    font-size: 13px;
    color: var(--text-3);
  }
  .links button {
    color: var(--accent);
  }
  .links button:hover {
    text-decoration: underline;
  }
  .links button:disabled {
    opacity: 0.5;
  }
  .muted {
    color: var(--text-3);
    font-size: 12px;
  }
  .hello.compact {
    text-align: left;
    color: var(--text-2);
  }
  .logo {
    display: block;
    width: 88px;
    height: 88px;
    margin: 0 auto 18px;
    filter: drop-shadow(0 14px 24px rgb(95 109 230 / 0.35));
  }
  h1 {
    margin: 0;
    font-size: 22px;
    font-weight: 700;
  }
  .hello p {
    margin: 8px 0 0;
    color: var(--text-2);
  }

  form {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .field {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 4px 4px 14px;
    border-radius: 13px;
    background: var(--surface);
    box-shadow: var(--shadow), inset 0 0 0 1px var(--line);
    transition: box-shadow 0.2s;
  }
  .field:focus-within {
    box-shadow: var(--shadow), inset 0 0 0 2px var(--accent);
  }
  input {
    flex: 1;
    min-width: 0;
    height: 40px;
    border: 0;
    outline: none;
    background: none;
    font-size: 13.5px;
  }
  input::placeholder {
    color: var(--text-3);
  }
  .paste {
    width: 36px;
    height: 36px;
    border-radius: 10px;
    display: grid;
    place-items: center;
    color: var(--text-2);
  }
  .paste:hover {
    background: var(--surface-2);
    color: var(--accent);
  }
  .primary {
    height: 46px;
    border-radius: 13px;
    background: linear-gradient(145deg, var(--accent-2), var(--accent));
    color: var(--on-accent);
    font-weight: 600;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    box-shadow: 0 8px 22px -8px var(--accent);
    transition: opacity 0.2s, transform 0.15s;
  }
  .primary:not(:disabled):active {
    transform: scale(0.98);
  }
  .primary:disabled {
    opacity: 0.45;
    box-shadow: none;
  }
  .spinner {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    border: 2px solid rgb(255 255 255 / 0.35);
    border-top-color: #fff;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
