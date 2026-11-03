<script lang="ts">
  import { api } from "$lib/api";
  import TitleBar from "$lib/components/TitleBar.svelte";
  import { isEnglish, t } from "$lib/i18n.svelte";
  import { app } from "$lib/store.svelte";

  const EXAMPLE = $derived(
    t(
      `# 在订阅配置之上生效，订阅更新后依然保留。
# 追加规则（插到订阅规则最前面，优先匹配）：
prepend-rules:
  - DOMAIN-SUFFIX,example.com,DIRECT

# 也可以直接覆盖任意设置，例如：
# ipv6: true
`,
      `# Applied on top of your subscription and kept when it updates.
# Add rules (inserted before subscription rules, matched first):
prepend-rules:
  - DOMAIN-SUFFIX,example.com,DIRECT

# You can also override any setting, e.g.:
# ipv6: true
`,
    ),
  );

  let text = $state("");
  let saved = $state("");
  let loaded = $state(false);
  let saving = $state(false);

  const dirty = $derived(loaded && text !== saved);

  $effect(() => {
    app.run(() => api.readOverride()).then((value) => {
      text = saved = value ?? "";
      loaded = true;
    });
  });

  async function save() {
    saving = true;
    const result = await app.run(() => api.writeOverride(text));
    saving = false;
    if (result) {
      saved = text;
      app.showToast(result.phase === "connected" ? t("已保存并重新连接", "Saved and reconnected") : t("已保存", "Saved"));
    }
  }
</script>

<div class="screen">
  <TitleBar title={t("自定义配置", "Custom configuration")} onBack={() => app.back()} />

  <div class="body">
    <p class="hint">
      {#if isEnglish()}
        Same syntax as a Clash config. Use <code>prepend-rules</code> / <code>append-rules</code> to add rules, and
        <code>prepend-proxies</code> or <code>prepend-proxy-groups</code> to add servers or groups; same names replace.
      {:else}
        写法与 Clash 配置相同。用 <code>prepend-rules</code> / <code>append-rules</code> 追加规则，
        <code>prepend-proxies</code>、<code>prepend-proxy-groups</code> 追加节点或分组，同名会替换。
      {/if}
    </p>
    <textarea
      bind:value={text}
      placeholder={EXAMPLE}
      spellcheck="false"
      disabled={!loaded || saving}
    ></textarea>
    <div class="bar">
      <button class="ghost" onclick={() => (text = saved)} disabled={!dirty || saving}>{t("还原", "Revert")}</button>
      <button class="primary" onclick={save} disabled={!dirty || saving}>
        {saving ? t("正在应用…", "Applying…") : t("保存并应用", "Save and apply")}
      </button>
    </div>
  </div>
</div>

<style>
  .screen {
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  .body {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 0 20px 20px;
  }
  .hint {
    margin: 0;
    font-size: 12px;
    line-height: 1.6;
    color: var(--text-2);
  }
  code {
    font-family: "Cascadia Code", Consolas, monospace;
    font-size: 11.5px;
    padding: 1px 4px;
    border-radius: 4px;
    background: var(--surface-2);
  }
  textarea {
    flex: 1;
    resize: none;
    padding: 14px;
    border: 0;
    outline: none;
    border-radius: var(--radius);
    background: var(--surface);
    box-shadow: var(--shadow), inset 0 0 0 1px var(--line);
    font-family: "Cascadia Code", Consolas, monospace;
    font-size: 12.5px;
    line-height: 1.6;
    tab-size: 2;
  }
  textarea:focus {
    box-shadow: var(--shadow), inset 0 0 0 2px var(--accent);
  }
  textarea::placeholder {
    color: var(--text-3);
  }
  .bar {
    display: flex;
    gap: 10px;
  }
  .bar button {
    flex: 1;
    height: 42px;
    border-radius: 12px;
    font-weight: 600;
    transition: opacity 0.2s;
  }
  .ghost {
    background: var(--surface-2);
  }
  .primary {
    flex: 2 !important;
    background: linear-gradient(145deg, var(--accent-2), var(--accent));
    color: var(--on-accent);
  }
  .bar button:disabled {
    opacity: 0.45;
  }
</style>
