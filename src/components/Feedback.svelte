<script lang="ts">
  import { untrack, getContext } from "svelte";
  import { putNotice, removeNotice } from "../lib/feedback-store.svelte";
  let { message, kind = "error", className = "", onDismiss }: {
    message: string; kind?: "error" | "status"; className?: string; onDismiss?: () => void;
  } = $props();
  const id = Symbol();
  const isActive = getContext<(() => boolean) | undefined>("feedbackActive");
  $effect(() => {
    const notice = { id, message: isActive?.() === false ? "" : message.replace(/^Error:\s*/, ""), kind, className, onDismiss };
    untrack(() => notice.message ? putNotice(notice) : removeNotice(id));
    return () => untrack(() => removeNotice(id));
  });
</script>
