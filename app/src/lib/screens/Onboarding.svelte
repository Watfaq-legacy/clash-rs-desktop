<script lang="ts">
  import { readText } from "@tauri-apps/plugin-clipboard-manager";
  import { api } from "$lib/api";
  import Icon from "$lib/components/Icon.svelte";
  import TitleBar from "$lib/components/TitleBar.svelte";
  import { app } from "$lib/store.svelte";

  let { onBack }: { onBack?: () => void } = $props();

  let url = $state("");
  let busy = $state(false);

  const looksLikeUrl = $derived(/^https?:\/\/\S+$/i.test(url.trim()));

  async function paste() {
    const text = (await readText().catch(() => ""))?.trim() ?? "";
    if (/^https?:\/\//i.test(text)) url = text;
    else app.showToast("剪贴板里没有订阅链接");
  }

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    if (!looksLikeUrl || busy) return;
    busy = true;
    const view = await app.run(() => api.addSubscription(url.trim()));
    busy = false;
    if (view) {
      url = "";
      app.showToast("订阅已添加");
      onBack?.();
    }
  }
</script>

<div class="screen">
  <TitleBar title={onBack ? "添加订阅" : ""} {onBack} />

  <main>
    {#if !onBack}
      <div class="hello">
        <img class="logo" src="/brand/logo.png" alt="" draggable="false" />
        <h1>欢迎使用 Choclite</h1>
        <p>粘贴你的订阅链接，一键开始</p>
      </div>
    {:else}
      <div class="hello compact">
        <p>粘贴服务商提供的订阅链接</p>
      </div>
    {/if}

    <form onsubmit={submit}>
      <label class="field">
        <input
          bind:value={url}
          placeholder="https://..."
          spellcheck="false"
          autocomplete="off"
          disabled={busy}
        />
        <button type="button" class="paste" onclick={paste} disabled={busy} title="从剪贴板粘贴">
          <Icon name="clipboard" size={16} />
        </button>
      </label>
      <button class="primary" type="submit" disabled={!looksLikeUrl || busy}>
        {#if busy}<span class="spinner"></span>正在下载订阅…{:else}添加订阅{/if}
      </button>
    </form>
  </main>
</div>

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
