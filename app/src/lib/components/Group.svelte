<script lang="ts">
  import type { Snippet } from "svelte";

  let { title, footer, children }: { title?: string; footer?: string; children: Snippet } =
    $props();
</script>

<section>
  {#if title}<h2>{title}</h2>{/if}
  <div class="box">{@render children()}</div>
  {#if footer}<p>{footer}</p>{/if}
</section>

<style>
  section {
    margin-bottom: 18px;
  }
  h2 {
    margin: 0 4px 8px;
    font-size: 12px;
    font-weight: 500;
    color: var(--text-3);
  }
  .box {
    border-radius: var(--radius);
    background: var(--surface);
    box-shadow: var(--shadow);
    overflow: hidden;
  }
  .box > :global(* + *) {
    box-shadow: inset 0 1px 0 var(--line);
  }
  p {
    margin: 8px 4px 0;
    font-size: 12px;
    line-height: 1.5;
    color: var(--text-3);
  }
</style>
