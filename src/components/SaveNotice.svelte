<script lang="ts">
  import { privateCopy as p } from "../lib/private-i18n";
  let { sequence = 0 }: { sequence: number } = $props();
  let visible = $state(false);
  $effect(() => {
    if (!sequence) return;
    visible = true;
    const timer = setTimeout(() => visible = false, 2400);
    return () => clearTimeout(timer);
  });
</script>

<div class="save-feedback" role={visible ? "status" : undefined} aria-live="polite" aria-atomic="true">
  {#if visible}
    {#key sequence}
      <span class="save-confirmation"><span aria-hidden="true">✓</span> {p("Guardado")}</span>
    {/key}
  {/if}
</div>

<style>
  .save-feedback { position:absolute; bottom:20px; right:20px; z-index:20; pointer-events:none; }
  .save-confirmation { display:flex; align-items:center; gap:8px; padding:10px 16px; border:1px solid var(--line); border-radius:8px; color:var(--success); background:var(--surface); box-shadow:0 3px 14px var(--shadow); font-size:13px; animation:save-confirmation 180ms ease-out; }
  @keyframes save-confirmation { from { opacity:0; transform:translateY(6px); } to { opacity:1; transform:translateY(0); } }
  @media (prefers-reduced-motion: reduce) { .save-confirmation { animation:none; } }
</style>
