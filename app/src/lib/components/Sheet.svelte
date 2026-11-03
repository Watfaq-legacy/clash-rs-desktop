<script lang="ts">
  import type { Snippet } from "svelte";
  import { fade, fly } from "svelte/transition";
  import { cubicOut } from "svelte/easing";

  let {
    title,
    onclose,
    children,
    actions,
  }: { title: string; onclose: () => void; children: Snippet; actions: Snippet } = $props();
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onclose()} />

<div class="backdrop" transition:fade={{ duration: 180 }} onclick={onclose} role="presentation"></div>
<div
  class="sheet"
  role="dialog"
  aria-modal="true"
  aria-label={title}
  transition:fly={{ y: 40, duration: 260, easing: cubicOut }}
>
  <h3>{title}</h3>
  <div class="body">{@render children()}</div>
  <div class="actions">{@render actions()}</div>
</div>

<style>
  .backdrop {
    position: absolute;
    inset: 0;
    z-index: 20;
    background: rgb(10 12 30 / 0.35);
    backdrop-filter: blur(2px);
  }
  .sheet {
    position: absolute;
    left: 12px;
    right: 12px;
    bottom: 12px;
    z-index: 21;
    padding: 22px 20px 16px;
    border-radius: 22px;
    background: var(--surface);
    box-shadow: 0 24px 60px rgb(0 0 0 / 0.25);
  }
  h3 {
    margin: 0 0 10px;
    font-size: 17px;
    font-weight: 700;
  }
  .body {
    color: var(--text-2);
    font-size: 13.5px;
    line-height: 1.6;
  }
  .body :global(p) {
    margin: 0 0 8px;
  }
  .actions {
    display: flex;
    gap: 10px;
    margin-top: 18px;
  }
  .actions :global(button) {
    flex: 1;
    height: 44px;
    border-radius: 13px;
    font-weight: 600;
    transition: opacity 0.2s, transform 0.15s;
  }
  .actions :global(button:active) {
    transform: scale(0.98);
  }
  .actions :global(.ghost) {
    background: var(--surface-2);
  }
  .actions :global(.primary) {
    flex: 1.6;
    background: linear-gradient(145deg, var(--accent-2), var(--accent));
    color: var(--on-accent);
    box-shadow: 0 8px 22px -8px var(--accent);
  }
  .actions :global(button:disabled) {
    opacity: 0.5;
  }
</style>
