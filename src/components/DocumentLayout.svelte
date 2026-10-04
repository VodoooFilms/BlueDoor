<script lang="ts">
  import { untrack, type Snippet } from "svelte";
  import { DEFAULT_FORMAT } from "../lib/types";
  import TranscriptView from "./TranscriptView.svelte";
  import { htmlToPlain } from "../lib/editor/content";
  import type { DocFormat } from "../lib/types";
  import { invoke } from "@tauri-apps/api/core";
  import { privateCopy as p } from "../lib/private-i18n";
  import type { LayoutPreview, PrivateEntity, PrivacySession } from "../lib/privacy";
  let { session, entities, dirty, busy, onAliasFocus, onAliasChange, onApply, onExport, onSessionChange, onTextDirty, onMap, editable = true }: {
    session: PrivacySession; entities: PrivateEntity[]; dirty: boolean; busy: boolean;
    onAliasFocus: (index: number) => void; onAliasChange: (index: number, value: string) => void; onApply: () => void; onExport: () => Promise<void>; onSessionChange: (session: PrivacySession) => void; onTextDirty: (dirty: boolean) => void; onMap: () => void; editable?: boolean;
  } = $props();
  let preview = $state<LayoutPreview | null>(null);
  let loading = $state(false);
  let error = $state("");
  let selected = $state<number | null>(null);
  let zoom = $state(100);
  let textMode = $state(true);
  let selectedBlock = $state<number | null>(null);
  let textDirty = $state(false);
  let backgrounds = $state<Record<number, string>>({});
  const block = $derived(preview?.blocks?.find(b => b.id === selectedBlock));
  const blockTranscript = $derived(block ? {
    id: `${session.id}:${block.id}`, title: `${p("Editar texto")} · ${p("Página")} ${block.page + 1}`,
    text: htmlToPlain(block.html), corrected: htmlToPlain(block.html), correctedHtml: block.html,
    format: { font: block.font, size: block.size, align: block.align }, segments: [], createdAt: new Date(), duration: 0,
    language: "es", model: "local", audioSource: "file" as const,
  } : {
    id: `${session.id}:page`, title: p("Documento protegido"), text: "", corrected: "", correctedHtml: "", format: DEFAULT_FORMAT,
    segments: [], createdAt: new Date(), duration: 0, language: "es", model: "local", audioSource: "file" as const,
  });
  function textChanged(dirty: boolean) { textDirty = dirty; onTextDirty(dirty); }
  async function saveBlock(_id: string, _text: string, format?: DocFormat, html?: string | null) {
    if (!block || !html || !format) throw new Error(p("No se pudo guardar el texto."));
    const next = await invoke<PrivacySession>("save_private_layout_text", { id: session.id, revision: session.revision,
      edit: { blockId: block.id, html, font: format.font, size: format.size, align: format.align } });
    textChanged(false); onSessionChange(next);
  }
  function sampleBackground(event: Event, pageNumber: number) {
    const img = event.currentTarget as HTMLImageElement;
    if (!preview) return;
    const page = preview.pages[pageNumber];
    const canvas = document.createElement("canvas"); canvas.width = 1; canvas.height = 1;
    const context = canvas.getContext("2d"); if (!context) return;
    const colors: Record<number, string> = {};
    for (const item of preview.blocks ?? []) {
      if (item.page !== pageNumber) continue;
      const x = Math.max(0, item.rect[0] - 2) / page.width * img.naturalWidth;
      const y = Math.max(0, item.rect[1] - 2) / page.height * img.naturalHeight;
      context.drawImage(img, x, y, 1, 1, 0, 0, 1, 1);
      const [r,g,b] = context.getImageData(0,0,1,1).data;
      colors[item.id] = `rgb(${r}, ${g}, ${b})`;
    }
    backgrounds = { ...backgrounds, ...colors };
  }
  $effect(() => {
    const id = session.id, revision = session.revision;
    let cancelled = false;
    loading = true; error = "";
    untrack(() => { if (preview?.id !== id) { preview = null; selectedBlock = null; selected = null; backgrounds = {}; } });
    void invoke<LayoutPreview>("preview_private_layout", { id, revision }).then(next => {
      if (!cancelled && next.id === id && next.revision === revision) preview = next;
    }).catch(reason => { if (!cancelled) error = String(reason); })
      .finally(() => { if (!cancelled) loading = false; });
    return () => { cancelled = true; };
  });
</script>

{#snippet pages(editor?: Snippet<[string]>)}
  {#if loading}<p class="layout-note" role="status">{p("Preparando vista del documento…")}</p>{/if}
  {#if preview}
    {#each preview.pages as page, number (number)}
      <div class="page-wrapper" style:width={`${page.width * zoom / 100}px`} style:height={`${page.height * zoom / 100}px`}>
        <div class="paper-content" style:width={`${page.width}px`} style:height={`${page.height}px`} style:transform={`scale(${zoom / 100})`}>
          <img src={`data:image/png;base64,${page.imageBase64}`} alt={`${p("Página")} ${number + 1}`} draggable="false" onload={event => sampleBackground(event, number)} />
          {#if editable && textMode && session.analyzed}
            {#each preview.blocks ?? [] as item}
              {#if item.page === number}
                <button class="alias-zone text-zone" class:selected={selectedBlock === item.id}
                  aria-label={`${p("Editar texto")} ${item.id + 1}, ${p("Página")} ${number + 1}`}
                  title={htmlToPlain(item.html)} disabled={busy || loading || dirty || textDirty}
                  style:left={`${item.rect[0]}px`} style:top={`${item.rect[1]}px`}
                  style:width={`${item.rect[2]}px`} style:height={`${item.rect[3]}px`}
                  onclick={() => selectedBlock = item.id}></button>
              {/if}
            {/each}
            {#if editor && block && block.page === number}
              <div class="text-cover" style:left={`${block.rect[0]}px`} style:top={`${block.rect[1]}px`}
                style:width={`${block.rect[2]}px`} style:height={`${block.rect[3]}px`} style:background={backgrounds[block.id] ?? "white"}></div>
              {@render editor(`;left:${block.bounds[0]}px;top:${block.bounds[1]}px;width:${block.bounds[2]}px;min-height:${block.rect[3]}px;`)}
            {/if}
          {/if}
          {#each (editable && !textMode ? preview.redactions.filter(area => area.page === number) : []) as area}
            <button class="alias-zone" class:selected={selected === area.entityIndex}
              aria-label={`${p("Editar alias")} ${entities[area.entityIndex]?.alias ?? area.alias}, ${p("Página")} ${number + 1}`}
              title={entities[area.entityIndex]?.alias ?? area.alias}
              style:left={`${area.rect[0]}px`} style:top={`${area.rect[1]}px`} style:width={`${area.rect[2]}px`} style:height={`${area.rect[3]}px`}
              disabled={busy || loading} onclick={() => selected = area.entityIndex}></button>
          {/each}
        </div>
      </div>
      <p class="page-number">{p("Página")} {number + 1} / {preview.pages.length}</p>
    {/each}
  {/if}
{/snippet}

<section class="layout-view" aria-label={p("Documento con formato original")}>
  <div class="layout-toolbar">
    <div><strong>{p(editable ? "Documento protegido" : "Formato original")}</strong><span>{preview?.pages.length ?? "…"} {p("páginas")}</span></div>
    <label>{p("Zoom")} <select bind:value={zoom} disabled={textDirty}><option value={75}>75%</option><option value={100}>100%</option><option value={150}>150%</option><option value={200}>200%</option></select></label>
    {#if editable && session.analyzed}
      <button class="secondary" class:chosen={textMode} disabled={textDirty || dirty} onclick={() => { textMode = true; selected = null; }}>{p("Editar texto")}</button>
      <button class="secondary" class:chosen={!textMode} disabled={textDirty} onclick={() => { textMode = false; selectedBlock = null; }}>{p("Editar aliases")}</button>
      <button class="secondary" onclick={onMap} disabled={busy || textDirty}>{p("Mapa privado")}</button>
      <button class="primary" onclick={onExport} disabled={busy || loading || dirty || textDirty || !!error || !preview || !session.analysisComplete || !!session.warnings.length}>{p("Exportar PDF")}</button>
    {/if}
  </div>
  {#if editable && session.analyzed}<p class="layout-note">{p("Edita directamente sobre la página. Guarda las correcciones antes de exportar.")}</p>{/if}
  {#if preview?.estimated}<p class="layout-note">{p("Las zonas OCR son estimaciones. Comprueba que cada dato queda cubierto por completo.")}</p>{/if}
  {#if dirty}<div class="draft-notice" role="status">{p("Hay cambios sin aplicar. La página muestra la última versión aplicada.")} <button class="secondary" onclick={onApply} disabled={busy}>{p("Aplicar cambios al mapa")}</button></div>{/if}
  {#each session.warnings as warning}<p role="alert" class="layout-error">{p(warning)}</p>{/each}
  {#if error}<p role="alert" class="layout-error">{error}</p>{/if}
  {#if editable && textMode && session.analyzed}
    <TranscriptView transcript={blockTranscript} embedded privateDocument pageSurface={pages} surfaceActive={!!block && !busy && !loading} surfaceScale={zoom / 100}
      onSave={saveBlock} onDirtyChange={textChanged} onSaved={() => { selectedBlock = null; textChanged(false); }} />
  {:else}
    {#if editable && !textMode && selected !== null && entities[selected]}
      <div class="alias-editor" aria-label={p("Editar alias")}>
        <label>{p("Alias")}<input aria-label={p("Alias en el documento")} value={entities[selected].alias}
          onfocus={() => { if (selected !== null) onAliasFocus(selected); }} oninput={event => { if (selected !== null) onAliasChange(selected, event.currentTarget.value); }} disabled={busy} maxlength="120" spellcheck="false" /></label>
        <span>{p("El cambio se aplica a todas las entidades y apariciones que comparten este alias.")}</span>
        <button class="secondary" onclick={onApply} disabled={busy || !dirty}>{p("Aplicar cambios al mapa")}</button>
        <button class="secondary" aria-label={p("Cerrar editor de alias")} onclick={() => selected = null}>×</button>
      </div>
    {/if}
    <div class="page-scroll">{@render pages()}</div>
  {/if}
</section>

<style>
  .layout-view { display:flex; flex-direction:column; flex:1; min-height:0; border:1px solid var(--line); border-radius:5px; overflow:hidden; }
  .layout-toolbar { display:flex; align-items:center; flex-wrap:wrap; gap:12px; padding:12px 16px; background:var(--surface); border-bottom:1px solid var(--line); }
  .layout-toolbar > div { display:flex; align-items:center; gap:10px; margin-right:auto; }
  .layout-toolbar span, .layout-toolbar label, .layout-note { font-size:12px; color:var(--muted); }
  .layout-note { padding:8px 16px; margin:0; }
  .page-scroll { flex:1; min-width:0; overflow:auto; padding:22px; background:var(--panel); }
  .page-wrapper { position:relative; margin:0 auto; background:white; box-shadow:0 2px 12px #0002; }
  .paper-content { position:relative; transform-origin:top left; }
  img { display:block; width:100%; height:100%; }
  .alias-zone { position:absolute; border:1px solid transparent; padding:0; min-width:0; border-radius:1px; background:transparent; cursor:pointer; }
  .alias-zone:hover, .alias-zone:focus-visible, .alias-zone.selected { outline:2px solid #9271ce; outline-offset:1px; background:#9271ce15; }
  .text-cover { position:absolute; pointer-events:none; z-index:3; }
  .page-number { font-size:11px; color:var(--muted); text-align:center; margin:10px 0 24px; }
  .alias-editor { display:flex; align-items:center; flex-wrap:wrap; gap:14px; padding:10px 16px; border-bottom:1px solid var(--line); background:var(--surface); }
  .alias-editor label { display:flex; align-items:center; gap:8px; font-size:12px; }
  .alias-editor span { flex:1; font-size:12px; color:var(--muted); }
  input, select { font:inherit; color:var(--text); background:var(--background); border:1px solid var(--line); border-radius:4px; padding:7px; min-width:0; }
  button { cursor:pointer; border-radius:4px; padding:8px 12px; font-size:12px; }
  button:disabled { opacity:.45; cursor:default; }
  .primary { background:var(--text); color:var(--surface); border:1px solid var(--text); }
  .secondary { background:var(--surface); color:var(--text); border:1px solid var(--line); }
  .chosen { box-shadow:inset 0 -2px var(--accent); }
  .draft-notice { padding:10px 16px; background:var(--panel); font-size:12px; }
  .draft-notice button { margin-left:10px; }
  .layout-error { padding:12px; color:var(--error); }
</style>
