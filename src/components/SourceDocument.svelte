<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import Icon from "./Icon.svelte";
  let { documentId }: { documentId: string } = $props();
  let page = $state(0);
  let pages = $state(1);
  let image = $state("");
  let error = $state("");
  let loading = $state(false);
  let scale = $state(100);
  $effect(() => {
    const id = documentId;
    const requested = page;
    let disposed = false;
    loading = true; error = ""; image = "";
    void invoke<{ pages: number; image: string }>("private_source_page", { page: requested }).then(result => {
      if (!disposed && id === documentId) { pages = result.pages; image = result.image; }
    }).catch(reason => { if (!disposed) error = String(reason); }).finally(() => { if (!disposed) loading = false; });
    return () => { disposed = true; };
  });
</script>
<section class="source-document" aria-label="PDF o imagen original">
  <strong>Original · sin modificar</strong>
  <div class="source-controls">
    <button aria-label="Página anterior" title="Página anterior" disabled={page === 0 || loading} onclick={() => page--}><Icon name="chevron" /></button>
    <span>Página {page + 1} de {pages}</span>
    <button aria-label="Página siguiente" title="Página siguiente" disabled={page + 1 >= pages || loading} onclick={() => page++}><Icon name="chevron" /></button>
    <select aria-label="Ampliación del original" bind:value={scale}><option value={100}>Ajustar</option><option value={150}>150 %</option><option value={200}>200 %</option><option value={300}>300 %</option></select>
  </div>
  {#if loading}<p role="status">Abriendo original…</p>{/if}
  {#if error}<p role="alert">{error}</p>{/if}
  <div class="source-page">{#if image}<img src={image} alt={`Documento original, página ${page + 1}`} style:width={`${scale}%`}/>{/if}</div>
</section>
<style>
  .source-document { display:flex; flex-direction:column; gap:16px; min-width:0; padding:16px; border:1px solid var(--line); border-radius:10px; }
  .source-controls { display:flex; align-items:center; flex-wrap:wrap; gap:8px; }
  button,select { display:flex; align-items:center; justify-content:center; min-width:44px; min-height:44px; border:1px solid var(--line); border-radius:6px; background:var(--surface); color:var(--text); }
  button:first-child :global(svg) { transform:rotate(180deg); }
  .source-controls span { flex:1; text-align:center; }
  .source-page { width:100%; min-height:280px; overflow:auto; }
  img { display:block; height:auto; max-width:none; background:white; }
</style>
