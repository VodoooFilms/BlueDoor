<script lang="ts">
  import { privateCopy as p } from "../lib/private-i18n";
  import { onMount, onDestroy } from "svelte";
  import { invoke, isTauri } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { ENTITY_KINDS, type EntityKind, type PrivateEntity, type PrivacySession } from "../lib/privacy";
  import importArt from "../assets/import-document.svg";
  import protectedArt from "../assets/protected-document.svg";
  import Icon from "./Icon.svelte";
  import DocumentLayout from "./DocumentLayout.svelte";
  import TranscriptView from "./TranscriptView.svelte";
  import type { Transcript, DocFormat } from "../lib/types";
  import { pdfFileName } from "../lib/editor/export";
  import { enterView } from "../lib/motion";
  import SaveNotice from "./SaveNotice.svelte";
  let saveSequence = $state(0);

  let { onBusyChange, onSessionChange, onEdit, externalSession, comparisonRequest, textTranscript, onSaveText, onTextDirty, onConfirmReview, onValidateExport, editing = false }: { textTranscript: Transcript | null; onSaveText: (id: string, text: string, format?: DocFormat, html?: string | null) => Promise<void>; onTextDirty: (dirty: boolean, save: () => void | Promise<void>) => void; onConfirmReview: () => Promise<void>; onValidateExport: (text: string, title?: string) => Promise<void>; editing?: boolean; onBusyChange: (busy: boolean) => void; onSessionChange: (session: PrivacySession | null) => void; onEdit: (session: PrivacySession) => void; externalSession: PrivacySession | null; comparisonRequest: number } = $props();
  let session = $state<PrivacySession | null>(null);
  let entities = $state<PrivateEntity[]>([]);
  let draftDirty = $state(false);
  let layoutDirty = $state(false);
  function layoutEdited(dirty: boolean, save: () => void | Promise<void>) { layoutDirty = dirty; onTextDirty(dirty, save); }
  let aliasEdit: { index: number; members: number[] } | null = null;
  let busy = $state("");
  let sanitizing = $state(false);
  let analysisStep = $state("");
  let analysisId: string | null = null;
  let analysisRevision: number | null = null;
  let unlistenAnalysis: (() => void) | undefined;
  let disposed = false;
  let importZone = $state<HTMLButtonElement>();
  let draggingDocument = $state(false);
  let unlistenDrop: (() => void) | undefined;
  onMount(() => {
    if (!isTauri()) return;
    getCurrentWebview().onDragDropEvent(event => {
      const drop = event.payload;
      if (drop.type === "leave") { draggingDocument = false; return; }
      const bounds = importZone?.getBoundingClientRect();
      const x = drop.position.x / window.devicePixelRatio;
      const y = drop.position.y / window.devicePixelRatio;
      const inside = !session && !busy && !!bounds && x >= bounds.left && x <= bounds.right && y >= bounds.top && y <= bounds.bottom;
      draggingDocument = inside && drop.type !== "drop";
      if (drop.type !== "drop" || !inside) return;
      if (drop.paths.length !== 1) { error = "Arrastra un solo documento a la vez."; return; }
      void importDocument(drop.paths[0]);
    }).then(stop => { if (disposed) stop(); else unlistenDrop = stop; }).catch(() => {});
  });
  const stepLabel = $derived.by(() => {
    const match = /^(reading|ocr):(\d+)\/(\d+)(?::(\d+))?$/.exec(analysisStep);
    if (!match) return "";
    const [, kind, done, total, tokens] = match;
    // The dictionary key is the template itself, so translate first and fill
    // the counts afterwards; interpolating before p() would never match.
    const template = kind === "ocr"
      ? p("Corrigiendo OCR · fragmento {0} de {1}")
      : p("Leyendo con AI local · fragmento {0} de {1}");
    // A running token count proves the model is advancing. Without it a
    // fragment that spends its full budget shows one frozen label for minutes.
    const filled = template.replace("{0}", done).replace("{1}", total);
    return tokens ? `${filled} · ${tokens} tokens` : filled;
  });
  const processingLabel = $derived(busy.startsWith("Analizando")
    ? p(sanitizing ? "Protegiendo el documento" : "Analizando con AI local")
    : p(busy).replace(/(?:…|\.{3})$/, ""));
  let error = $state("");
  let addText = $state("");
  let addKind = $state<EntityKind>("PERSON");
  let mapOpen = $state(false);
  const ZOOM_LEVELS = [75, 100, 125, 150, 200, 250, 300, 400, 500, 600, 800];
  let zoom = $state(100);
  let fitWidth = $state(true);
  $effect(() => { void editing; fitWidth = true; });
  function chooseZoom(event: Event) {
    const value = (event.currentTarget as HTMLSelectElement).value;
    fitWidth = value === "fit";
    if (!fitWidth) zoom = Number(value);
  }
  let exportReady = $state(false);
  let pageCount = $state(0);
  let textEditorDirty = $state(false);
  let textExport = $state<(() => Promise<void>) | null>(null);
  let textExportBusy = $state(false);
  let exported = $state("");
  $effect(() => {
    void comparisonRequest;
    void editing;
    mapOpen = false;
  });
  function adopt(next: PrivacySession | null) {
    if (next?.id !== session?.id) { zoom = 100; fitWidth = true; exported = ""; exportReady = false; }
    if (!next?.analyzed || next.id !== session?.id) mapOpen = false;
    session = next;
    onSessionChange(next);
    entities = next?.entities.map(e => ({ ...e })) ?? [];
    draftDirty = false;
    if (!layoutDirty && !textEditorDirty) onTextDirty(false, () => applyMap());
    aliasEdit = null;
  }
  function edited() { draftDirty = true; error = ""; onTextDirty(true, () => applyMap()); }
  function resetMap() {
    entities = session?.entities.map(entity => ({ ...entity })) ?? [];
    draftDirty = false; aliasEdit = null; error = "";
    onTextDirty(false, () => applyMap());
  }
  function beginAliasEdit(index: number) {
    const alias = entities[index]?.alias;
    if (alias === undefined) return;
    // Keep the original group throughout typing, even if an intermediate value
    // temporarily matches a different group's alias.
    aliasEdit = { index, members: entities.flatMap((entity, i) =>
      (alias.trim() ? entity.alias.trim() === alias.trim() : i === index) ? [i] : []) };
  }
  function renameAlias(index: number, value: string) {
    if (aliasEdit?.index !== index) beginAliasEdit(index);
    const members = new Set(aliasEdit?.members ?? []);
    entities = entities.map((entity, i) => members.has(i) ? { ...entity, alias: value } : entity);
    edited();
  }
  $effect(() => { if (externalSession && (session?.id !== externalSession.id || session.revision !== externalSession.revision)) adopt(externalSession); });
  onMount(async () => {
    const stop = await listen<[string, number, string]>("private-analysis-progress", event => {
      const [id, revision, phase] = event.payload;
      if (busy.startsWith("Analizando") && id === analysisId && revision === analysisRevision) {
        // `reading:d/t` and `ocr:d/t` come from the Rust analysis loop so the
        // lawyer sees which fragment is being read instead of a frozen label.
        // Sanitizing is not a fragment step, so it must clear the step or the
        // stale fragment label keeps masking the final phase.
        if (phase === "sanitizing") { sanitizing = true; analysisStep = ""; }
        else analysisStep = phase;
      }
    });
    if (disposed) { stop(); return; }
    unlistenAnalysis = stop;
    try {
      const restored = await invoke<PrivacySession | null>("get_privacy_session");
      adopt(restored);
      if (restored?.analyzed) onEdit(restored);
    } catch { error = "Abre el demo con Tauri para importar documentos y ejecutar Qwen localmente."; }
  });
  onDestroy(() => { disposed = true; unlistenAnalysis?.(); unlistenDrop?.(); });

  async function run(label: string, action: () => Promise<void>) {
    if (busy) return;
    busy = label; error = ""; onBusyChange(true);
    try { await action(); }
    catch (e) {
      error = String(e);
      if (label.startsWith("Analizando")) {
        try { adopt(await invoke<PrivacySession | null>("get_privacy_session")); } catch { }
      }
    }
    finally { busy = ""; sanitizing = false; analysisId = null; analysisRevision = null; onBusyChange(false); }
  }
  async function importDocument(path?: string) {
    await run("Importando documento local…", async () => {
      const next = await invoke<PrivacySession | null>("import_private_document", { path: path ?? null });
      if (next) adopt(next);
    });
  }
  async function analyze() {
    if (!session || busy) return;
    const current = session;
    analysisId = current.id;
    analysisRevision = current.revision;
    sanitizing = false;
    analysisStep = "";
    await run("Analizando documento local…", async () => {
      const modelId = localStorage.getItem("bluedoor-privacy-model") || "qwen3-fast";
      const analyzed = await invoke<PrivacySession>("analyze_private_document", { id: current.id, revision: current.revision, modelId });
      adopt(analyzed);
      if (analyzed.analyzed) onEdit(analyzed);
    });
  }
  function addEntity() {
    if (!addText.trim()) return;
    const used = new Set(entities.map(e => e.alias));
    let n = 1;
    while (used.has(`${addKind}_${String(n).padStart(2, "0")}`)) n++;
    entities = [...entities, { kind: addKind, original: addText.trim(), alias: `${addKind}_${String(n).padStart(2, "0")}`, source: "review" }];
    addText = ""; edited();
  }
  async function applyMap(confirm = false) {
    if (!session) return;
    const current = session;
    await run("Validando el mapa local…", async () => {
      adopt(await invoke<PrivacySession>(current.layoutAvailable ? "review_private_layout" : "review_private_document", { id: current.id, revision: current.revision, entities: $state.snapshot(entities), confirm }));
      saveSequence++;
    });
  }
  async function clearResult() {
    if (!session) return;
    const current = session;
    await run("Borrando resultado local…", async () => {
      adopt(await invoke<PrivacySession>("reset_private_analysis", { id: current.id, revision: current.revision }));
    });
  }
  async function clear() {
    await run("Cerrando documento…", async () => { await invoke("clear_privacy_session"); adopt(null); });
  }
  $effect(() => { onBusyChange(!!busy || layoutDirty || draftDirty || textEditorDirty); });
  async function exportOriginalLayout() {
    if (!session || draftDirty) return;
    await run("Exportando PDF con formato original…", async () => {
      const reviewed = await invoke<PrivacySession>("review_private_layout", { id: session!.id, revision: session!.revision, entities: $state.snapshot(entities), confirm: true });
      adopt(reviewed);
      if (!reviewed.reviewed) throw new Error(p("Revisa el mapa privado antes de exportar."));
      const destPath = await invoke<string | null>("choose_pdf_destination", { defaultName: pdfFileName("Protected") });
      if (!destPath) return;
      exported = await invoke<string>("export_private_layout", { id: reviewed.id, revision: reviewed.revision, destPath });
    });
  }
</script>

{#snippet processingIndicator()}
  <div class="processing-slot">
    <div class="processing-label" role="status" aria-live="polite">
      <span class="processing-orbit" aria-hidden="true"></span><span class="processing-copy" title={stepLabel || processingLabel}>{stepLabel || processingLabel}</span>
    </div>
    <div class="processing-track processing" role="progressbar" aria-label={p("Procesando documento")} aria-busy="true"><span></span></div>
  </div>
{/snippet}

<section class="privacy-workflow" aria-label="Private Legal" use:enterView={`${session?.id ?? "welcome"}:${editing}:${mapOpen}`}>
  <SaveNotice sequence={saveSequence} />
  {#if session}
    <div class="workspace-header">
      <div class="workspace-title"><strong>{session.fileName}</strong><span>{p(session.analyzed ? (session.reviewed ? "Revisión confirmada" : "Pendiente de revisión") : "Documento original")}{#if pageCount} · {pageCount} {p("páginas")}{/if}</span></div>
      {#if editing && session.analyzed}
        <div class="workspace-views" role="group" aria-label={p("Vistas de edición")}>
          <button class="secondary" class:active-page={!mapOpen} aria-pressed={!mapOpen} disabled={!!busy || layoutDirty || textEditorDirty || draftDirty} onclick={() => mapOpen = false} aria-label={p("Documento")} title={p("Documento")}><Icon name="document" /></button>
          <button class="secondary" class:active-page={mapOpen} aria-pressed={mapOpen} disabled={!!busy || layoutDirty || textEditorDirty || draftDirty} onclick={() => mapOpen = true} aria-label={p("Mapa privado")} title={p("Mapa privado")}><Icon name="map" /></button>
        </div>
        {#if session.layoutAvailable}
          <label class="workspace-zoom"><span class="sr-only">{p("Zoom")}</span><Icon name="search" size={18} /> <select value={fitWidth ? "fit" : zoom} onchange={chooseZoom} disabled={!!busy || mapOpen}><option value="fit">{p("Ajustar al ancho")}</option>{#each ZOOM_LEVELS as level}<option value={level}>{level}%</option>{/each}</select></label>
        {/if}
        <button class="primary" onclick={() => session?.layoutAvailable ? exportOriginalLayout() : textExport?.()} disabled={!!busy || textExportBusy || (session.layoutAvailable && !exportReady) || (!session.layoutAvailable && !textExport) || draftDirty || layoutDirty || !session.analysisComplete || !!session.warnings.length}>{p("Exportar PDF")}</button>
      {:else}
        <details class="document-menu"><summary aria-label={p("Acciones del documento")}>···</summary><div>
          {#if session.analyzed}<button class="secondary" onclick={clearResult} disabled={!!busy}>{p("Volver a analizar")}</button>{/if}
          <button class="secondary" onclick={clear} disabled={!!busy}>{p("Borrar documento y empezar de cero")}</button>
        </div></details>
        <button class="secondary" onclick={() => importDocument()} disabled={!!busy}>{p("Importar otro documento")}</button>
        {#if session.layoutAvailable}<label class="workspace-zoom"><span class="sr-only">{p("Zoom")}</span><Icon name="search" size={18} /> <select value={fitWidth ? "fit" : zoom} onchange={chooseZoom} disabled={!!busy}><option value="fit">{p("Ajustar al ancho")}</option>{#each ZOOM_LEVELS as level}<option value={level}>{level}%</option>{/each}</select></label>{/if}
        {#if session.analyzed}<button class="primary" onclick={() => { if (session) onEdit(session); }} disabled={!!busy}>{p("Revisar y editar")}</button>
        {:else}<button class="primary analyze-button" onclick={analyze} disabled={!!busy}>{p("Proteger documento")}</button>{/if}
      {/if}
    </div>
    {#if mapOpen && editing}
        <section class="map-page" aria-label={p("Mapa privado")}>
          <div class="map-heading"><h2>{p("Mapa privado")}</h2><span>{entities.length} {p("entradas · solo local")}</span></div>
          {#if session.analyzed}
            <p class="map-help">{p("Corrige detecciones y aliases. Cambiar un alias actualiza todas las entradas que lo comparten. Para unir variantes usa el mismo alias. Eliminar una entrada restaura ese dato en la salida: revisa con cuidado.")}</p>
            <div class="entity-list">
              {#each entities as entity, i}
                <div class="entity-row">
                  <label><span>{p("Categoría")}</span><select bind:value={entity.kind} onchange={edited} disabled={!!busy}>{#each ENTITY_KINDS as kind}<option value={kind.value}>{p(kind.label)}</option>{/each}</select></label>
                  <label><span>{p("Valor privado")}</span><input aria-label={`${p("Valor privado")} ${i+1}`} bind:value={entity.original} oninput={edited} disabled={!!busy}/></label>
                  <label><span>{p("Alias")}</span><input aria-label={`Alias ${i+1}`} value={entity.alias} onfocus={() => beginAliasEdit(i)} oninput={event => renameAlias(i, event.currentTarget.value)} disabled={!!busy} spellcheck="false"/></label>
                  <button class="remove" aria-label={`${p("Eliminar entidad")} ${i+1}`} disabled={!!busy} onclick={() => { entities = entities.filter((_, n) => n !== i); aliasEdit = null; edited(); }}>×</button>
                </div>
              {/each}
            </div>
            <div class="add-entity"><select aria-label={p("Tipo de nueva entidad")} bind:value={addKind} disabled={!!busy}>{#each ENTITY_KINDS as kind}<option value={kind.value}>{p(kind.label)}</option>{/each}</select><input aria-label={p("Nueva entidad privada")} bind:value={addText} placeholder={p("Texto exacto del original")} disabled={!!busy}/><button class="secondary" onclick={addEntity} disabled={!!busy || !addText.trim()}>{p("Añadir entidad")}</button></div>
            <button class="secondary" onclick={resetMap} disabled={!!busy || !draftDirty}>{p("Restablecer cambios")}</button>
            <button class="primary apply-map" onclick={() => applyMap()} disabled={!!busy || !draftDirty}>{p(draftDirty ? "Guardar cambios" : "Guardado")}</button>
          {:else}<p class="empty-note">{p("Analiza el documento con Qwen para detectar entidades y construir el mapa.")}</p>{/if}
          {#if draftDirty}<p class="notice">{p("Aplica los cambios al mapa para actualizar el documento.")}</p>{/if}
        </section>
    {/if}
    <div class="document-surface" class:hidden={mapOpen}>
      {#if session.layoutAvailable}
        <DocumentLayout bind:zoom {fitWidth} onExportReady={(ready, count) => { exportReady = ready; pageCount = count; }} onSessionChange={adopt} onTextDirty={layoutEdited} {session} {entities} editable={editing} dirty={draftDirty} busy={!!busy} onAliasFocus={beginAliasEdit} onAliasChange={renameAlias} onApply={() => applyMap()} />
      {:else if editing && textTranscript}
        <TranscriptView transcript={textTranscript} privateDocument privateReviewed={session.reviewed} privateId={session.id} privateRevision={session.revision} onSave={onSaveText} onExportAction={(action, exporting) => { textExport = action; textExportBusy = exporting; }} onDirtyChange={(dirty, save) => { textEditorDirty = dirty; onTextDirty(dirty, save); }} {onConfirmReview} {onValidateExport} />
      {:else}
        <div class="original-page"><pre>{session.original}</pre></div>
      {/if}
    </div>
    {#if session.warnings.length}<div class="leak-alert" role="alert"><strong>{p("Posibles filtraciones · vista previa bloqueada")}</strong>{#each session.warnings as warning}<p>{p(warning)}</p>{/each}</div>{/if}
    {#if session.analyzed && !session.analysisComplete}<div class="leak-alert" role="alert"><strong>{p("Análisis incompleto · revisión bloqueada")}</strong></div>{/if}
    {#if session.advisories?.length}<details class="review-advisories"><summary>{p("Lo que esta revisión no puede comprobar")}</summary>{#each session.advisories as advisory}<p>{p(advisory)}</p>{/each}</details>{/if}
    {#if exported}<p class="notice" role="status">{p("PDF guardado con el formato original.")} {exported}</p>{/if}
  {:else}
    <div class="welcome blue-import-screen">
      <h2 class="sr-only">{p("Lo privado permanece aquí.")}</h2>
      <button bind:this={importZone} class="welcome-import" class:dragging-document={draggingDocument} aria-label={p("Importar documento")} title={p("Importar documento")} onclick={() => importDocument()} disabled={!!busy}>
        <img class="blue-secondary import-art" src={importArt} alt="" />
        <span class="import-action"><Icon name="upload" size={26} /><span>{p(draggingDocument ? "Suelta el documento aquí" : "Seleccionar archivo")}</span></span>
      </button>
      <img class="blue-secondary protect-art" src={protectedArt} alt="" />
      <div class="blue-secondary workflow-steps" aria-label={p("Importar, revisar y exportar")}>
        <span title={p("Importar documento")}><Icon name="document" size={40} /><i><Icon name="plus" size={16} /></i></span><b></b>
        <span title={p("Revisar documento")}><Icon name="search" size={40} /></span><b></b>
        <span title={p("Exportar PDF")}><Icon name="shield" size={40} /></span>
      </div>
    </div>
  {/if}
  {#if busy}{@render processingIndicator()}{/if}
  {#if error}<p class="error" role="alert">{p(error)}</p>{/if}
</section>

<style>
  .privacy-workflow { position:relative; display:flex; flex-direction:column; flex:1; min-height:0; min-width:0; border:1px solid var(--line); border-top:0; border-radius:0 0 6px 6px; background:var(--background); padding:16px; gap:12px; }
  .workspace-header { display:flex; flex-wrap:wrap; align-items:center; gap:12px; flex:none; }
  .document-menu { position:relative; }
  .document-menu summary { cursor:pointer; padding:8px; }
  .document-menu > div { position:absolute; top:36px; right:0; z-index:20; background:var(--surface); border:1px solid var(--line); border-radius:5px; padding:10px; display:grid; gap:8px; }
  .workspace-title { flex:1; margin-right:auto; min-width:0; display:flex; flex-direction:column; gap:4px; }
  .workspace-title strong { overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
  .workspace-title span,.workspace-zoom { color:var(--muted); font-size:12px; }
  .workspace-views { display:flex; gap:4px; }
  .workspace-zoom { flex-shrink:0; display:flex; gap:6px; align-items:center; }
  .document-surface { display:flex; flex:1; min-height:0; min-width:0; }
  .hidden { display:none; }
  .original-page { flex:1; min-height:0; overflow:auto; background:var(--panel); padding:24px; }
  .original-page pre { margin:0 auto; max-width:800px; min-height:100%; padding:36px; white-space:pre-wrap; overflow-wrap:anywhere; font:16px/1.7 Georgia,serif; background:var(--surface); color:var(--text); }
  h2 { font-family:var(--font-heading); font-weight:400; margin:0; color:var(--text); }
  button { cursor:pointer; font-size:13px; border-radius:4px; padding:8px 12px; white-space:nowrap; }
  button:disabled { opacity:.45; cursor:default; }
  .primary { background:var(--button); border:1px solid var(--button); color:var(--button-text); }
  .secondary { background:var(--surface); border:1px solid var(--line-dark); color:var(--text); }
  .active-page { border-color:var(--text); background:var(--panel); }
  input,select { color:var(--text); background:var(--surface); border:1px solid var(--line); border-radius:4px; padding:7px; font:inherit; min-width:0; }
  .map-page { flex:1; min-height:0; overflow:auto; background:var(--surface); border:1px solid var(--line); border-radius:5px; padding:24px; }
  .map-heading { display:flex; align-items:center; justify-content:space-between; gap:12px; }
  .map-heading span,.map-help { font-size:12px; color:var(--muted); }
  .entity-list { display:grid; gap:12px; margin:20px 0; }
  .entity-row { display:grid; grid-template-columns:150px minmax(0,1fr) minmax(0,1fr) 32px; gap:12px; align-items:end; }
  .entity-row label { display:grid; gap:5px; font-size:12px; }
  .entity-row input { width:100%; }
  .remove { padding:7px; color:var(--text); background:transparent; border:1px solid var(--line); }
  .add-entity { display:flex; gap:10px; margin-bottom:14px; }
  .add-entity input { flex:1; }
  .welcome { display:flex; flex:1; flex-direction:column; align-items:center; justify-content:center; gap:18px; text-align:center; }
  .welcome h2 { font-size:30px; }
  .welcome-import { display:flex; flex-direction:column; align-items:center; justify-content:center; gap:12px; width:min(100%,620px); min-height:220px; padding:28px 24px; background:var(--panel); border:2px dashed var(--line-dark); border-radius:12px; color:var(--text); white-space:normal; transition:background 180ms ease,border-color 180ms ease,transform 180ms ease,box-shadow 180ms ease; }
  .welcome-import > span:first-of-type { font-size:20px; font-family:var(--font-heading); }
  .welcome-import:hover:not(:disabled),.welcome-import.dragging-document { background:var(--surface); border-color:var(--text); }
  .welcome-import:focus-visible { outline:2px solid var(--text); outline-offset:4px; }
  .welcome-import.dragging-document { transform:scale(1.015); box-shadow:0 0 0 5px var(--accent-soft); }
  .processing-orbit { width:12px; height:12px; flex:none; border:1.5px solid var(--line-dark); border-top-color:var(--text); border-radius:50%; animation:processing-orbit 1.1s linear infinite; }
  @keyframes processing-orbit { to { transform:rotate(360deg); } }
  @media (prefers-reduced-motion:reduce) {
    .welcome-import { transition:none; }
    .welcome-import.dragging-document { transform:none; }
    .processing-orbit { animation:none; }
  }
  .processing-slot { flex:none; padding:8px 0; }
  .processing-label { display:flex; align-items:center; gap:8px; justify-content:center; color:var(--muted); font-size:12px; }
  .processing-track { height:3px; margin-top:6px; overflow:hidden; background:var(--panel); }
  .processing-track span { display:block; width:35%; height:100%; background:var(--accent); animation:processing 1.8s ease-in-out infinite alternate; }
  @keyframes processing { to { transform:translateX(180%); } }
  @media(prefers-reduced-motion:reduce) { .processing-track span { animation:none; width:100%; } }
  .notice,.error,.leak-alert,.review-advisories { flex:none; font-size:12px; margin:0; }
  .error,.leak-alert { color:var(--error); }
  .leak-alert { padding:10px; border:1px solid var(--error); }
  .review-advisories { color:var(--muted); }
  .review-advisories summary { cursor:pointer; }
  @media(max-width:1000px) { .workspace-header { gap:8px; } .workspace-title { max-width:240px; } .workspace-views button { padding:8px; } }
</style>
