<script lang="ts">
  import { privacyModel } from "../lib/privacy-model";
  import BatchQueue from "./BatchQueue.svelte";
  import { BatchController } from "../lib/batch.svelte";
  import Feedback from "./Feedback.svelte";
  import ChatGPTButton from "./ChatGPTButton.svelte";
  import { importErrorMessage } from "../lib/user-errors";
  import { getLang } from "../lib/i18n-store.svelte";
  import { privateCopy as p } from "../lib/private-i18n";
  import { onMount, onDestroy, setContext } from "svelte";
  import { invoke, isTauri } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { ENTITY_KINDS, type EntityKind, type PrivateEntity, type PrivacySession } from "../lib/privacy";
  import importArt from "../assets/import-document.svg";
  import protectedArt from "../assets/protected-document.svg";
  import Icon from "./Icon.svelte";
  import SourceDocument from "./SourceDocument.svelte";
  import DocumentLayout from "./DocumentLayout.svelte";
  import TranscriptView from "./TranscriptView.svelte";
  import type { Transcript, DocFormat } from "../lib/types";
  import { pdfFileName } from "../lib/editor/export";
  import { enterView } from "../lib/motion";
  import SaveNotice from "./SaveNotice.svelte";
  let saveSequence = $state(0);
  setContext("feedbackActive", () => active);

  let { active = true, onBusyChange, onMissingModel, onSessionChange, onEdit, externalSession, comparisonRequest, textTranscript, onSaveText, onTextDirty, onConfirmReview, onValidateExport, editing = false }: { active?: boolean; onMissingModel: () => void; textTranscript: Transcript | null; onSaveText: (id: string, text: string, format?: DocFormat, html?: string | null) => Promise<void>; onTextDirty: (dirty: boolean, save: () => void | Promise<void>) => void; onConfirmReview: () => Promise<void>; onValidateExport: (text: string, title?: string) => Promise<void>; editing?: boolean; onBusyChange: (busy: boolean) => void; onSessionChange: (session: PrivacySession | null) => void; onEdit: (session: PrivacySession) => void; externalSession: PrivacySession | null; comparisonRequest: number } = $props();
  let session = $state<PrivacySession | null>(null);

  let entities = $state<PrivateEntity[]>([]);
  let draftDirty = $state(false);
  let layoutDirty = $state(false);
  function layoutEdited(dirty: boolean, save: () => void | Promise<void>) { layoutDirty = dirty; onTextDirty(dirty, save); }
  let aliasEdit: { index: number; members: number[] } | null = null;
  let busy = $state("");
  let elapsedSeconds = $state(0);
  const protecting = $derived(busy.startsWith("Analizando"));
  $effect(() => {
    elapsedSeconds = 0;
    if (!protecting) return;
    const started = performance.now();
    const timer = setInterval(() => { elapsedSeconds = Math.floor((performance.now() - started) / 1000); }, 250);
    return () => clearInterval(timer);
  });
  let processingDialog = $state<HTMLDialogElement>();
  const modalProcessing = $derived(busy.startsWith("Importando") || busy.startsWith("Analizando"));
  $effect(() => {
    if (!processingDialog) return;
    if (modalProcessing && active && !processingDialog.open) processingDialog.showModal();
    else if ((!modalProcessing || !active) && processingDialog.open) processingDialog.close();
  });
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
    if (analysisStep === "model_verify") return p("Comprobando integridad del modelo local…");
    if (analysisStep === "model_load") return p("Cargando modelo en memoria…");
    const match = /^(reading|ocr|prefill|ner):(\d+)\/(\d+)(?::(\d+))?$/.exec(analysisStep);
    if (!match) return "";
    const [, kind, done, total, tokens] = match;
    if (kind === "ner") return p("Detectando entidades · fragmento {0} de {1}").replace("{0}", done).replace("{1}", total).replace("{2}", tokens ?? "0");
    if (kind === "reading" && (!tokens || tokens === "0")) return p("Preparando entrada · fragmento {0} de {1}").replace("{0}", done).replace("{1}", total).replace("{2}", tokens ?? "0");
    if (kind === "prefill") return p("Preparando fragmento {0} de {1} · {2} tokens de entrada").replace("{0}", done).replace("{1}", total).replace("{2}", tokens ?? "0");
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
  let mobile = $state(false);
  onMount(() => { void invoke<string>("app_platform").then(p => mobile = p === "android").catch(() => {}); });
  let compareOriginal = $state(false);
  let sourceEditing = $state(false);
  let sourceDraft = $state("");
  let showExtracted = $state(false);
  let showOriginal = $state(false);
  const hasMobileSource = $derived(mobile && !!session && /\.(pdf|png|jpe?g|tiff?|heic)$/i.test(session.fileName));
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
  const batch = new BatchController(next => { adopt(next); if(next?.analyzed) onEdit(next); },
    () => draftDirty || layoutDirty || textEditorDirty || sourceEditing || !!busy,
    () => onMissingModel(), () => session);
  const batchDirty = $derived(draftDirty || layoutDirty || textEditorDirty || sourceEditing || !!busy);
  function closeDocumentMenu(event:Event) { (event.currentTarget as HTMLElement).closest("details")?.removeAttribute("open"); }

  $effect(() => {
    void comparisonRequest;
    void editing;
    mapOpen = false;
  });
  function adopt(next: PrivacySession | null) {
    if (next?.id !== session?.id) { zoom = 100; fitWidth = true; exported = ""; exportReady = false; }
    if (!next?.analyzed || next.id !== session?.id) mapOpen = false;
    if (next) localStorage.setItem("bluedoor-session-in-memory", "true"); else localStorage.removeItem("bluedoor-session-in-memory");
    sourceDraft = next?.original ?? ""; sourceEditing = false; showExtracted = false; showOriginal = false;
    session = next;
    onSessionChange(next);
    entities = next?.entities.map(e => ({ ...e })) ?? [];
    draftDirty = false;
    if (!layoutDirty && !textEditorDirty) onTextDirty(false, () => { void applyMap(); });
    aliasEdit = null;
    if(batch.view && !batch.locked) void batch.refresh();
  }
  function edited() { draftDirty = true; error = ""; onTextDirty(true, () => { void applyMap(); }); }
  function resetMap() {
    entities = session?.entities.map(entity => ({ ...entity })) ?? [];
    draftDirty = false; aliasEdit = null; error = "";
    onTextDirty(false, () => { void applyMap(); });
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
    await invoke<boolean>("interrupted_private_session").catch(() => false);
    localStorage.removeItem("bluedoor-interrupted-operation");
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
      await batch.restore();

      if (restored?.analyzed) onEdit(restored);
    } catch { error = "Abre el demo con Tauri para importar documentos y ejecutar Qwen localmente."; }
  });
  onDestroy(() => { disposed = true; unlistenAnalysis?.(); unlistenDrop?.(); });

  async function run(label: string, action: () => Promise<void>): Promise<boolean> {
    if (busy) return false;
    localStorage.setItem("bluedoor-interrupted-operation", "true");
    busy = label; error = ""; onBusyChange(true);
    try { await action(); return true; }
    catch (e) {
      error = label.startsWith("Importando") ? importErrorMessage(e, "document", getLang()) : String(e);
      if (label.startsWith("Analizando")) {
        try { adopt(await invoke<PrivacySession | null>("get_privacy_session")); } catch { }
      }
      return false;
    }
    finally { localStorage.removeItem("bluedoor-interrupted-operation"); busy = ""; sanitizing = false; analysisId = null; analysisRevision = null; onBusyChange(false); }
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
      const modelId = privacyModel(mobile);
      const all = await invoke<{ privacy: { id: string; installed: boolean }[] }>("list_models");
      if (!all.privacy.some(model => model.id === modelId && model.installed)) {
        onMissingModel();
        return;
      }
      const analyzed = await invoke<PrivacySession>("analyze_private_document", { id: current.id, revision: current.revision, modelId });
      adopt(analyzed);
      if (analyzed.analyzed) onEdit(analyzed);
    });
  }
  function addEntity() {
    if (!addText.trim()) return;
    const used = new Set(entities.map(e => e.alias));
    let n = 1;
    while (used.has(`${addKind}_${String(n).padStart(3, "0")}`)) n++;
    entities = [...entities, { kind: addKind, original: addText.trim(), alias: `${addKind}_${String(n).padStart(3, "0")}`, source: "review" }];
    addText = ""; edited();
  }
  async function applyMap(confirm = false) {
    if (!session) return false;
    const current = session;
    if(batch.view?.started) {
      const next=await batch.changeAlias({type:"map",entities:$state.snapshot(entities)},false);
      if(!next)return false;
      adopt(next);saveSequence++;return true;
    }
    const applied = await run("Validando el mapa local…", async () => {
      adopt(await invoke<PrivacySession>(current.layoutAvailable ? "review_private_layout" : "review_private_document", { id: current.id, revision: current.revision, entities: $state.snapshot(entities), confirm }));
      saveSequence++;
    });
    return applied;
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
  $effect(() => { onBusyChange(!!busy || batch.locked || layoutDirty || draftDirty || textEditorDirty || sourceEditing); });
  async function exportOriginalLayout() {
    if (!session || draftDirty) return;
    await run("Exportando PDF con formato original…", async () => {
      const reviewed = await invoke<PrivacySession>("review_private_layout", { id: session!.id, revision: session!.revision, entities: $state.snapshot(entities), confirm: true });
      adopt(reviewed);
      if (!reviewed.reviewed) throw new Error(p("Revisa el mapa privado antes de exportar."));
      const destPath = await invoke<string | null>(mobile ? "prepare_share_export" : "choose_pdf_destination", { defaultName: pdfFileName("Protected") });
      if (!destPath) return;
      exported = await invoke<string>("export_private_layout", { id: reviewed.id, revision: reviewed.revision, destPath });
      if (mobile) await invoke("share_export", { path: exported });
    });
  }
</script>

{#snippet approveReview()}
  {#if session}
        {#if !session.reviewed}<p class="review-instruction">{p(batch.view?.started ? "Antes de exportar, aprueba todos los documentos." : "Antes de exportar aprueba la revisión:")}</p>{/if}
        <button class="secondary approve-review" disabled={(!!busy || batch.locked) || draftDirty || layoutDirty || !session.analysisComplete || !!session.warnings.length || session.reviewed} onclick={async () => {
          if (!session) return;
          if (textEditorDirty) { error = "Guarda la edición antes de aprobar."; return; }
          await run("Confirmando revisión…", async () => { await onConfirmReview(); adopt(await invoke<PrivacySession | null>("get_privacy_session")); });
        }} aria-label={p(session.reviewed ? "Revisión aprobada" : "Aprobar revisión")} title={p(session.reviewed ? "Revisión aprobada" : "Aprobar revisión")}><Icon name="clipboard-check" size={20} /><span>{p(session.reviewed ? "Revisión aprobada" : "Aprobar revisión")}</span></button>
  {/if}
{/snippet}

{#snippet batchMenu()}
  {#if !batch.view}
    <button class="secondary" disabled={!!busy || batch.locked || batchDirty} onclick={event=>{closeDocumentMenu(event);void batch.choose();}} title={p("Detectar varios archivos…")}><Icon name="folder" size={18}/><span>{p("Detectar varios archivos…")}</span></button>
  {:else}
    <button class="secondary" disabled={batch.locked || batchDirty} onclick={event=>{closeDocumentMenu(event);batch.mode=batch.view?.started?"list":"prepare";}}><Icon name="list" size={18}/><span>{p(batch.view.started?"Documentos del lote":"Preparar lote")}</span></button>
    <button class="secondary" disabled={batch.locked || batchDirty || !batch.approved} onclick={event=>{closeDocumentMenu(event);batch.mode="export";}}><Icon name="folder" size={18}/><span>{p("Exportar aprobados a…")}</span></button>
    <button class="secondary" disabled={batch.locked || batchDirty} onclick={event=>{closeDocumentMenu(event);batch.mode="close";}}><Icon name="close" size={18}/><span>{p("Cerrar lote")}</span></button>
  {/if}
{/snippet}

{#snippet editorZoom()}
  <label class="workspace-zoom" title={p("Zoom")}><span class="sr-only">{p("Zoom")}</span><span class="zoom-value" aria-hidden="true"><Icon name="search" size={18} /><span>{fitWidth ? p("Ajustar al ancho") : `${Math.round(zoom)}%`}</span></span><span class="zoom-chevron" aria-hidden="true"><Icon name="chevron" size={16} /></span><select value={fitWidth ? "fit" : zoom} onchange={chooseZoom} disabled={(!!busy || batch.locked) || mapOpen}><option value="fit">{p("Ajustar al ancho")}</option>{#if !fitWidth && !ZOOM_LEVELS.includes(zoom)}<option value={zoom}>{Math.round(zoom)}%</option>{/if}{#each ZOOM_LEVELS as level}<option value={level}>{level}%</option>{/each}</select></label>
{/snippet}

{#snippet processingIndicator()}
  <div class="processing-slot">
    <div class="processing-label" role="status" aria-live="polite">
      <span class="processing-orbit" aria-hidden="true"></span><span class="processing-copy" title={stepLabel || processingLabel}>{stepLabel || processingLabel}</span>
    </div>
    {#if mobile && busy.startsWith("Importando")}<button class="secondary" onclick={() => invoke("cancel_private_import")}>{p("Cancelar extracción")}</button>{/if}
    {#if protecting}<div class="processing-actions"><button class="secondary" onclick={() => invoke("cancel_private_analysis")}>{p("Cancelar análisis")}</button><span class="processing-elapsed" aria-label={p("Tiempo transcurrido en segundos")}>{String(elapsedSeconds).padStart(3, "0")} s</span></div>{/if}
    <div class="processing-track processing" role="progressbar" aria-label={p("Procesando documento")} aria-busy="true"><span></span></div>
  </div>
{/snippet}

<section class="privacy-workflow" aria-label="Private Legal" use:enterView={`${session?.id ?? "welcome"}:${editing}:${mapOpen}`}>
  <SaveNotice sequence={saveSequence} />
  <BatchQueue controller={batch} modelId={privacyModel(mobile)} dirty={batchDirty}/>

  {#if session}
    {#if hasMobileSource && !session.layoutAvailable && !editing && (!session.analyzed || showOriginal)}<div class="mobile-source"><button disabled={(!!busy || batch.locked)} class="secondary" onclick={async () => { try { await invoke("preview_import_source"); } catch (e) { error = String(e); } }}>Ampliar original</button><button class="secondary" disabled={(!!busy || batch.locked)} onclick={() => showExtracted = !showExtracted}>{showExtracted ? "Ver PDF / imagen" : "Ver texto extraído"}</button></div>{/if}
    <div class="workspace-header">
      <div class="workspace-identity">
        <details class="document-menu"><summary aria-label={p("Acciones del documento")}>···</summary><div>
          {#if session.analyzed}<button class="secondary" onclick={event => { event.currentTarget.closest("details")!.open=false; clearResult(); }} disabled={(!!busy || batch.locked) || !!batch.view}>{p("Volver a analizar")}</button>{/if}
          <button class="secondary" onclick={event => { event.currentTarget.closest("details")!.open=false; clear(); }} disabled={(!!busy || batch.locked) || !!batch.view}>{p("Borrar documento y empezar de cero")}</button>
          {@render batchMenu()}
        </div></details>
      <div class="workspace-title"><strong title={session.fileName}>{session.fileName}</strong><span>{p(session.analyzed ? (session.reviewed ? "Revisión confirmada" : "Pendiente de revisión") : "Documento original")}{#if pageCount} · {pageCount} {p(pageCount === 1 ? "página" : "páginas")}{/if}</span></div>
      </div>
      {#if editing && session.analyzed}
        <div class="edit-action-grid">
        <div class="workspace-views" role="group" aria-label={p("Vistas de edición")}>
          <button class="secondary" disabled={(!!busy || batch.locked) || layoutDirty || textEditorDirty || (mapOpen && draftDirty)} onclick={() => mapOpen = !mapOpen} aria-label={p(mapOpen ? "Volver al documento" : "Mapa privado")} title={p(mapOpen ? "Volver al documento" : "Mapa privado")}><Icon name={mapOpen ? "document" : "map"} /><span>{p(mapOpen ? "Volver al documento" : "Mapa privado")}</span></button>
        </div>
        <div class="review-actions" role="group" aria-label={p("Revisión y exportación")}>

        <button class="primary" onclick={() => session?.layoutAvailable ? exportOriginalLayout() : textExport?.()} disabled={(!!busy || batch.locked) || textExportBusy || ((mobile || batch.view?.started) && (!session.reviewed || textEditorDirty)) || (session.layoutAvailable && !exportReady) || (!session.layoutAvailable && !textExport) || draftDirty || layoutDirty || !session.analysisComplete || !!session.warnings.length} aria-label={p("Exportar PDF")} title={p("Exportar PDF")}><Icon name="file-down" size={20} /><span>{p("Exportar PDF")}</span></button>
        <ChatGPTButton disabled={(!!busy || batch.locked) || textExportBusy} />
        </div>
        </div>
      {:else}
        <div class="document-actions">

        <button class="secondary" onclick={() => importDocument()} disabled={(!!busy || batch.locked) || !!batch.view} aria-label={p("Importar otro documento")} title={p("Importar otro documento")}><Icon name="file-plus" size={20} /><span>{p(mobile ? "Importar otro" : "Importar otro documento")}</span></button>

        {#if session.analyzed}<button class="primary" onclick={() => { if (session) onEdit(session); }} disabled={(!!busy || batch.locked)} aria-label={p(mobile ? "Editar" : "Revisar y editar")} title={p(mobile ? "Editar" : "Revisar y editar")}><Icon name="pencil" size={20} /><span>{p(mobile ? "Editar" : "Revisar y editar")}</span></button>
        {:else}<button class="primary analyze-button" onclick={analyze} disabled={(!!busy || batch.locked) || sourceEditing} aria-label={p("Proteger documento")} title={p("Proteger documento")}><Icon name="shield" size={20} /><span>{p("Proteger documento")}</span></button>{/if}
        </div>
      {/if}
    </div>
    {#if session.analyzed && ((editing && mobile) || (batch.view?.started && !batch.running))}
      {@render approveReview()}
    {/if}
    {#if mapOpen && editing}
        <section class="map-page" aria-label={p("Mapa privado")}>
          <div class="map-heading"><h2>{p("Mapa privado")}</h2><span>{entities.length} {p("entradas · solo local")}</span></div>
          {#if batch.view?.started}
            <button class="secondary batch-alias-entry" disabled={batch.locked||batchDirty} onclick={()=>batch.openAliases()} title={p("Alias del lote")}><Icon name="map" size={20}/><span>{p("Alias del lote")}</span></button>
            <p class="batch-alias-hint">{p("Las coincidencias exactas comparten alias. Las variantes se unen solo con tu confirmación.")}</p>
          {/if}
          {#if session.analyzed}
            <p class="map-help">{p("Corrige detecciones y aliases. Cambiar un alias actualiza todas las entradas que lo comparten. Para unir variantes usa el mismo alias. Eliminar una entrada restaura ese dato en la salida: revisa con cuidado.")}</p>
            <div class="entity-list">
              {#each entities as entity, i}
                <div class="entity-row">
                  <label><span>{p("Categoría")}</span><span class="map-select"><select bind:value={entity.kind} onchange={edited} disabled={(!!busy || batch.locked)}>{#each ENTITY_KINDS as kind}<option value={kind.value}>{p(kind.label)}</option>{/each}</select><span class="map-select-chevron" aria-hidden="true"><Icon name="chevron" size={16} /></span></span></label>
                  <label><span>{p("Valor privado")}</span><input aria-label={`${p("Valor privado")} ${i+1}`} bind:value={entity.original} oninput={edited} disabled={(!!busy || batch.locked)}/></label>
                  <label><span>{p("Alias")}</span><input aria-label={`Alias ${i+1}`} value={entity.alias} onfocus={() => beginAliasEdit(i)} oninput={event => renameAlias(i, event.currentTarget.value)} disabled={(!!busy || batch.locked)} spellcheck="false"/></label>
                  <details class="entity-context"><summary>Ver contexto</summary><p>{session.original.slice(Math.max(0, session.original.indexOf(entity.original) - 90), session.original.indexOf(entity.original) + entity.original.length + 90)}</p></details>
                  <button class="remove" aria-label={`${p("Eliminar entidad")} ${i+1}`} disabled={(!!busy || batch.locked)} onclick={() => { entities = entities.filter((_, n) => n !== i); aliasEdit = null; edited(); }}>×</button>
                </div>
              {/each}
            </div>
            <div class="add-entity"><span class="map-select"><select aria-label={p("Tipo de nueva entidad")} bind:value={addKind} disabled={(!!busy || batch.locked)}>{#each ENTITY_KINDS as kind}<option value={kind.value}>{p(kind.label)}</option>{/each}</select><span class="map-select-chevron" aria-hidden="true"><Icon name="chevron" size={16} /></span></span><input aria-label={p("Nueva entidad privada")} bind:value={addText} placeholder={p("Texto exacto del original")} disabled={(!!busy || batch.locked)}/><button class="secondary" onclick={addEntity} disabled={(!!busy || batch.locked) || !addText.trim()}>{p("Añadir entidad")}</button></div>
            <div class="map-actions">
            <button class="secondary compact-action" onclick={resetMap} disabled={(!!busy || batch.locked) || !draftDirty} aria-label={p("Restablecer cambios")} title={p("Restablecer cambios")}><Icon name="history" /><span class="compact-action-label">{p("Restablecer cambios")}</span></button>
            <button class="primary apply-map" onclick={() => applyMap()} disabled={(!!busy || batch.locked) || !draftDirty}><Icon name="check" /><span>{p(draftDirty ? "Guardar cambios" : "Guardado")}</span></button>
            </div>
          {:else}<p class="empty-note">{p("Analiza el documento con Qwen para detectar entidades y construir el mapa.")}</p>{/if}
          <Feedback message={draftDirty ? p("Aplica los cambios al mapa para actualizar el documento.") : ""} kind="status" />
        </section>
    {/if}
    {#if editing && !session.layoutAvailable}<button class="secondary" onclick={() => compareOriginal = !compareOriginal} aria-label={p(compareOriginal ? "Ver copia protegida" : "Ver original extraído")} title={p(compareOriginal ? "Ver copia protegida" : "Ver original extraído")}><Icon name={compareOriginal ? "shield" : "eye"} size={20} /><span>{p(compareOriginal ? "Ver copia protegida" : "Ver original extraído")}</span></button>{/if}
    <div class="document-surface" class:hidden={mapOpen || batch.running}>
    {#if (session.layoutAvailable || (hasMobileSource && !showExtracted)) && !mapOpen}
      <div class="workspace-zoom-row">{@render editorZoom()}</div>
    {/if}
      {#if compareOriginal && editing && !session.layoutAvailable}<div class="original-page"><pre>{session.original}</pre></div>
      {:else}
      {#if mobile && !session.layoutAvailable && session.analyzed && !editing && !showOriginal}
        <div class="original-page protected-preview" aria-label="Copia protegida pendiente de revisión">{#if !session.analysisComplete || session.warnings.length || !session.entities.length}<strong>{!session.analysisComplete || session.warnings.length ? "Análisis incompleto o con advertencias · revisión bloqueada" : "Sin sustituciones · documento no protegido"}</strong>{/if}<pre>{session.entities.length && session.analysisComplete && !session.warnings.length ? session.sanitized : session.original}</pre></div>
      {:else if hasMobileSource && !editing && (!session.layoutAvailable || showOriginal) && (!session.analyzed || showOriginal) && !showExtracted}
        {#key session.id}<SourceDocument documentId={session.id} bind:zoom bind:fitWidth/>{/key}
      {:else if session.layoutAvailable}
        {#key session.id}<DocumentLayout bind:zoom bind:fitWidth onExportReady={(ready, count) => { exportReady = ready; pageCount = count; }} onSessionChange={adopt} onTextDirty={layoutEdited} {session} {entities} editable={editing} dirty={draftDirty} busy={(!!busy || batch.locked)} onAliasFocus={beginAliasEdit} onAliasChange={renameAlias} onApply={() => applyMap()} />{/key}
      {:else if editing && textTranscript}
        {#key session.id}<TranscriptView transcript={textTranscript} privateDocument privateReviewed={session.reviewed} privateId={session.id} privateRevision={session.revision} onSave={onSaveText} onExportAction={(action, exporting) => { textExport = action; textExportBusy = exporting; }} onDirtyChange={(dirty, save) => { textEditorDirty = dirty; onTextDirty(dirty, save); }} {onConfirmReview} {onValidateExport} />{/key}
      {:else}
        <div class="original-page">
          {#if mobile}<button class="secondary" onclick={() => sourceEditing = !sourceEditing} disabled={(!!busy || batch.locked)}>{sourceEditing ? "Cancelar corrección" : "Corregir texto extraído"}</button>{/if}
          {#if sourceEditing}<textarea aria-label="Corrección del texto extraído" bind:value={sourceDraft} rows="14"></textarea>
            <button class="primary" disabled={(!!busy || batch.locked) || !sourceDraft.trim()} onclick={async () => { if (!session) return; const current = session; await run("Guardando extracción corregida…", async () => { adopt(await invoke<PrivacySession>("update_private_source", { id: current.id, revision: current.revision, text: sourceDraft })); }); }}>Guardar corrección</button>
          {:else}<pre>{session.original}</pre>{/if}
        </div>
      {/if}
      {/if}
    </div>
    {#if mobile && session.analyzed && !editing}
      <button class="secondary original-toggle" disabled={(!!busy || batch.locked)} onclick={() => showOriginal = !showOriginal} aria-label={p(showOriginal ? "Ver copia protegida" : "Ver original")} title={p(showOriginal ? "Ver copia protegida" : "Ver original")}><Icon name={showOriginal ? "shield" : "eye"} size={20} /><span>{p(showOriginal ? "Ver copia protegida" : "Ver original")}</span></button>
    {/if}
    <Feedback className="leak-alert" message={session.warnings.length ? [p("Posibles filtraciones · vista previa bloqueada"), ...session.warnings.map(p)].join("\n") : session.analyzed && !session.analysisComplete ? p("Análisis incompleto · revisión bloqueada") : ""} />
    <Feedback message={exported ? `${p("PDF guardado con el formato original.")} ${exported}` : ""} kind="status" />
  {:else}

    {#if !batch.running}
    <div class="welcome blue-import-screen">
      <h2 class="sr-only">{p("Lo privado permanece aquí.")}</h2>
      <button bind:this={importZone} class="welcome-import" class:dragging-document={draggingDocument} aria-label={p("Importar documento")} title={p("Importar documento")} onclick={() => importDocument()} disabled={(!!busy || batch.locked) || !!batch.view}>
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
    <div class="batch-entry-menu"><details class="document-menu"><summary aria-label={p("Acciones del documento")} title={p("Acciones del documento")}>···</summary><div>{@render batchMenu()}</div></details></div>
    {/if}
  {/if}
  <dialog class="processing-dialog" bind:this={processingDialog} aria-label={p("Procesando documento")} oncancel={event => event.preventDefault()}>
    {#if modalProcessing}{@render processingIndicator()}{/if}
  </dialog>
  {#if busy && !modalProcessing}{@render processingIndicator()}{/if}
  <Feedback message={error ? p(error) : ""} onDismiss={() => error = ""} />
</section>

<style>
  .privacy-workflow { position:relative; display:flex; flex-direction:column; flex:1; min-height:0; min-width:0; border:1px solid var(--line); border-top:0; border-radius:0 0 6px 6px; background:var(--background); padding:16px; gap:12px; }
  .workspace-header { display:flex; flex-wrap:wrap; align-items:center; gap:12px; flex:none; }
  .review-instruction { margin:0; color:var(--text); font-size:14px; line-height:1.5; }
  .workspace-identity { display:flex; align-items:flex-start; gap:12px; flex:1 1 100%; min-width:0; }
  .document-actions { display:flex; flex-wrap:wrap; align-items:center; gap:12px; flex-basis:100%; min-width:0; }
  .document-actions > button { flex:1; min-width:0; }
  .review-actions { display:contents; }
  .batch-entry-menu { display:flex; justify-content:center; flex:none; width:100%; padding:0 0 12px; }
  .batch-entry-menu summary { display:flex; align-items:center; justify-content:center; box-sizing:border-box; width:48px; height:48px; padding:0; font-size:22px; border-radius:9px; }
  .batch-entry-menu summary:hover { background:var(--panel); }
  .batch-entry-menu .document-menu > div { top:auto; bottom:56px; left:50%; transform:translateX(-50%); }
  .document-menu { position:relative; }
  .document-menu summary { cursor:pointer; padding:8px; list-style:none; min-width:28px; text-align:center; }
  .document-menu summary::-webkit-details-marker { display:none; }
  .document-menu > div { position:absolute; top:40px; left:0; max-width:calc(100vw - 48px); z-index:20; background:var(--surface); border:1px solid var(--line); border-radius:5px; padding:10px; display:grid; gap:8px; }
  .workspace-title { flex:1; margin-right:auto; min-width:0; display:flex; flex-direction:column; gap:4px; }
  .workspace-title strong { overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
  .workspace-title span,.workspace-zoom { color:var(--muted); font-size:12px; }
  .workspace-views { display:flex; flex:1 0 100%; width:100%; }
  .workspace-views button { display:inline-flex; flex:1; width:100%; align-items:center; justify-content:center; gap:8px; min-height:40px; }
  .workspace-zoom-row { flex:none; width:100%; padding:0 0 12px; }
  .workspace-zoom-row .workspace-zoom { position:relative; width:100%; height:48px; justify-content:center; box-sizing:border-box; border:1px solid var(--line); border-radius:9px; }
  .zoom-chevron { position:absolute; right:16px; display:flex; align-items:center; justify-content:center; pointer-events:none; transform:rotate(90deg); }
  .zoom-value { display:inline-flex; align-items:center; justify-content:center; gap:8px; line-height:1; pointer-events:none; }
  .zoom-value :global(svg) { flex:none; }
  .workspace-zoom:focus-within { outline:2px solid var(--text); outline-offset:2px; }
  .workspace-zoom-row select { position:absolute; inset:0; width:100%; height:100%; padding:0; opacity:0; cursor:pointer; border:0; border-radius:9px; }
  .workspace-zoom { flex-shrink:0; display:flex; gap:6px; align-items:center; }
  .document-surface { display:flex; flex-direction:column; flex:1; min-height:0; min-width:0; }
  .hidden { display:none; }
  .original-page { flex:1; min-height:0; overflow:auto; background:var(--panel); padding:24px; }
  .original-page pre { margin:0 auto; max-width:800px; min-height:100%; padding:36px; white-space:pre-wrap; overflow-wrap:anywhere; font:16px/1.7 Georgia,serif; background:var(--surface); color:var(--text); }
  h2 { font-family:var(--font-heading); font-weight:400; margin:0; color:var(--text); }
  button { cursor:pointer; font-size:13px; border-radius:4px; padding:8px 12px; white-space:nowrap; }
  button:disabled { opacity:.45; cursor:default; }
  button:has(> :global(svg)) { display:inline-flex; align-items:center; justify-content:center; gap:8px; }
  button > :global(svg) { flex-shrink:0; }
  .primary { background:var(--button); border:1px solid var(--button); color:var(--button-text); }
  .secondary { background:var(--surface); border:1px solid var(--line-dark); color:var(--text); }
  input,select { color:var(--text); background:var(--surface); border:1px solid var(--line); border-radius:4px; padding:7px; font:inherit; min-width:0; }
  .batch-alias-entry { display:flex; align-items:center; justify-content:center; gap:8px; width:100%; min-height:48px; margin-top:12px; }
  .batch-alias-hint { font-size:12px; color:var(--muted); line-height:1.6; margin:8px 0 16px; }
  .map-page { flex:1; min-height:0; overflow:auto; background:var(--surface); border:1px solid var(--line); border-radius:5px; padding:24px; }
  .map-heading { display:flex; align-items:center; justify-content:space-between; gap:12px; }
  .map-heading span,.map-help { font-size:12px; color:var(--muted); }
  .entity-list { display:grid; gap:12px; margin:20px 0; }
  .map-select { position:relative; display:flex; align-items:center; min-width:0; }
  .map-select-chevron { position:absolute; right:16px; display:flex; align-items:center; pointer-events:none; transform:rotate(90deg); }
  .entity-row { display:grid; grid-template-columns:150px minmax(0,1fr) minmax(0,1fr) 32px; gap:12px; align-items:end; }
  .entity-row label { display:grid; gap:5px; font-size:12px; }
  .entity-row input { width:100%; }
  .entity-context { grid-column:1 / -2; min-width:0; }
  .entity-row .remove { grid-column:-2 / -1; justify-self:end; }
  .map-actions { display:grid; grid-template-columns:repeat(2,minmax(0,1fr)); align-items:stretch; gap:12px; }
  .map-actions button { display:inline-flex; align-items:center; justify-content:center; gap:8px; }
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
  .processing-dialog { width:min(460px,calc(100vw - 48px)); box-sizing:border-box; padding:28px; border:1px solid var(--modal-border); border-radius:18px; background:var(--modal-surface); color:white; box-shadow:var(--modal-shadow); }
  .processing-dialog::backdrop { background:var(--modal-backdrop); backdrop-filter:blur(var(--modal-blur)); }
  .processing-dialog .processing-label { color:white; font-size:14px; }
  .processing-dialog .processing-copy { white-space:normal; overflow-wrap:anywhere; }
  .processing-dialog .processing-orbit { border-color:var(--modal-border); border-top-color:white; }
  .processing-dialog .processing-track { background:var(--modal-border); }
  .processing-dialog .processing-track span { background:white; }
  .processing-actions { display:flex; align-items:center; gap:16px; min-width:0; }
  .processing-elapsed { margin-left:auto; text-align:right; font-size:11px; font-variant-numeric:tabular-nums; color:#c5dbff; white-space:nowrap; }
  .processing-slot button { pointer-events:auto; }
  .processing-slot { position:absolute; bottom:8px; left:36px; right:36px; height:40px; padding:8px 0; pointer-events:none; }
  .processing-copy { overflow:hidden; text-overflow:ellipsis; white-space:nowrap; min-width:0; }
  .processing-label { display:flex; align-items:center; gap:8px; justify-content:center; color:var(--muted); font-size:12px; }
  .processing-track { height:3px; margin-top:6px; overflow:hidden; background:var(--panel); }
  .processing-track span { display:block; width:35%; height:100%; background:var(--accent); animation:processing 1.8s ease-in-out infinite alternate; }
  @keyframes processing { to { transform:translateX(180%); } }
  @media(prefers-reduced-motion:reduce) { .processing-track span { animation:none; width:100%; } }
  @media(max-width:1000px) { .workspace-header { gap:8px; } .workspace-title { max-width:240px; } .workspace-views button { padding:8px; } }
</style>
