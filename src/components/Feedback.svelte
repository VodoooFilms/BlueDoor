<script lang="ts">
  import FeedbackMessage from "./FeedbackMessage.svelte";
  import { untrack, getContext } from "svelte";
  import { putNotice, removeNotice } from "../lib/feedback-store.svelte";
  let { message, kind = "error", className = "", onDismiss, inline = false, autoDismissMs }: {
    autoDismissMs?: number; inline?: boolean; message: string; kind?: "error" | "status"; className?: string; onDismiss?: () => void;
  } = $props();
  const id = Symbol();
  let dismissed = $state(false);
  const isActive = getContext<(() => boolean) | undefined>("feedbackActive");
  const visibleMessage = $derived(isActive?.() === false ? "" : message.replace(/^Error:\s*/, ""));
  $effect(() => {
    dismissed = false;
    const delay = autoDismissMs ?? (kind === "status" ? 5000 : 0);
    const notice = { id, message: inline ? "" : visibleMessage, kind, className, onDismiss };
    untrack(() => notice.message ? putNotice(notice) : removeNotice(id));
    const timer = notice.message || (inline && visibleMessage)
      ? delay > 0 ? setTimeout(() => {
          dismissed = true;
          untrack(() => { onDismiss?.(); removeNotice(id); });
        }, delay) : undefined
      : undefined;
    return () => { clearTimeout(timer); untrack(() => removeNotice(id)); };
  });
</script>

{#if inline && visibleMessage && !dismissed}
  <FeedbackMessage message={visibleMessage} {kind} {className} {onDismiss} />
{/if}
