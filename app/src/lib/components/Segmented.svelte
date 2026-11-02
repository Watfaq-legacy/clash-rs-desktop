<script lang="ts" generics="T extends string">
  let {
    options,
    value,
    onchange,
  }: { options: { value: T; label: string }[]; value: T; onchange: (value: T) => void } = $props();

  const index = $derived(Math.max(0, options.findIndex((o) => o.value === value)));
</script>

<div class="seg" style="--count: {options.length}; --index: {index}" role="radiogroup">
  <span class="thumb"></span>
  {#each options as option (option.value)}
    <button
      role="radio"
      aria-checked={option.value === value}
      class:active={option.value === value}
      onclick={() => option.value !== value && onchange(option.value)}
    >
      {option.label}
    </button>
  {/each}
</div>

<style>
  .seg {
    position: relative;
    display: grid;
    grid-template-columns: repeat(var(--count), 1fr);
    padding: 3px;
    border-radius: 11px;
    background: var(--surface-2);
  }
  .thumb {
    position: absolute;
    top: 3px;
    bottom: 3px;
    left: 3px;
    width: calc((100% - 6px) / var(--count));
    transform: translateX(calc(100% * var(--index)));
    border-radius: 8px;
    background: var(--surface);
    box-shadow: 0 1px 3px rgb(0 0 0 / 0.12);
    transition: transform 0.28s var(--ease);
  }
  button {
    position: relative;
    height: 30px;
    font-size: 13px;
    color: var(--text-2);
    transition: color 0.2s;
  }
  .active {
    color: var(--text);
    font-weight: 600;
  }
</style>
