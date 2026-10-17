<script lang="ts">
  import { getNotices, removeNotice } from "../lib/feedback-store.svelte";
  import { privateCopy as p } from "../lib/private-i18n";
  import Icon from "./Icon.svelte";
</script>
<aside class="feedback-host" aria-label={p("Avisos")}>
  {#each getNotices() as notice (notice.id)}
    <div class="feedback-message {notice.className}" class:feedback-error={notice.kind === "error"} role={notice.kind === "error" ? "alert" : "status"} aria-atomic="true">
      <div class="feedback-copy">{#each notice.message.split("\n") as line}<p>{line}</p>{/each}</div>
      <button aria-label={p("Cerrar aviso")} title={p("Cerrar aviso")} onclick={() => { notice.onDismiss?.(); removeNotice(notice.id); }}><Icon name="close" size={18} /></button>
    </div>
  {/each}
</aside>
<style>
  .feedback-host { position:fixed; z-index:800; bottom:24px; left:24px; width:min(480px,calc(100vw - 48px)); max-height: min(36vh,340px); overflow:auto; display:flex; flex-direction:column; gap:10px; pointer-events:none; }
  .feedback-message { display:flex; align-items:flex-start; gap:14px; padding:14px 16px; border:1px solid #91baff80; border-radius:12px; background:#0b2d78; color:#f3f7ff; box-shadow:0 6px 24px #08225238; font-size:14px; line-height:1.5; pointer-events:auto; }
  .feedback-error { border-color:#b9d3ff; background:var(--error-background); }
  .feedback-copy { flex:1; min-width:0; white-space:pre-line; overflow-wrap:anywhere; user-select:text; }
  .feedback-copy p { margin:0; }
  .feedback-copy p + p { margin-top:6px; }
  .leak-alert .feedback-copy p:first-child { font-weight:700; }
  button { display:grid; place-items:center; flex:none; width:28px; height:28px; padding:4px; color:white; background:transparent; border:1px solid #b9d3ff60; border-radius:6px; cursor:pointer; }
  button:hover { background:#ffffff18; }
</style>
