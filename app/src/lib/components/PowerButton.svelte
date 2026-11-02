<script lang="ts">
  import type { Phase } from "$lib/api";
  import Icon from "./Icon.svelte";

  let { phase, onclick }: { phase: Phase; onclick: () => void } = $props();

  const busy = $derived(phase === "connecting" || phase === "disconnecting");
  const on = $derived(phase === "connected");
</script>

<button
  class="power"
  class:on
  class:busy
  {onclick}
  disabled={busy}
  aria-label={on ? "断开连接" : "连接"}
  aria-pressed={on}
>
  <span class="halo"></span>
  <span class="ring"></span>
  <span class="disc">
    <Icon name="power" size={44} stroke={2.2} />
  </span>
</button>

<style>
  .power {
    position: relative;
    width: 176px;
    height: 176px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    -webkit-tap-highlight-color: transparent;
  }

  .halo,
  .ring,
  .disc {
    position: absolute;
    border-radius: 50%;
  }

  /* Soft glow behind the button, only when connected. */
  .halo {
    inset: -28px;
    background: radial-gradient(circle, var(--accent-soft) 0%, transparent 68%);
    opacity: 0;
    transform: scale(0.85);
    transition: opacity 0.6s var(--ease), transform 0.6s var(--ease);
  }
  .on .halo {
    opacity: 1;
    transform: scale(1);
    animation: breathe 3.6s ease-in-out infinite;
  }

  .ring {
    inset: 0;
    border: 2px solid var(--line);
    transition: border-color 0.4s;
  }
  .on .ring {
    border-color: transparent;
    box-shadow: 0 0 0 2px var(--accent-soft);
  }
  /* Rotating arc while working. */
  .busy .ring {
    border-color: transparent;
    background: conic-gradient(from 0deg, transparent 0 62%, var(--accent) 100%) border-box;
    mask: radial-gradient(farthest-side, transparent calc(100% - 3px), #000 calc(100% - 3px));
    animation: spin 1s linear infinite;
  }

  .disc {
    inset: 16px;
    display: grid;
    place-items: center;
    background: var(--surface);
    color: var(--text-3);
    box-shadow: var(--shadow), inset 0 0 0 1px var(--line);
    transition:
      background 0.45s var(--ease),
      color 0.45s var(--ease),
      transform 0.25s var(--ease),
      box-shadow 0.45s var(--ease);
  }
  .power:not(:disabled):hover .disc {
    transform: scale(1.03);
    color: var(--accent);
  }
  .power:not(:disabled):active .disc {
    transform: scale(0.97);
  }
  .on .disc,
  .on:not(:disabled):hover .disc {
    background: linear-gradient(145deg, var(--accent-2), var(--accent));
    color: var(--on-accent);
    box-shadow:
      0 10px 30px -6px var(--accent),
      inset 0 1px 0 rgb(255 255 255 / 0.25);
  }
  .busy .disc {
    color: var(--accent);
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  @keyframes breathe {
    0%,
    100% {
      transform: scale(0.96);
      opacity: 0.85;
    }
    50% {
      transform: scale(1.04);
      opacity: 1;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .on .halo,
    .busy .ring {
      animation: none;
    }
  }
</style>
