<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import Icon from "./Icon.svelte";
  import { pinchDocument } from "../lib/document-pinch";
  let { documentId, zoom = $bindable(100), fitWidth = $bindable(true) }: { documentId: string; zoom?: number; fitWidth?: boolean } = $props();
  let page = $state(0);
  let pages = $state(1);
  let image = $state("");
  let error = $state("");
  let loading = $state(false);
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
<section use:pinchDocument={{ getZoom: () => fitWidth ? 100 : zoom, setZoom: value => { fitWidth = false; zoom = value; }, viewerSelector: ".source-page", pageSelector: ".source-image" }} class="source-document" aria-label="PDF o imagen original">
  <strong>Original · sin modificar</strong>
  <div class="source-controls">
    <button aria-label="Página anterior" title="Página anterior" disabled={page === 0 || loading} onclick={() => page--}><Icon name="chevron" /></button>
    <span>Página {page + 1} de {pages}</span>
    <button aria-label="Página siguiente" title="Página siguiente" disabled={page + 1 >= pages || loading} onclick={() => page++}><Icon name="chevron" /></button>

  </div>
  {#if loading}<p role="status">Abriendo original…</p>{/if}
  {#if error}<p role="alert">{error}</p>{/if}
  <div class="source-page">{#if image}<div class="source-image" style:width={`${fitWidth ? 100 : zoom}%`}><img src={image} alt={`Documento original, página ${page + 1}`}/></div>{/if}</div>
</section>
<style>
  .source-document { display:flex; flex-direction:column; gap:16px; min-width:0; padding:16px; border:1px solid var(--line); border-radius:10px; }
  .source-controls { display:flex; align-items:center; flex-wrap:wrap; gap:8px; }
  button { display:flex; align-items:center; justify-content:center; min-width:44px; min-height:44px; border:1px solid var(--line); border-radius:6px; background:var(--surface); color:var(--text); }
  button:first-child :global(svg) { transform:rotate(180deg); }
  .source-controls span { flex:1; text-align:center; }
  .source-page { width:100%; min-height:280px; overflow:auto; }
  .source-image { margin-inline:auto; }
  img { display:block; width:100%; height:auto; max-width:none; background:white; }
</style>
