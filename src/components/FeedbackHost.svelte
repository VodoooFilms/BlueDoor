<script lang="ts">
  import { getNotices, removeNotice } from "../lib/feedback-store.svelte";
  import { privateCopy as p } from "../lib/private-i18n";
  import FeedbackMessage from "./FeedbackMessage.svelte";
</script>
<aside class="feedback-host" aria-label={p("Avisos")}>
  {#each getNotices() as notice (notice.id)}
    <FeedbackMessage message={notice.message} kind={notice.kind} className={notice.className} onDismiss={() => { notice.onDismiss?.(); removeNotice(notice.id); }} />
  {/each}
</aside>
<style>
  .feedback-host { position:fixed; z-index:800; bottom:calc(24px + env(safe-area-inset-bottom, 0px)); left:24px; width:min(480px,calc(100vw - 48px)); max-height: min(36vh,340px); overflow:auto; display:flex; flex-direction:column; gap:10px; pointer-events:none; }
</style>
