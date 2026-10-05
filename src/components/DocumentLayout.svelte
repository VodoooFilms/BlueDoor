<script lang="ts">
  import { tick, untrack, onDestroy, type Snippet } from "svelte";
  import { DEFAULT_FORMAT } from "../lib/types";
  import TranscriptView from "./TranscriptView.svelte";
  import { htmlToPlain } from "../lib/editor/content";
  import type { DocFormat } from "../lib/types";
  import { invoke } from "@tauri-apps/api/core";
  import { privateCopy as p } from "../lib/private-i18n";
  import type { LayoutPreview, LayoutRedaction, PrivateEntity, PrivacySession } from "../lib/privacy";
  let { session, entities, dirty, busy, onAliasFocus, onAliasChange, onApply, onSessionChange, onTextDirty, onExportReady, zoom = $bindable(100), fitWidth = true, editable = true }: {
    session: PrivacySession; entities: PrivateEntity[]; dirty: boolean; busy: boolean;
    onAliasFocus: (index: number) => void; onAliasChange: (index: number, value: string) => void; onApply: () => void; onSessionChange: (session: PrivacySession) => void; onTextDirty: (dirty: boolean, save: () => void | Promise<void>) => void; onExportReady: (ready: boolean, count: number) => void; zoom?: number; fitWidth?: boolean; editable?: boolean;
  } = $props();
  let preview = $state<LayoutPreview | null>(null);
  let loading = $state(false);
  let error = $state("");
  let selected = $state<number | null>(null);
  let selectedRegion = $state<string | null>(null);
  let layoutRoot = $state<HTMLElement | null>(null);
  $effect(() => {
    const root = layoutRoot, pages = preview?.pages;
    void editable; void session.analyzed;
    if (!fitWidth || !root || !pages?.length) return;
    const width = Math.max(...pages.map(page => page.width));
    let cancelled = false;
    let observer: ResizeObserver | undefined;
    void tick().then(() => {
      if (cancelled) return;
      const viewer = root.querySelector<HTMLElement>(".viewer-body, .page-scroll");
      if (!viewer) return;
      const fit = () => {
        const style = getComputedStyle(viewer);
        const available = viewer.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight);
        // Hidden map views have no width; preserve the last usable scale.
        if (available > 0 && width > 0) zoom = Math.floor(available / width * 10000) / 100;
      };
      observer = new ResizeObserver(fit);
      observer.observe(viewer);
      fit();
    });
    return () => { cancelled = true; observer?.disconnect(); };
  });
  function regionKey(area: LayoutRedaction) { return `${area.page}:${area.entityIndex}:${area.rect.join(",")}`; }
  const aliasRegions = $derived([...(preview?.redactions ?? [])].filter(area => !!entities[area.entityIndex])
    .sort((a, b) => a.page - b.page || a.rect[1] - b.rect[1] || a.rect[0] - b.rect[0]));
  const selectedPosition = $derived(aliasRegions.findIndex(area => regionKey(area) === selectedRegion));
  function selectAlias(area: LayoutRedaction) {
    if (busy || loading || textDirty || boundsDirty) return;
    selectedBlock = null; selected = area.entityIndex; selectedRegion = regionKey(area);
    onAliasFocus(area.entityIndex);
  }
  async function moveAlias(delta: number) {
    const area = aliasRegions[selectedPosition + delta];
    if (!area || selectedPosition < 0) return;
    selectAlias(area); await tick();
    const target = Array.from(layoutRoot?.querySelectorAll<HTMLButtonElement>("[data-alias-region]") ?? [])
      .find(button => button.dataset.aliasRegion === regionKey(area));
    const viewer = target?.closest<HTMLElement>(".viewer-body, .page-scroll");
    if (target && viewer) {
      const box = target.getBoundingClientRect(), viewport = viewer.getBoundingClientRect();
      viewer.scrollBy({ left: box.left + box.width / 2 - viewport.left - viewport.width / 2,
        top: box.top + box.height / 2 - viewport.top - viewport.height / 2 });
    }
  }
  let selectedBlock = $state<number | null>(null);
  let textDirty = $state(false);
  let backgrounds = $state<Record<number, string>>({});
  const block = $derived(preview?.blocks?.find(b => b.id === selectedBlock));
  let draftBounds = $state<number[] | null>(null);
  let boundsHistory = $state<{ bounds: number[] | null; touched: boolean; order: number }[]>([]);
  let undoOrder = 0;
  function nextUndoOrder() { return ++undoOrder; }
  const activeBounds = $derived(draftBounds ?? block?.bounds);
  const boundsDirty = $derived(!!block && !!draftBounds && draftBounds.some((v, i) => Math.abs(v - block.bounds[i]) > 0.01));
  let contentHeight = $state(0);
  let heightTouched = $state(false);
  const visibleHeight = $derived(activeBounds && block ? Math.min(activeBounds[3],
    heightTouched || (block.limits && block.bounds[3] < block.limits[3] - 0.01)
      ? activeBounds[3] : Math.max(block.minHeight ?? block.rect[3], contentHeight)) : 0);
  const overflowing = $derived(!!activeBounds && contentHeight > activeBounds[3] + 1);
  let resizeController: AbortController | null = null;
  onDestroy(() => resizeController?.abort());
  let leaveTarget = $state<number | null | undefined>(undefined);
  let leaving = $state(false);
  let saveSucceeded = false;
  let leaveError = $state("");
  let exitDialog = $state<HTMLDialogElement | null>(null);
  $effect(() => {
    if (!exitDialog) return;
    if (leaveTarget !== undefined && !exitDialog.open) exitDialog.showModal();
    else if (leaveTarget === undefined && exitDialog.open) exitDialog.close();
  });
  function finishSelection(target: number | null = null) {
    resizeController?.abort(); selectedBlock = null; resetBounds(); contentHeight = 0;
    leaveTarget = undefined; leaveError = ""; textChanged(false);
    if (target !== null) selectBlock(target);
  }
  function requestFinish(target: number | null = null) {
    if (busy || leaving || target === selectedBlock) return;
    if (selectedBlock !== null && (textDirty || boundsDirty)) { leaveError = ""; leaveTarget = target; }
    else finishSelection(target);
  }
  async function saveAndLeave() {
    const target = leaveTarget; leaving = true; saveSucceeded = false; leaveError = "";
    try {
      await saveText(); await tick();
      if (saveSucceeded) finishSelection(target ?? null);
      // A failed save stays in this dialog with its actual error; never repeat
      // the generic unsaved-changes prompt after hiding the reason.
    } finally { leaving = false; }
  }
  function exitKey(event: KeyboardEvent) {
    if (event.key !== "Escape" || selectedBlock === null || leaveTarget !== undefined) return;
    event.preventDefault(); requestFinish();
  }
  function outsideParagraph(event: PointerEvent) {
    const target = event.target as HTMLElement;
    if (selectedBlock === null || leaveTarget !== undefined || !target.closest(".page-editor .viewer-body")) return;
    if (target.closest(".rich-editor, .text-resize, .alias-zone, dialog")) return;
    requestFinish();
  }
  function selectBlock(id: number) {
    selected = null; selectedRegion = null; selectedBlock = id; draftBounds = null; boundsHistory = []; contentHeight = 0; heightTouched = false;
  }
  function resetBounds() { draftBounds = null; boundsHistory = []; heightTouched = false; }
  function undoBounds() { const previous = boundsHistory.pop(); draftBounds = previous?.bounds ?? null; heightTouched = previous?.touched ?? false; }
  function changedBounds(side: "left" | "right" | "bottom", delta: number, start: number[]) {
    if (!block || !preview) return;
    const limits = block.limits ?? [0, start[1], preview.pages[block.page].width, start[3]];
    const end = start[0] + start[2];
    if (side === "bottom") {
      heightTouched = true;
      draftBounds = [start[0], start[1], start[2], Math.max(block.minHeight ?? block.rect[3], Math.min(limits[3], start[3] + delta))];
    } else if (side === "left") {
      const left = Math.max(limits[0], Math.min(end - 10, start[0] + delta));
      draftBounds = [left, start[1], end - left, start[3]];
    } else {
      const right = Math.min(limits[0] + limits[2], Math.max(start[0] + 10, end + delta));
      draftBounds = [start[0], start[1], right - start[0], start[3]];
    }
  }
  function resizeText(side: "left" | "right" | "bottom", event: PointerEvent) {
    if (!activeBounds || busy || loading) return;
    event.preventDefault();
    const previous = { bounds: draftBounds ? [...draftBounds] : null, touched: heightTouched };
    const start = [...activeBounds], x = side === "bottom" ? event.clientY : event.clientX, scale = zoom / 100;
    if (side === "bottom") start[3] = visibleHeight;
    resizeController?.abort();
    const controller = new AbortController(); resizeController = controller;
    const finish = () => {
      if (draftBounds?.some((v, i) => Math.abs(v - start[i]) > 0.01)) boundsHistory.push({ ...previous, order: nextUndoOrder() });
      controller.abort();
    };
    window.addEventListener("pointermove", e => changedBounds(side, ((side === "bottom" ? e.clientY : e.clientX) - x) / scale, start), { signal: controller.signal });
    window.addEventListener("pointerup", finish, { signal: controller.signal, once: true });
    window.addEventListener("pointercancel", () => { draftBounds = previous.bounds; heightTouched = previous.touched; controller.abort(); }, { signal: controller.signal, once: true });
  }
  function nudgeWidth(side: "left" | "right" | "bottom", event: KeyboardEvent) {
    if (!activeBounds || !(side === "bottom" ? ["ArrowUp", "ArrowDown"] : ["ArrowLeft", "ArrowRight"]).includes(event.key)) return;
    event.preventDefault(); const previous = { bounds: draftBounds ? [...draftBounds] : null, touched: heightTouched }; const start = [...activeBounds];
    if (side === "bottom") start[3] = visibleHeight;
    changedBounds(side, ["ArrowRight", "ArrowDown"].includes(event.key) ? 5 : -5, start);
    if (draftBounds?.some((v, i) => Math.abs(v - start[i]) > 0.01)) boundsHistory.push({ ...previous, order: nextUndoOrder() });
  }
  const blockTranscript = $derived(block ? {
    id: `${session.id}:${block.id}`, title: `${p("Editar texto")} · ${p("Página")} ${block.page + 1}`,
    text: htmlToPlain(block.html), corrected: htmlToPlain(block.html), correctedHtml: block.html,
    format: { font: block.font, size: block.size, align: block.align }, segments: [], createdAt: new Date(), duration: 0,
    language: "es", model: "local", audioSource: "file" as const,
  } : {
    id: `${session.id}:page`, title: p("Documento protegido"), text: "", corrected: "", correctedHtml: "", format: DEFAULT_FORMAT,
    segments: [], createdAt: new Date(), duration: 0, language: "es", model: "local", audioSource: "file" as const,
  });
  let saveText: () => void | Promise<void> = () => {};
  function textChanged(dirty: boolean, save?: () => void | Promise<void>) { if (save) saveText = save; textDirty = dirty; onTextDirty(dirty, saveText); }
  async function saveBlock(_id: string, _text: string, format?: DocFormat, html?: string | null) {
    if (!block || !html || !format) throw new Error(p("No se pudo guardar el texto."));
    const next = await invoke<PrivacySession>("save_private_layout_text", { id: session.id, revision: session.revision,
      edit: { blockId: block.id, html, font: format.font, size: format.size, align: format.align, bounds: activeBounds ? [...activeBounds] : undefined } });
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
    untrack(() => { if (preview?.id !== id) { preview = null; selectedBlock = null; selected = null; selectedRegion = null; backgrounds = {}; } });
    void invoke<LayoutPreview>("preview_private_layout", { id, revision }).then(next => {
      if (!cancelled && next.id === id && next.revision === revision) preview = next;
    }).catch(reason => { if (!cancelled) error = String(reason); })
      .finally(() => { if (!cancelled) loading = false; });
    return () => { cancelled = true; };
  });
  $effect(() => { onExportReady(!loading && !error && !!preview && !textDirty && !dirty, preview?.pages.length ?? 0); });
</script>

{#snippet aliasActions()}
  <div class="alias-actions">
    <button class="secondary" onclick={() => { if (selected !== null) onAliasChange(selected, session.entities[selected].alias); }} disabled={busy || !dirty}>{p("Restablecer alias")}</button>
    <button class="primary" onclick={onApply} disabled={busy || !dirty}>{p(dirty ? "Guardar cambios" : "Guardado")}</button>
  </div>
{/snippet}

{#snippet pages(editor?: Snippet<[string]>)}
  {#if loading}<p class="layout-note" role="status">{p("Preparando vista del documento…")}</p>{/if}
  {#if preview}
    {#each preview.pages as page, number (number)}
      <div class="page-wrapper" style:width={`${page.width * zoom / 100}px`} style:height={`${page.height * zoom / 100}px`}>
        <div class="paper-content" style:width={`${page.width}px`} style:height={`${page.height}px`} style:transform={`scale(${zoom / 100})`}>
          <img src={`data:image/png;base64,${page.imageBase64}`} alt={`${p("Página")} ${number + 1}`} draggable="false" onload={event => sampleBackground(event, number)} />
          {#if editable && session.analyzed}
            {#each preview.blocks ?? [] as item}
              {#if item.page === number}
                <button class="alias-zone text-zone" class:selected={selectedBlock === item.id}
                  aria-label={`${p("Editar texto")} ${item.id + 1}, ${p("Página")} ${number + 1}`}
                  title={htmlToPlain(item.html)} disabled={busy || loading || dirty || leaving}
                  style:left={`${item.rect[0]}px`} style:top={`${item.rect[1]}px`}
                  style:width={`${item.rect[2]}px`} style:height={`${item.rect[3]}px`}
                  onclick={() => requestFinish(item.id)}></button>
              {/if}
            {/each}
            {#if editor && block && activeBounds && block.page === number}
              <div class="text-cover" style:left={`${block.rect[0]}px`} style:top={`${block.rect[1]}px`}
                style:width={`${block.rect[2]}px`} style:height={`${block.rect[3]}px`} style:background={backgrounds[block.id] ?? "white"}></div>
              {@render editor(`;left:${activeBounds[0]}px;top:${activeBounds[1]}px;width:${activeBounds[2]}px;min-height:${Math.min(block.minHeight ?? block.rect[3], activeBounds[3])}px;max-height:${activeBounds[3]}px;overflow:auto;`)}
              <button class="text-resize left" aria-label={p("Borde izquierdo del área de texto")} title={p("Arrastra para ajustar el ancho del texto")}
                style:left={`${activeBounds[0] - 5}px`} style:top={`${activeBounds[1]}px`} style:height={`${Math.max(20, visibleHeight)}px`}
                disabled={busy || loading} onpointerdown={event => resizeText("left", event)} onkeydown={event => nudgeWidth("left", event)}></button>
              <button class="text-resize right" aria-label={p("Borde derecho del área de texto")} title={p("Arrastra para ajustar el ancho del texto")}
                style:left={`${activeBounds[0] + activeBounds[2] - 5}px`} style:top={`${activeBounds[1]}px`} style:height={`${Math.max(20, visibleHeight)}px`}
                disabled={busy || loading} onpointerdown={event => resizeText("right", event)} onkeydown={event => nudgeWidth("right", event)}></button>
              <button class="text-resize bottom" aria-label={p("Borde inferior del área de texto")} title={p("Arrastra para ajustar la altura del texto")}
                style:left={`${activeBounds[0]}px`} style:top={`${activeBounds[1] + visibleHeight - 4}px`} style:width={`${activeBounds[2]}px`}
                disabled={busy || loading} onpointerdown={event => resizeText("bottom", event)} onkeydown={event => nudgeWidth("bottom", event)}></button>
            {/if}
          {/if}
          {#each (editable && session.analyzed ? preview.redactions.filter(area => area.page === number) : []) as area}
            <button class="alias-zone" class:selected={selectedRegion === regionKey(area)} data-alias-region={regionKey(area)}
              aria-label={`${p("Editar alias")} ${entities[area.entityIndex]?.alias ?? area.alias}, ${p("Página")} ${number + 1}`}
              title={entities[area.entityIndex]?.alias ?? area.alias}
              style:left={`${area.rect[0]}px`} style:top={`${area.rect[1]}px`} style:width={`${area.rect[2]}px`} style:height={`${area.rect[3]}px`}
              disabled={busy || loading || textDirty || boundsDirty} onclick={() => selectAlias(area)}></button>
          {/each}
        </div>
      </div>
      <p class="page-number">{p("Página")} {number + 1} / {preview.pages.length}</p>
    {/each}
  {/if}
{/snippet}

<svelte:window onkeydown={exitKey} onpointerdown={outsideParagraph} />

<section bind:this={layoutRoot} class="layout-view" aria-label={p("Documento con formato original")}>

  {#if preview?.pages.some(page => page.qrRegions?.length)}<p class="layout-note" role="status">{p("QR detectados: cubiertos en blanco al proteger y exportar. Revisa si hay otros códigos sin detectar.")}</p>{/if}
  {#if preview?.estimated}<p class="layout-note">{p("Las zonas OCR son estimaciones. Comprueba que cada dato queda cubierto por completo.")}</p>{/if}
  {#if preview?.unlabeledAliases?.length}<p class="layout-note" role="status">{p("Las zonas muy pequeñas están protegidas sin texto de alias. Consulta sus referencias en el mapa privado.")} {preview.unlabeledAliases.join(", ")}</p>{/if}
  {#if dirty}<div class="draft-notice" role="status">{p("Hay cambios sin aplicar. La página muestra la última versión aplicada.")}</div>{/if}
  {#each session.warnings as warning}<p role="alert" class="layout-error">{p(warning)}</p>{/each}
  {#if overflowing}<p class="layout-note" role="status">{p("El texto supera el espacio libre. Amplía el área o ajusta el tamaño y los márgenes antes de guardar.")}</p>{/if}
  {#if error}<p role="alert" class="layout-error">{error}</p>{/if}
  {#if editable && session.analyzed}
    <TranscriptView transcript={blockTranscript} embedded privateDocument pageSurface={pages} surfaceActive={!!block && !busy && !loading} surfaceScale={zoom / 100} onSurfaceExtent={height => contentHeight = height}
      externalDirty={boundsDirty} externalCanUndo={boundsHistory.length > 0} externalUndoOrder={boundsHistory[boundsHistory.length - 1]?.order ?? -1} {nextUndoOrder} onResetExternal={resetBounds} onUndoExternal={undoBounds}
      onSaveOutcome={(success, reason) => { saveSucceeded = success; if (!success) leaveError = reason ?? p("No se pudo guardar el texto."); }} onFinishEditing={() => requestFinish()} actions={selected !== null ? aliasActions : undefined} onSave={saveBlock} onDirtyChange={textChanged} onSaved={() => { if (!leaving) finishSelection(); }} />
    {#if selected !== null && entities[selected]}
      <aside class="alias-editor" aria-label={p("Editar alias")}>
        <strong>{p("Alias")}</strong>
        <span>{entities[selected].original}</span>
        <input aria-label={p("Alias en el documento")} value={entities[selected].alias}
          onfocus={() => { if (selected !== null) onAliasFocus(selected); }} oninput={event => { if (selected !== null) onAliasChange(selected, event.currentTarget.value); }} disabled={busy} maxlength="120" spellcheck="false" />
        <span>{p("El cambio se aplica a todas las entidades y apariciones que comparten este alias.")}</span>
        <div class="alias-navigation" role="group" aria-label={p("Navegar cajas de alias")}>
          <button class="secondary" aria-label={p("Caja de alias anterior")} title={p("Caja de alias anterior")} onclick={() => moveAlias(-1)} disabled={busy || loading || selectedPosition <= 0}>←</button>
          <span aria-live="polite">{selectedPosition + 1} / {aliasRegions.length}{#if selectedPosition >= 0}<br />{p("Página")} {aliasRegions[selectedPosition].page + 1}{/if}</span>
          <button class="secondary" aria-label={p("Caja de alias siguiente")} title={p("Caja de alias siguiente")} onclick={() => moveAlias(1)} disabled={busy || loading || selectedPosition < 0 || selectedPosition >= aliasRegions.length - 1}>→</button>
          <button class="secondary alias-close" aria-label={p("Cerrar editor de alias")} onclick={() => { selected = null; selectedRegion = null; }} disabled={dirty || busy}>×</button>
        </div>
      </aside>
    {/if}
  {:else}
    <div class="page-scroll">{@render pages()}</div>
  {/if}
  <dialog bind:this={exitDialog} oncancel={event => { event.preventDefault(); if (!leaving) leaveTarget = undefined; }} aria-label={p("Salir de la edición del párrafo")}>
    {#if leaveError}<p role="alert">{p("No se guardó el párrafo. Puedes seguir editando o descartar el borrador para salir.")}</p><p role="alert">{leaveError}</p>{:else}<p>{p("Hay cambios sin guardar en este párrafo.")}</p>{/if}
    <div class="exit-actions">
      <button class="secondary" disabled={leaving} onclick={() => leaveTarget = undefined}>{p("Seguir editando")}</button>
      <button class="secondary" disabled={leaving} onclick={() => finishSelection(leaveTarget ?? null)}>{p("Descartar cambios")}</button>
      <button class="primary" disabled={leaving} onclick={saveAndLeave}>{p("Guardar y salir")}</button>
    </div>
  </dialog>
</section>

<style>
  dialog { border:1px solid var(--line); border-radius:6px; padding:22px; background:var(--surface); color:var(--text); max-width:480px; }
  dialog::backdrop { background:#0004; }
  .exit-actions { display:flex; gap:8px; flex-wrap:wrap; }
  .layout-view { position:relative; display:flex; flex-direction:column; flex:1; min-height:0; min-width:0; border:1px solid var(--line); border-radius:5px; overflow:hidden; }
  .layout-note { font-size:12px; color:var(--muted); }
  .layout-note { padding:8px 16px; margin:0; }
  .page-scroll { flex:1; min-width:0; overflow:auto; scrollbar-gutter:stable; padding:22px; background:var(--panel); }
  .page-wrapper { position:relative; margin:0 auto; background:white; box-shadow:0 2px 12px #0002; }
  .paper-content { position:relative; transform-origin:top left; }
  img { display:block; width:100%; height:100%; }
  .alias-zone { position:absolute; border:1px solid transparent; padding:0; min-width:0; border-radius:1px; background:transparent; cursor:pointer; }
  .alias-zone:hover, .alias-zone:focus-visible, .alias-zone.selected { outline:2px solid #9271ce; outline-offset:1px; background:#9271ce15; }
  .text-cover { position:absolute; pointer-events:none; z-index:3; }
  .text-resize { position:absolute; z-index:5; width:10px; padding:0; border:0; border-radius:2px; background:#9271ce; cursor:ew-resize; touch-action:none; opacity:.8; }
  .text-resize.bottom { height:8px; cursor:ns-resize; opacity:.45; }
  .text-resize:focus-visible { outline:2px solid var(--text); outline-offset:2px; }
  .page-number { font-size:11px; color:var(--muted); text-align:center; margin:10px 0 24px; }
  .alias-editor { position:absolute; right:16px; top:132px; z-index:10; width:260px; display:flex; flex-direction:column; gap:12px; padding:18px; border:1px solid var(--line); border-radius:6px; background:var(--surface); box-shadow:0 8px 24px #0002; }
  .alias-editor span { font-size:12px; color:var(--muted); overflow-wrap:anywhere; }
  .alias-navigation { display:flex; align-items:center; gap:8px; }
  .alias-navigation span { flex:1; text-align:center; font-size:11px; }
  .alias-navigation button { flex:none; }
  .alias-close { margin-left:4px; }
  .alias-actions { margin-left:auto; display:flex; align-items:center; gap:8px; }
  .alias-zone:not(.text-zone) { z-index:2; }
  input { font:inherit; color:var(--text); background:var(--background); border:1px solid var(--line); border-radius:4px; padding:7px; min-width:0; }
  button { cursor:pointer; border-radius:4px; padding:8px 12px; font-size:12px; }
  button:disabled { opacity:.45; cursor:default; }
  .primary { background:var(--text); color:var(--surface); border:1px solid var(--text); }
  .secondary { background:var(--surface); color:var(--text); border:1px solid var(--line); }

  .draft-notice { padding:10px 16px; background:var(--panel); font-size:12px; }
  .layout-error { padding:12px; color:var(--error); }
</style>
