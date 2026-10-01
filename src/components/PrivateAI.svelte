<script lang="ts">
  import { privateCopy as p } from "../lib/private-i18n";
  import { onMount, onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { ENTITY_KINDS, type EntityKind, type PrivateEntity, type PrivacySession } from "../lib/privacy";
  import belegalMark from "../assets/belegal-mark.png";

  let { onBusyChange, onSessionChange, onEdit, externalSession, comparisonRequest }: { onBusyChange: (busy: boolean) => void; onSessionChange: (session: PrivacySession | null) => void; onEdit: (session: PrivacySession) => void; externalSession: PrivacySession | null; comparisonRequest: number } = $props();
  let session = $state<PrivacySession | null>(null);
  let entities = $state<PrivateEntity[]>([]);
  let draftDirty = $state(false);
  let busy = $state("");
  let sanitizing = $state(false);
  let analysisId: string | null = null;
  let analysisRevision: number | null = null;
  let unlistenAnalysis: (() => void) | undefined;
  let disposed = false;
  const processingLabel = $derived(busy.startsWith("Analizando")
    ? p(sanitizing ? "Sanitizando el documento" : "Analizando con AI local")
    : p(busy).replace(/(?:…|\.{3})$/, ""));
  let error = $state("");
  let pasteOpen = $state(false);
  let pasteText = $state("");
  let addText = $state("");
  let addKind = $state<EntityKind>("PERSON");
  let mapOpen = $state(false);
  let originalExpanded = $state(false);
  $effect(() => {
    void comparisonRequest;
    originalExpanded = false;
    mapOpen = false;
    pasteOpen = false;
  });
  function adopt(next: PrivacySession | null) {
    if (next?.id !== session?.id) originalExpanded = false;
    if (!next?.analyzed || next.id !== session?.id) mapOpen = false;
    session = next;
    onSessionChange(next);
    entities = next?.entities.map(e => ({ ...e })) ?? [];
    draftDirty = false;
  }
  function edited() { draftDirty = true; error = ""; }
  $effect(() => { if (externalSession && (session?.id !== externalSession.id || session.revision !== externalSession.revision)) adopt(externalSession); });
  onMount(async () => {
    const stop = await listen<[string, number, string]>("private-analysis-progress", event => {
      const [id, revision, phase] = event.payload;
      if (busy.startsWith("Analizando") && id === analysisId && revision === analysisRevision && phase === "sanitizing") sanitizing = true;
    });
    if (disposed) { stop(); return; }
    unlistenAnalysis = stop;
    try {
      adopt(await invoke<PrivacySession | null>("get_privacy_session"));
    } catch { error = "Abre el demo con Tauri para importar documentos y ejecutar Qwen localmente."; }
  });
  onDestroy(() => { disposed = true; unlistenAnalysis?.(); });

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
  async function importDocument() {
    await run("Importando documento local…", async () => {
      const next = await invoke<PrivacySession | null>("import_private_document");
      if (next) { adopt(next); pasteOpen = false; pasteText = ""; }
    });
  }
  async function pasteDocument() {
    await run("Preparando texto local…", async () => {
      adopt(await invoke<PrivacySession>("paste_private_document", { text: pasteText }));
      pasteOpen = false; pasteText = "";
    });
  }
  async function analyze() {
    if (!session || busy) return;
    const current = session;
    analysisId = current.id;
    analysisRevision = current.revision;
    sanitizing = false;
    await run("Analizando documento local…", async () => {
      const modelId = localStorage.getItem("belegal-privacy-model") || "qwen3-fast";
      adopt(await invoke<PrivacySession>("analyze_private_document", { id: current.id, revision: current.revision, modelId }));
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
      adopt(await invoke<PrivacySession>("review_private_document", { id: current.id, revision: current.revision, entities: $state.snapshot(entities), confirm }));
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
    await run("Cerrando documento…", async () => { await invoke("clear_privacy_session"); adopt(null); pasteText = ""; });
  }
</script>

<section class="privacy-workflow" aria-label="Private Legal">
  <div class="intro">
    <div><p class="eyebrow">{p("PRIVATE LEGAL · MOTOR LOCAL")}</p><h1>{#each p("Lo privado permanece aquí.").split(/\b(privado|private)\b/) as part}{#if part === "privado" || part === "private"}<em>{part}</em>{:else}{part}{/if}{/each}</h1></div>
    <div class="intro-status">
      <div class="status-slot">
        {#if error}<p class="error" role="alert">{p(error)}</p>{/if}
      </div>
    </div>
  </div>

    <div class="document-actions">
      <button class="primary" onclick={importDocument} disabled={!!busy}>{p("Importar documento")}</button>
      <button class="secondary" onclick={() => pasteOpen = !pasteOpen} disabled={!!busy}>{p("Pegar texto")}</button>
      <button class="secondary analyze-button" onclick={analyze} disabled={!!busy || !session}>{session?.analyzed ? p("Volver a analizar") : p("Analizar documento")}</button>
      <button class="secondary" class:active-page={mapOpen} onclick={() => { mapOpen = true; pasteOpen = false; }} disabled={!!busy || !session?.analyzed || !session.entities.length} aria-pressed={mapOpen}>{p("Mapa privado")}</button>
      <button class="secondary" onclick={() => { if (session) onEdit(session); }} disabled={!!busy || !session?.analyzed || draftDirty}>{p("Abrir en Edit")}</button>
      <div class="processing-slot">
        <div class="processing-label" role={busy ? "status" : undefined} aria-live="polite">
          {#if busy}<span class="processing-copy" title={processingLabel}>{processingLabel}</span><span class="processing-dots" aria-hidden="true"><span>.</span><span>.</span><span>.</span></span>{/if}
        </div>
        <div class="processing-track" class:processing={!!busy} role={busy ? "progressbar" : undefined} aria-label={busy ? p("Procesando documento") : undefined} aria-busy={!!busy}><span></span></div>
      </div>
      <span class="format-note">TXT · MD · DOCX · PDF · PNG · JPG · TIFF · HEIC</span>
    </div>
    {#if pasteOpen}
      <div class="paste-panel"><div class="paste-heading"><label for="private-paste">{p("Texto privado · solo en esta sesión")}</label><button class="paste-close" aria-label={p("Cerrar pegar texto")} onclick={() => { pasteOpen = false; pasteText = ""; }}>×</button></div><textarea id="private-paste" bind:value={pasteText} disabled={!!busy} placeholder={p("Pega el contenido del documento…")}></textarea><button class="primary" disabled={!!busy || !pasteText.trim()} onclick={pasteDocument}>{p("Usar texto local")}</button></div>
    {/if}
    {#if session}
      {#if mapOpen}
        <section class="map-page" aria-label={p("Mapa privado")}>
          <div class="panel-heading"><div><p class="eyebrow">{p("MAPA PRIVADO · SOLO EN ESTE EQUIPO")}</p><h2>{p("Revisa las entidades y sus aliases.")}</h2></div><button class="secondary" onclick={() => mapOpen = false}>{p("← Volver al documento")}</button></div>
          <div class="map-heading"><h2>{p("Mapa privado")}</h2><span>{entities.length} {p("entradas · solo local")}</span></div>
          {#if session.analyzed}
            <p class="map-help">{p("Corrige detecciones y aliases. Para unir variantes de una persona usa el mismo alias. Eliminar una entrada restaura ese dato en la salida: revisa con cuidado.")}</p>
            <div class="entity-list">
              {#each entities as entity, i}
                <div class="entity-row">
                  <label><span>{p("Categoría")}</span><select bind:value={entity.kind} onchange={edited} disabled={!!busy}>{#each ENTITY_KINDS as kind}<option value={kind.value}>{p(kind.label)}</option>{/each}</select></label>
                  <label><span>{p("Valor privado")}</span><input aria-label={`${p("Valor privado")} ${i+1}`} bind:value={entity.original} oninput={edited} disabled={!!busy}/></label>
                  <label><span>{p("Alias")}</span><input aria-label={`Alias ${i+1}`} bind:value={entity.alias} oninput={edited} disabled={!!busy} spellcheck="false"/></label>
                  <button class="remove" aria-label={`${p("Eliminar entidad")} ${i+1}`} disabled={!!busy} onclick={() => { entities = entities.filter((_, n) => n !== i); edited(); }}>×</button>
                </div>
              {/each}
            </div>
            <div class="add-entity"><select aria-label={p("Tipo de nueva entidad")} bind:value={addKind} disabled={!!busy}>{#each ENTITY_KINDS as kind}<option value={kind.value}>{p(kind.label)}</option>{/each}</select><input aria-label={p("Nueva entidad privada")} bind:value={addText} placeholder={p("Texto exacto del original")} disabled={!!busy}/><button class="secondary" onclick={addEntity} disabled={!!busy || !addText.trim()}>{p("Añadir entidad")}</button></div>
            <button class="secondary apply-map" onclick={() => applyMap()} disabled={!!busy || !draftDirty}>{p("Aplicar cambios al mapa")}</button>
          {:else}<p class="empty-note">{p("Analiza el documento con Qwen para detectar entidades y construir el mapa.")}</p>{/if}
          {#if draftDirty}<p class="notice">{p("Aplica los cambios al mapa antes de abrir el documento en Edit.")}</p>{/if}
        </section>
      {:else}
      <div class="boundary-grid" class:original-expanded={originalExpanded}>
        <section class="private-panel">
          <div class="panel-heading"><div><p class="eyebrow">{p("PRIVATE — SE QUEDA EN ESTE EQUIPO")}</p><h2>{p("Documento original")}</h2></div><div class="panel-meta"><button class="trash-button" onclick={clear} disabled={!!busy} title={p("Borrar documento y empezar de cero")} aria-label={p("Borrar documento original")}><svg viewBox="0 0 20 20" aria-hidden="true"><path d="M4.5 6h11"></path><path d="M6.5 6v10a1 1 0 0 0 1 1h5a1 1 0 0 0 1-1V6"></path><path d="M8 6V4.5a1 1 0 0 1 1-1h2a1 1 0 0 1 1 1V6"></path><path d="M10 9v5"></path></svg></button></div></div>
          <div class="text-window">
            <button class="view-toggle" onclick={() => originalExpanded = !originalExpanded} title={p(originalExpanded ? "Volver a pantalla dividida" : "Ampliar documento original")} aria-label={p(originalExpanded ? "Volver a pantalla dividida" : "Ampliar documento original")} aria-pressed={originalExpanded}>{#if originalExpanded}<svg viewBox="0 0 20 20" aria-hidden="true"><path d="M12.5 4.5 7 10l5.5 5.5M7 10h10" /></svg>{:else}<svg viewBox="0 0 20 20" aria-hidden="true"><path d="M7 3H3v4M13 3h4v4M3 13v4h4M17 13v4h-4" /></svg>{/if}</button>
            <pre class="document-text original" data-testid="private-original">{session.original}</pre>
          </div>
          <div class="panel-footer" aria-hidden="true"></div>
        </section>
        <section class="sanitized-panel" class:panel-hidden={originalExpanded}>
          <div class="panel-heading"><div><p class="eyebrow">SAFE — SANITIZED</p><h2>{p("Versión para compartir")}</h2></div><div class="panel-meta"><button class="trash-button" onclick={clearResult} disabled={!!busy || !session.analyzed} title={p("Borrar versión sanitizada para repetir el análisis")} aria-label={p("Borrar versión sanitizada")}><svg viewBox="0 0 20 20" aria-hidden="true"><path d="M4.5 6h11"></path><path d="M6.5 6v10a1 1 0 0 0 1 1h5a1 1 0 0 0 1-1V6"></path><path d="M8 6V4.5a1 1 0 0 1 1-1h2a1 1 0 0 1 1 1V6"></path><path d="M10 9v5"></path></svg></button></div></div>
          {#if session.analyzed}
            <div class="text-window">
              <button class="view-toggle" onclick={() => { if (session) onEdit(session); }} disabled={!!busy || draftDirty} title={p("Abrir versión sanitizada en Edit")} aria-label={p("Abrir versión sanitizada en Edit")}><svg viewBox="0 0 20 20" aria-hidden="true"><path d="M7 3H3v4M13 3h4v4M3 13v4h4M17 13v4h-4" /></svg></button>
              <pre class="document-text sanitized" data-testid="sanitized-document">{session.sanitized}</pre>
            </div>
            {#if session.warnings.length}<div class="leak-alert" role="alert"><strong>{p("Posibles filtraciones · vista previa bloqueada")}</strong>{#each session.warnings as warning}<p>{p(warning)}</p>{/each}</div>{/if}
            {#if session.advisories.length}<div class="advisory-box" data-testid="privacy-advisories"><strong>{p("Lo que esta revisión no puede comprobar")}</strong>{#each session.advisories as advisory}<p>{p(advisory)}</p>{/each}</div>{/if}
            {#if draftDirty}<p class="notice">{p("La salida aún corresponde al mapa anterior. Aplica los cambios para validarla de nuevo.")}</p>{/if}
            <div class="review-controls panel-footer">
              <button class="secondary" onclick={() => { if (session) onEdit(session); }} disabled={!!busy || draftDirty}>{p("Abrir en Edit")}</button>
            </div>
          {:else}<div class="empty-sanitized"><span aria-hidden="true">◇</span><p>{p("La versión sanitizada aparecerá aquí después del análisis local.")}</p></div>{/if}
        </section>
      </div>
      {/if}
    {:else}
      <div class="welcome">
        <img class="welcome-mark" src={belegalMark} alt="" />
        <h2>{p("Primero, importa un documento privado.")}</h2>
        <p>{p("Importa un archivo o pega texto. Qwen detectará entidades en este equipo; tú revisarás qué información puede formar parte de una futura solicitud.")}</p>
        <button class="welcome-import" onclick={importDocument} disabled={!!busy}>
          <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true"><path d="M12 16V3m-5 5 5-5 5 5M4 15v5a1 1 0 0 0 1 1h14a1 1 0 0 0 1-1v-5" /></svg>
          <span>{p("Importa un documento")}</span>
          <small>PDF · DOCX · PNG · JPG · TIFF · HEIC · TXT · MD</small>
        </button>
        </div>
    {/if}
</section>

<style>
  .privacy-workflow { --ai-border: linear-gradient(110deg, #a58de2 0%, #8b9cec 28%, #caa0de 56%, #e7aa98 82%, #e4b68a 100%); display:flex; flex-direction:column; flex:1; min-height:0; overflow:hidden; border:1px solid var(--line); border-top:0; border-radius:0 0 6px 6px; background:var(--background); padding:16px 22px; }
  .intro { display:flex; justify-content:space-between; align-items:center; gap:20px; flex:none; min-height:76px; margin-bottom:8px; }
  .intro-status { flex:0 0 clamp(240px,30vw,420px); display:flex; flex-direction:column; align-items:flex-end; gap:4px; }
  .status-slot { width:100%; height:34px; overflow:auto; text-align:right; }
  .status-slot p { margin:0; font-size:12px; line-height:1.5; }
  .error { color:var(--error); }

  .eyebrow { margin:0 0 8px; font-size:11px; font-weight:500; letter-spacing:.12em; color:var(--muted); }
  h1,h2 { font-family:var(--font-heading); font-weight:400; color:var(--text); }
  h1 em { font-family:"Newsreader",var(--font-heading); font-style:italic; font-weight:300; }
  h1 { margin:0 0 5px; font-size:30px; line-height:1.2; } h2 { margin:0; font-size:21px; line-height:1.3; }
  button { cursor:pointer; font-size:13px; border-radius:4px; padding:8px 12px; white-space:nowrap; }
  button:disabled { opacity:.45; cursor:default; }
  .primary { background:var(--button); border:1px solid var(--button); color:var(--button-text); } .primary:hover:not(:disabled) { background:var(--button-hover); }
  .secondary { background:var(--surface); border:1px solid var(--line-dark); color:var(--text); } .secondary:hover:not(:disabled) { background:var(--hover); }
  .secondary.analyze-button {
    border-color: transparent;
    background: linear-gradient(var(--surface), var(--surface)) padding-box, var(--ai-border) border-box;
  }
  .secondary.analyze-button:hover:not(:disabled) {
    background: linear-gradient(var(--hover), var(--hover)) padding-box, var(--ai-border) border-box;
  }
  .secondary.active-page { border-color:var(--text); background:var(--panel); }
  .map-page { flex:1; min-height:0; width:100%; overflow:auto; max-width:1100px; margin:0 auto; padding:26px; background:var(--surface); border:1px solid var(--line); border-radius:5px; }
  .map-page .panel-heading { flex-wrap:wrap; }
  .map-page .entity-row { grid-template-columns:160px minmax(0,1fr) minmax(0,1fr) 30px; gap:12px; }
  .map-page .entity-row input,.map-page .entity-row select { padding:10px; font-size:14px; }
  .map-page .entity-row label:not(:first-child) span { text-align:center; font-size:15px; font-weight:500; color:var(--text); margin-bottom:8px; }
  .map-page .entity-row input { text-align:center; }
  .map-page .entity-list { gap:14px; margin:22px 0; }
  @media(max-width:650px) { .map-page { padding:16px; } .map-page .entity-row { grid-template-columns:minmax(0,1fr) minmax(0,1fr) 26px; gap:8px; } .map-page .entity-row label:first-child { grid-column:1 / -1; } }
  .paste-heading { display:flex; align-items:center; justify-content:space-between; gap:12px; margin-bottom:8px; } .paste-heading label { margin:0; } .paste-close { border:0; background:transparent; color:var(--muted); font-size:24px; line-height:1; padding:2px 6px; }
  .document-actions { flex:none; display:flex; align-items:center; gap:9px; flex-wrap:wrap; margin:8px 0 14px; } .format-note { font-size:12px; color:var(--muted); white-space:nowrap; }
  .processing-slot { flex:1 1 90px; min-width:90px; padding:0 6px; align-self:stretch; display:flex; flex-direction:column; justify-content:flex-end; gap:4px; }
  .processing-label { display:flex; align-items:center; justify-content:center; min-height:15px; font-size:11px; line-height:15px; color:var(--muted); min-width:0; }
  .processing-copy { overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
  .processing-dots { display:inline-flex; flex:none; width:12px; margin-left:1px; }
  .processing-dots span { animation:processing-dot 1.4s ease-in-out infinite; }
  .processing-dots span:nth-child(2) { animation-delay:.2s; }
  .processing-dots span:nth-child(3) { animation-delay:.4s; }
  @keyframes processing-dot { 0%,80%,100% { opacity:.25; } 40% { opacity:1; } }
  .processing-track { height:5px; overflow:hidden; border-radius:999px; background:var(--line); visibility:hidden; }
  .processing-track.processing { visibility:visible; }
  .processing-track span { display:block; height:100%; width:38%; border-radius:inherit; background:var(--ai-border); animation:processing-slide 1.5s ease-in-out infinite; }
  @keyframes processing-slide { from { transform:translateX(-110%); } to { transform:translateX(365%); } }
  @media(prefers-reduced-motion:reduce) { .processing-track span { animation:none; width:100%; opacity:.65; } .processing-dots span { animation:none; opacity:1; } }
  @media(max-width:760px) { .intro { align-items:flex-start; } .intro-status { flex-basis:200px; } }
  @media(max-width:540px) { .intro { flex-direction:column; gap:12px; } .intro-status { flex-basis:auto; width:100%; } }
  .boundary-grid { flex:1; min-height:0; display:grid; grid-template-rows:minmax(0,1fr); grid-template-columns:minmax(0,1fr) minmax(0,1fr); border:1px solid var(--line); border-radius:5px; overflow:hidden; }
  .private-panel,.sanitized-panel { display:flex; flex-direction:column; min-width:0; min-height:0; padding:16px; } .private-panel { background:var(--panel); border-right:1px solid var(--line); } .sanitized-panel { background:var(--surface); }
  .text-window { flex:1; min-height:0; position:relative; margin:8px 0; }
  .text-window .document-text { height:100%; margin:0; padding-right:64px; }
  .view-toggle { position:absolute; top:10px; right:28px; z-index:1; display:grid; place-items:center; width:28px; height:28px; padding:5px; border:1px solid var(--line); background:var(--surface); color:var(--muted); }
  .view-toggle:hover:not(:disabled) { color:var(--text); background:var(--hover); }
  .view-toggle svg { width:18px; height:18px; fill:none; stroke:currentColor; stroke-width:1.5; stroke-linecap:round; stroke-linejoin:round; }
  .boundary-grid .panel-heading { height:64px; }
  .boundary-grid .panel-meta { height:100%; justify-content:flex-end; }
  .panel-footer { flex:none; display:flex; flex-direction:column; justify-content:center; min-height:44px; margin:8px 0 0; }
  .panel-meta { display:flex; flex-direction:column; align-items:flex-end; gap:8px; }
  .trash-button { display:grid; place-items:center; width:30px; height:30px; padding:5px; border:1px solid transparent; background:transparent; color:var(--muted); }
  .trash-button:hover:not(:disabled) { color:var(--error); border-color:var(--line); background:var(--hover); }
  .trash-button svg { width:20px; height:20px; fill:none; stroke:currentColor; stroke-width:1.5; stroke-linecap:round; stroke-linejoin:round; }
  .panel-heading { flex:none; display:flex; align-items:flex-start; justify-content:space-between; gap:10px; margin-bottom:6px; }
  .map-help { font-size:12px; color:var(--muted); line-height:1.5; margin:4px 0; }
  .document-text { white-space:pre-wrap; overflow-wrap:anywhere; font:400 15px/1.65 "Helvetica Neue",Arial,sans-serif; padding:16px; flex:1; min-height:0; margin:8px 0; overflow-y:auto; border:1px solid var(--line); border-radius:3px; color:var(--text); }
  .original { background:var(--background); } .sanitized { background:var(--surface); }
  .map-heading { display:flex; justify-content:space-between; gap:10px; align-items:center; margin-top:25px; } .map-heading span { font-size:11px; color:var(--muted); }
  .entity-list { margin:14px 0; display:flex; flex-direction:column; gap:9px; }
  .entity-row { display:grid; grid-template-columns:96px minmax(0,1fr) minmax(0,.85fr) 22px; gap:6px; align-items:end; }
  .entity-row label { min-width:0; } .entity-row label span { display:block; font-size:10px; color:var(--muted); margin-bottom:3px; }
  input,select,textarea { border:1px solid var(--line-dark); border-radius:3px; background:var(--surface); color:var(--text); padding:7px 8px; font-size:12px; min-width:0; width:100%; }
  .entity-row input { padding:7px 6px; } .entity-row select { padding:7px 3px; } .remove { padding:5px 0; background:transparent; border:0; color:var(--muted); font-size:20px; }
  .add-entity { display:flex; flex-wrap:wrap; gap:7px; } .add-entity select { width:110px; } .add-entity input { flex:1; min-width:110px; } .apply-map { margin-top:12px; }
  .leak-alert { font-size:12px; padding:12px; border:1px solid var(--error); color:var(--error); background:var(--error-background); } .leak-alert p { margin:6px 0 0; }
  .advisory-box { font-size:12px; padding:12px; margin-top:10px; border:1px solid var(--line-dark); background:var(--panel); } .advisory-box p { margin:6px 0 0; }
  .review-controls { flex:none; display:flex; flex-direction:column; align-items:flex-start; gap:12px; margin:8px 0 0; }
  .notice { font-size:12px; line-height:1.6; color:var(--muted); border-top:1px solid var(--line); padding-top:8px; margin:8px 0 0; }
  .empty-note { font-size:13px; color:var(--muted); }
  .empty-sanitized { display:grid; place-content:center; text-align:center; flex:1; min-height:0; color:var(--muted); font-size:14px; } .empty-sanitized > span { font-size:40px; }
  .welcome-import { display:flex; flex-direction:column; align-items:center; gap:9px; width:min(100%, 430px); margin:22px auto 0; padding:20px 24px; border:1px dashed var(--line-dark); border-radius:6px; background:var(--surface); color:var(--text); cursor:pointer; font-size:15px; }
  .welcome-import small { font-size:12px; color:var(--muted); line-height:1.5; }
  .welcome-import:hover { background:var(--panel); border-color:var(--text); }
  .welcome-import:focus-visible { outline:2px solid var(--text); outline-offset:4px; }
  .welcome-import:disabled { opacity:.5; cursor:wait; }
  .welcome { text-align:center; max-width:680px; margin:20px auto; } .welcome-mark { display:block; width:94px; height:94px; object-fit:contain; margin:0 auto 20px; filter:drop-shadow(0 4px 9px rgba(55,55,49,.16)); } .welcome h2 { font-size:27px; } .welcome p { font-size:15px; color:var(--muted); line-height:1.7; }
  .paste-panel { flex:none; padding:12px; background:var(--panel); border:1px solid var(--line); margin:15px 0; } .paste-panel label { display:block; font-size:13px; margin-bottom:8px; } .paste-panel textarea { height:100px; min-height:70px; max-height:120px; resize:vertical; font-size:15px; margin-bottom:10px; }
  @media(max-width:1000px) { .boundary-grid { grid-template-columns:1fr; grid-template-rows:repeat(2,minmax(0,1fr)); } .private-panel,.sanitized-panel { overflow:auto; } .private-panel { border-right:0; border-bottom:1px solid var(--line); } .format-note { margin-left:0; } .intro { align-items:flex-start; } }
  .boundary-grid.original-expanded { grid-template-columns:minmax(0,1fr); grid-template-rows:minmax(0,1fr); }
  .original-expanded .private-panel { border:0; }
  .sanitized-panel.panel-hidden { display:none; }
</style>
