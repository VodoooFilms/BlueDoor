<script lang="ts">
  import { privateCopy as p } from "../lib/private-i18n";
  import Icon from "./Icon.svelte";
  let { message, kind = "error", className = "", onDismiss }: {
    message: string; kind?: "error" | "status"; className?: string; onDismiss?: () => void;
  } = $props();
</script>
<div class="feedback-message {className}" class:feedback-error={kind === "error"} role={kind === "error" ? "alert" : "status"} aria-atomic="true">
  <div class="feedback-copy">{#each message.split("\n") as line}<p>{line}</p>{/each}</div>
  {#if onDismiss}<button aria-label={p("Cerrar aviso")} title={p("Cerrar aviso")} onclick={onDismiss}><Icon name="close" size={18} /><span>{p("Cerrar aviso")}</span></button>{/if}
</div>
<style>
  .feedback-message { box-sizing:border-box; max-width:100%; display:flex; flex-direction:column; align-items:stretch; gap:10px; padding:14px 16px; border:1px solid #91baff80; border-radius:12px; background:#0b2d78; color:#f3f7ff; box-shadow:0 6px 24px #08225238; font-size:14px; line-height:1.5; pointer-events:auto; }
  .feedback-error { border-color:#b9d3ff; background:var(--error-background); }
  .feedback-copy { flex:1; min-width:0; text-align:center; white-space:pre-line; overflow-wrap:anywhere; user-select:text; }
  .feedback-copy p { margin:0; }
  .feedback-copy p + p { margin-top:6px; }
  .leak-alert .feedback-copy p:first-child { font-weight:700; }
  button { display:inline-flex; align-items:center; justify-content:center; align-self:center; gap:6px; flex:none; min-height:36px; padding:6px 10px; color:white; background:transparent; border:1px solid #b9d3ff60; border-radius:6px; cursor:pointer; }
  @media (pointer:coarse) { button { min-height:44px; } }
  button:hover { background:#ffffff18; }
  button:focus-visible { outline:2px solid white; outline-offset:3px; }
</style>
