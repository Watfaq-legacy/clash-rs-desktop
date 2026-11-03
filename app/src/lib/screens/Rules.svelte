<script lang="ts">
  import { api, type CustomRule, type RuleAction, type RuleKind } from "$lib/api";
  import Icon from "$lib/components/Icon.svelte";
  import Segmented from "$lib/components/Segmented.svelte";
  import TitleBar from "$lib/components/TitleBar.svelte";
  import { app } from "$lib/store.svelte";

  const rules = $derived(app.view?.customRules ?? []);

  let value = $state("");
  let action = $state<RuleAction>("proxy");
  let busy = $state(false);

  const ACTIONS: { value: RuleAction; label: string }[] = [
    { value: "proxy", label: "走代理" },
    { value: "direct", label: "直连" },
    { value: "reject", label: "拦截" },
  ];
  const ACTION_LABEL: Record<RuleAction, string> = { proxy: "代理", direct: "直连", reject: "拦截" };
  const KIND_LABEL: Record<RuleKind, string> = {
    "domain-suffix": "网站",
    "domain-keyword": "关键词",
    "ip-cidr": "IP",
    "process-name": "程序",
  };

  /** Guess what the user typed, so they never have to pick a rule type. */
  function kindOf(input: string): RuleKind {
    const text = input.trim();
    if (/^[\d.]+(\/\d+)?$/.test(text) || /^[\da-f:]+:[\da-f:]*(\/\d+)?$/i.test(text)) return "ip-cidr";
    if (/\.exe$/i.test(text)) return "process-name";
    if (!text.includes(".") && !text.includes("/")) return "domain-keyword";
    return "domain-suffix";
  }

  const detected = $derived(value.trim() ? kindOf(value) : null);

  async function add(event: SubmitEvent) {
    event.preventDefault();
    if (!value.trim() || busy) return;
    busy = true;
    const result = await app.run(() => api.addCustomRule(kindOf(value), value, action));
    busy = false;
    if (result) {
      value = "";
      app.showToast(result.phase === "connected" ? "规则已生效" : "规则已保存");
    }
  }

  async function remove(index: number) {
    if (busy) return;
    busy = true;
    await app.run(() => api.removeCustomRule(index));
    busy = false;
  }

  const describe = (rule: CustomRule) =>
    rule.kind === "domain-keyword" ? `包含「${rule.value}」` : rule.value;
</script>

<div class="screen">
  <TitleBar title="网站规则" onBack={() => app.back()} />

  <div class="scroll">
    <form class="card" onsubmit={add}>
      <label class="field">
        <input
          bind:value
          placeholder="网址、域名、IP 或程序名"
          spellcheck="false"
          autocomplete="off"
          disabled={busy}
        />
        {#if detected}<span class="kind">{KIND_LABEL[detected]}</span>{/if}
      </label>
      <Segmented options={ACTIONS} value={action} onchange={(next) => (action = next)} />
      <button class="primary" type="submit" disabled={!value.trim() || busy}>
        <Icon name="plus" size={16} /> 添加规则
      </button>
    </form>
    <p class="hint">
      例如 <code>example.com</code>（含所有子域名）、<code>192.168.1.0/24</code>、<code>Telegram.exe</code>。
      这里的规则优先于订阅规则，订阅更新后依然保留。
    </p>

    {#if rules.length}
      <div class="list">
        {#each rules as rule, index (rule.kind + rule.value)}
          <div class="rule">
            <span class="tag">{KIND_LABEL[rule.kind]}</span>
            <span class="value">{describe(rule)}</span>
            <span class="action {rule.action}">{ACTION_LABEL[rule.action]}</span>
            <button class="del" onclick={() => remove(index)} disabled={busy} aria-label="删除">
              <Icon name="trash" size={15} />
            </button>
          </div>
        {/each}
      </div>
    {:else}
      <p class="empty">还没有自定义规则</p>
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
    padding: 4px 16px 20px 20px;
    scrollbar-gutter: stable;
  }
  .card {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px;
    border-radius: var(--radius);
    background: var(--surface);
    box-shadow: var(--shadow);
  }
  .field {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 10px 0 12px;
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
    height: 40px;
    border: 0;
    outline: none;
    background: none;
  }
  input::placeholder {
    color: var(--text-3);
  }
  .kind {
    font-size: 11.5px;
    color: var(--accent);
  }
  .primary {
    height: 40px;
    border-radius: 11px;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    font-weight: 600;
    background: linear-gradient(145deg, var(--accent-2), var(--accent));
    color: var(--on-accent);
    transition: opacity 0.2s;
  }
  .primary:disabled {
    opacity: 0.45;
  }
  .hint {
    margin: 10px 4px 16px;
    font-size: 12px;
    line-height: 1.6;
    color: var(--text-3);
  }
  code {
    font-family: "Cascadia Code", Consolas, monospace;
    font-size: 11.5px;
    color: var(--text-2);
  }
  .list {
    border-radius: var(--radius);
    background: var(--surface);
    box-shadow: var(--shadow);
    overflow: hidden;
  }
  .rule {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 8px 10px 14px;
  }
  .rule + .rule {
    box-shadow: inset 0 1px 0 var(--line);
  }
  .tag {
    font-size: 11px;
    color: var(--text-3);
    min-width: 34px;
  }
  .value {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    user-select: text;
  }
  .action {
    font-size: 12px;
    padding: 2px 8px;
    border-radius: 999px;
    background: var(--surface-2);
  }
  .action.proxy {
    color: var(--accent);
    background: var(--accent-soft);
  }
  .action.direct {
    color: var(--good);
  }
  .action.reject {
    color: var(--bad);
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
  .empty {
    text-align: center;
    color: var(--text-3);
    margin-top: 40px;
  }
</style>
