<script lang="ts">
  let {
    checked,
    onchange,
    disabled = false,
    label,
  }: { checked: boolean; onchange: (value: boolean) => void; disabled?: boolean; label: string } =
    $props();
</script>

<button
  class="switch"
  class:checked
  role="switch"
  aria-checked={checked}
  aria-label={label}
  {disabled}
  onclick={() => onchange(!checked)}
>
  <span class="knob"></span>
</button>

<style>
  .switch {
    width: 40px;
    height: 24px;
    border-radius: 12px;
    background: var(--surface-2);
    box-shadow: inset 0 0 0 1px var(--line);
    position: relative;
    flex-shrink: 0;
    transition: background 0.2s var(--ease);
  }
  .checked {
    background: var(--accent);
    box-shadow: none;
  }
  .knob {
    position: absolute;
    top: 3px;
    left: 3px;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: #fff;
    box-shadow: 0 1px 3px rgb(0 0 0 / 0.25);
    transition: transform 0.22s var(--ease);
  }
  .checked .knob {
    transform: translateX(16px);
  }
  .switch:disabled {
    opacity: 0.5;
  }
</style>
