<script lang="ts">
  let { modelId }: { modelId: string } = $props();
  const family = $derived(modelId.startsWith("vosk") ? "vosk" : modelId.startsWith("parakeet") ? "parakeet" : modelId.startsWith("gliner") ? "gliner" : "qwen");
</script>

<span class="family-icon {family}" data-model-family={family} aria-hidden="true">
  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
    {#if family === "vosk"}<path d="m3 5 9 15 9-15M7 5l5 8 5-8" />
    {:else if family === "parakeet"}
      <path d="M6 14c-2-4 0-9 5-10 4-1 7 2 7 5l3 2-4 1c-1 4-4 6-7 5l-5 4 1-7Z" />
      <path d="M7 11c1 3 3 4 6 3M11 17v3m3-5v5" /><circle cx="14.5" cy="7.5" r=".7" fill="currentColor" stroke="none" />
    {:else if family === "gliner"}
      <path d="M9 4H5v16h4m6-16h4v16h-4M9 9h6m-6 6h6" />
      <circle cx="12" cy="12" r="1" />
    {:else}
      <path d="m12 3 3 5h6l-3 5 3 5h-6l-3 4-3-4H3l3-5-3-5h6l3-5Z" />
    {/if}
  </svg>
</span>

<style>
  .family-icon { display:grid; place-items:center; flex:none; width:40px; height:40px; border:1px solid currentColor; border-radius:12px; }
  svg { width:24px; height:24px; }
  .gliner { color:#8bf0d1; background:#0d827845; }
  .vosk { color:#8bf0d1; background:#0d827845; }
  .parakeet { color:#bce8ff; background:#3a8ac545; }
  .qwen { color:#e0bbff; background:#985ce845; }
</style>
