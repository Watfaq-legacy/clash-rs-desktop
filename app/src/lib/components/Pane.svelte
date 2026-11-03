<script lang="ts" module>
  import { getContext } from "svelte";

  /** Where a screen is shown: alone, or as the left/right half of the wide layout. */
  export type PaneKind = "single" | "main" | "side";

  export function currentPane(): { readonly kind: PaneKind } {
    return getContext("pane") ?? { kind: "single" };
  }
</script>

<script lang="ts">
  import { setContext, type Snippet } from "svelte";

  let { kind, children }: { kind: PaneKind; children: Snippet } = $props();
  setContext("pane", {
    get kind() {
      return kind;
    },
  });
</script>

{@render children()}
