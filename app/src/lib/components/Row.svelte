<script lang="ts">
  import type { Snippet } from "svelte";
  import Icon from "./Icon.svelte";

  let {
    label,
    hint,
    onclick,
    danger = false,
    trailing,
  }: {
    label: string;
    hint?: string;
    onclick?: () => void;
    danger?: boolean;
    trailing?: Snippet;
  } = $props();
</script>

{#if onclick}
  <button class="row link" class:danger {onclick}>
    <span class="text"><span>{label}</span>{#if hint}<small>{hint}</small>{/if}</span>
    {#if trailing}{@render trailing()}{/if}
    {#if !danger}<span class="chev"><Icon name="chevron" size={16} /></span>{/if}
  </button>
{:else}
  <div class="row">
    <span class="text"><span>{label}</span>{#if hint}<small>{hint}</small>{/if}</span>
    {#if trailing}{@render trailing()}{/if}
  </div>
{/if}

<style>
  .row {
    width: 100%;
    min-height: 50px;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 14px;
    text-align: left;
  }
  .link {
    transition: background 0.15s;
  }
  .link:hover {
    background: var(--surface-2);
  }
  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  small {
    font-size: 12px;
    color: var(--text-3);
  }
  .danger {
    color: var(--bad);
    justify-content: center;
  }
  .danger .text {
    align-items: center;
  }
  .chev {
    display: grid;
    color: var(--text-3);
  }
</style>
