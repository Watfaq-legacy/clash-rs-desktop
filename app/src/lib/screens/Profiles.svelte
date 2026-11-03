<script lang="ts">
  import { api, type Profile } from "$lib/api";
  import Icon from "$lib/components/Icon.svelte";
  import Switch from "$lib/components/Switch.svelte";
  import TitleBar from "$lib/components/TitleBar.svelte";
  import * as fmt from "$lib/format";
  import { t } from "$lib/i18n.svelte";
  import { app } from "$lib/store.svelte";

  const profiles = $derived(app.view?.profiles ?? []);
  let busy = $state<string | null>(null);
  let confirmDelete = $state<string | null>(null);
  // Merge mode stays open while any subscription is merged.
  let merging = $state(false);
  const showMerge = $derived(merging || profiles.some((p) => p.merged));

  async function setMerging(on: boolean) {
    merging = on;
    if (!on) {
      for (const profile of profiles.filter((p) => p.merged)) {
        await act(profile.id, () => api.setMerged(profile.id, false));
      }
    }
  }

  async function act(id: string, action: () => Promise<unknown>, done?: string) {
    if (busy) return;
    busy = id;
    const result = await app.run(action);
    busy = null;
    if (result !== null && done) app.showToast(done);
  }

  function remove(profile: Profile) {
    if (confirmDelete !== profile.id) {
      confirmDelete = profile.id;
      setTimeout(() => confirmDelete === profile.id && (confirmDelete = null), 3000);
      return;
    }
    confirmDelete = null;
    act(profile.id, () => api.removeProfile(profile.id), t("已删除", "Deleted"));
  }

  function usage(profile: Profile) {
    const info = profile.subscription;
    if (!info?.total) return null;
    const used = info.upload + info.download;
    return { used, total: info.total, ratio: Math.min(1, used / info.total) };
  }
</script>

<div class="screen">
  <TitleBar title={t("订阅", "Subscriptions")} onBack={() => app.back()} />

  <div class="scroll">
    {#each profiles as profile (profile.id)}
      {@const u = usage(profile)}
      {@const expire = profile.subscription?.expire}
      <article class:active={profile.active}>
        <button
          class="main"
          onclick={() => !profile.active && act(profile.id, () => api.selectProfile(profile.id))}
          disabled={profile.active || !!busy}
        >
          <span class="radio"><span></span></span>
          <span class="info">
            <span class="name">
              {profile.name}
              {#if showMerge && profile.active}<span class="tag">{t("主订阅", "Main")}</span>{/if}
            </span>
            {#if u}
              <span class="meter"><span style="width: {u.ratio * 100}%"></span></span>
            {/if}
            <span class="meta num">
              {#if u}{fmt.bytes(u.used)} / {fmt.bytes(u.total, 0)}{/if}
              {#if expire}{u ? " · " : ""}{t(`${fmt.date(expire)} 到期`, `Expires ${fmt.date(expire)}`)}{/if}
              {#if !u && !expire && profile.updatedAt}{t(
                  `更新于 ${fmt.relative(profile.updatedAt)}`,
                  `Updated ${fmt.relative(profile.updatedAt)}`,
                )}{/if}
            </span>
          </span>
        </button>
        <div class="actions">
          {#if showMerge && !profile.active}
            <!-- A checkbox, not a pill: its state must read as a choice, so
                 nobody "taps the badge" and unmerges by accident. -->
            <label class="merge" class:on={profile.merged} title={t("把这个订阅的节点加入主订阅", "Add this subscription's servers to the main one")}>
              <input
                type="checkbox"
                checked={profile.merged}
                disabled={!!busy}
                onchange={async (event) => {
                  const input = event.currentTarget;
                  const merged = input.checked;
                  await act(profile.id, () => api.setMerged(profile.id, merged), merged ? t("已合并到主订阅", "Merged into main subscription") : t("已取消合并", "Unmerged"));
                  // If it was rolled back, show the real state again.
                  input.checked = !!app.view?.profiles.find((p) => p.id === profile.id)?.merged;
                }}
              />
              <span class="box"><Icon name="check" size={12} stroke={3} /></span>
              {t("合并", "Merge")}
            </label>
          {/if}
          {#if profile.remote}
            <button
              class="icon"
              title={t("更新订阅", "Update subscription")}
              disabled={!!busy}
              onclick={() =>
                act(profile.id, () => api.refreshSubscription(profile.id), t("订阅已更新", "Subscription updated"))}
            >
              <span class:spin={busy === profile.id}><Icon name="refresh" size={16} /></span>
            </button>
          {/if}
          <button
            class="icon del"
            class:confirm={confirmDelete === profile.id}
            title={t("删除", "Delete")}
            disabled={!!busy}
            onclick={() => remove(profile)}
          >
            {#if confirmDelete === profile.id}{t("确认删除", "Confirm")}{:else}<Icon name="trash" size={16} />{/if}
          </button>
        </div>
      </article>
    {/each}

    <button class="add" onclick={() => app.go("add")}>
      <Icon name="plus" size={16} /> {t("添加订阅", "Add subscription")}
    </button>

    {#if profiles.length > 1}
      <div class="merge-card">
        <div class="merge-row">
          <span>
            <b>{t("合并多个订阅", "Merge subscriptions")}</b>
            <small>
              {t(
                `把其他订阅的节点加入当前订阅，"自动选择"会在所有节点里挑最快的。分流规则沿用主订阅。`,
                `Add servers from other subscriptions to the current one. "Auto select" picks the fastest from all of them. Routing rules come from the main subscription.`,
              )}
            </small>
          </span>
          <Switch label={t("合并多个订阅", "Merge subscriptions")} checked={showMerge} onchange={setMerging} />
        </div>
      </div>
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
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .tag {
    margin-left: 6px;
    padding: 1px 6px;
    border-radius: 6px;
    font-size: 11px;
    font-weight: 600;
    color: var(--accent);
    background: var(--accent-soft);
    vertical-align: 1px;
  }
  .merge {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 6px;
    border-radius: 8px;
    font-size: 12.5px;
    color: var(--text-2);
    cursor: pointer;
    transition: background 0.15s;
  }
  .merge:hover {
    background: var(--surface-2);
  }
  .merge input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }
  .merge .box {
    width: 16px;
    height: 16px;
    border-radius: 5px;
    display: grid;
    place-items: center;
    color: transparent;
    box-shadow: inset 0 0 0 1.5px var(--text-3);
    transition: background 0.15s, box-shadow 0.15s, color 0.15s;
  }
  .merge.on {
    color: var(--accent);
    font-weight: 600;
  }
  .merge.on .box {
    color: var(--on-accent);
    background: var(--accent);
    box-shadow: none;
  }
  .merge:focus-within .box {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .merge-card {
    padding: 12px 14px;
    border-radius: var(--radius);
    background: var(--surface);
    box-shadow: var(--shadow);
  }
  .merge-row {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .merge-row span {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .merge-row small {
    font-size: 12px;
    line-height: 1.5;
    color: var(--text-3);
  }
  article {
    display: flex;
    align-items: center;
    border-radius: var(--radius);
    background: var(--surface);
    box-shadow: var(--shadow);
    transition: box-shadow 0.2s;
  }
  article.active {
    box-shadow: var(--shadow), inset 0 0 0 1.5px var(--accent);
  }
  .main {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 14px;
    text-align: left;
  }
  .radio {
    width: 18px;
    height: 18px;
    border-radius: 50%;
    flex-shrink: 0;
    display: grid;
    place-items: center;
    box-shadow: inset 0 0 0 2px var(--text-3);
    transition: box-shadow 0.2s;
  }
  .radio span {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--accent);
    transform: scale(0);
    transition: transform 0.2s var(--ease);
  }
  .active .radio {
    box-shadow: inset 0 0 0 2px var(--accent);
  }
  .active .radio span {
    transform: scale(1);
  }
  .info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .name {
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .meter {
    height: 4px;
    margin: 4px 0 2px;
    border-radius: 2px;
    background: var(--surface-2);
    overflow: hidden;
  }
  .meter span {
    display: block;
    height: 100%;
    border-radius: 2px;
    background: linear-gradient(90deg, var(--accent-2), var(--accent));
  }
  .meta {
    font-size: 12px;
    color: var(--text-2);
    min-height: 16px;
  }
  .actions {
    display: flex;
    gap: 2px;
    padding-right: 8px;
  }
  .icon {
    min-width: 34px;
    height: 34px;
    border-radius: 9px;
    display: grid;
    place-items: center;
    color: var(--text-2);
    font-size: 12px;
    transition: background 0.15s, color 0.15s;
  }
  .icon:not(:disabled):hover {
    background: var(--surface-2);
    color: var(--text);
  }
  .del:not(:disabled):hover,
  .del.confirm {
    color: var(--bad);
  }
  .del.confirm {
    padding: 0 10px;
    background: rgb(226 87 76 / 0.12);
  }
  .spin {
    display: grid;
    animation: spin 0.9s linear infinite;
  }
  .add {
    height: 48px;
    border-radius: var(--radius);
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    color: var(--accent);
    border: 1.5px dashed var(--accent);
    opacity: 0.85;
    transition: background 0.15s;
  }
  .add:hover {
    background: var(--accent-soft);
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
