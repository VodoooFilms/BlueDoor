<script lang="ts">
  import { privateCopy as p } from "./lib/private-i18n";
  import WelcomeSplash from "./components/WelcomeSplash.svelte";
  import PrivateAI from "./components/PrivateAI.svelte";
  import Recorder from "./components/Recorder.svelte";
  import TranscriptView from "./components/TranscriptView.svelte";
  import ModelsView from "./components/ModelsView.svelte";
  import type { Transcript, DocFormat } from "./lib/types";
  import type { PrivacySession } from "./lib/privacy";
  import { DEFAULT_FORMAT } from "./lib/types";
  import { getCopy, toggleLang } from "./lib/i18n-store.svelte";
  import { getSttModel } from "./lib/models-store.svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { generateId } from "./lib/utils";
  import { onMount, tick, untrack } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";

  let t = $derived(getCopy());

  let transcripts = $state<Transcript[]>([]);
  let activeTranscript = $state<Transcript | null>(null);
  let theme = $state<"dark" | "light">("light");
  let showModels = $state(false);
  let workflow = $state<"private" | "audio">("private");
  let privacyBusy = $state(false);
  let privateSession = $state<PrivacySession | null>(null);
  let privateEditVisible = $state(false);
  let privateComparisonRequest = $state(0);
  let privateEditHtml = $state<string | null>(null);
  let privateEditFormat = $state<DocFormat>(DEFAULT_FORMAT);
  const privateTranscript = $derived(privateSession?.analyzed ? {
    id: privateSession.id, title: "Documento protegido", text: privateSession.sanitized,
    corrected: privateSession.sanitized, correctedHtml: privateEditHtml ?? undefined,
    format: privateEditFormat, segments: [], createdAt: new Date(), duration: 0,
    language: "es", model: "local", audioSource: "file" as const,
  } : null);
  let fullScreen = $state(false);
  async function toggleFullScreen() {
    const window = getCurrentWindow();
    fullScreen = !(await window.isFullscreen());
    await window.setFullscreen(fullScreen);
  }
  const welcomeSeenKey = "belegal-local-demo-welcome-seen";
  let welcomeVisible = $state(false);
  let welcomeReady = $state(false);
  let welcomeLoading = $state(true);
  async function checkWelcomeModels() {
    welcomeLoading = true;
    try {
      const all = await invoke<{ whisper: { installed: boolean }[]; parakeet: { installed: boolean }[]; privacy: { id: string; installed: boolean }[] }>("list_models");
      const selected = localStorage.getItem("belegal-privacy-model") || "qwen3-fast";
      welcomeReady = (all.whisper.some(m => m.installed) || all.parakeet.some(m => m.installed))
        && all.privacy.some(m => m.id === selected && m.installed);
    } catch { welcomeReady = false; }
    finally { welcomeLoading = false; }
  }
  function closeWelcome() {
    localStorage.setItem(welcomeSeenKey, "true");
    welcomeVisible = false;
  }
  function openAbout() {
    welcomeVisible = true;
    void checkWelcomeModels();
  }
  function continueWelcome() {
    if (welcomeLoading) return;
    if (!welcomeReady && (recorderBusy || importing || privacyBusy || modelsBusy)) return;
    closeWelcome();
    if (!welcomeReady) openModels();
  }
  let firstRun = $state(false);
  let modelsBusy = $state(false);
  let hydrated = $state(false);
  let storageError = $state("");
  let recorderBusy = $state(false);
  let editorDirty = $state(false);
  let editorSave: (() => void | Promise<void>) | null = null;
  let pendingNavigation = $state<(() => void) | null>(null);
  let savingBeforeLeave = $state(false);
  let allowWindowClose = false;
  let pendingWrites = $state(0);
  let booting = $state(false);
  let saveQueue: Promise<void> = Promise.resolve();
  /** Última transcripción abierta en la pantalla Editar (persistida). */
  let editId = $state<string | null>(null);

  // Flujo de importación de audio (el reconocimiento es siempre Auto).
  let importing = $state(false);
  let importError = $state("");
  let importToastTimer: ReturnType<typeof setTimeout> | undefined;
  // --- Persistencia local (solo en este dispositivo) ---
  interface StoredSeg {
    start: number;
    end: number;
    text: string;
    speaker?: number;
  }
  interface StoredTranscript {
    id: string;
    title: string;
    text: string;
    segments: StoredSeg[];
    createdAt: string;
    duration: number;
    language: string;
    model: string;
    audioSource: string;
    fileName?: string | null;
    corrected?: string | null;
    correctedHtml?: string | null;
    format?: DocFormat | null;
    speakerNames?: Record<string, string> | null;
  }

  function toStored(t: Transcript): StoredTranscript {
    return {
      id: t.id,
      title: t.title,
      text: t.text,
      segments: t.segments.map((s) => ({ start: s.start, end: s.end, text: s.text, speaker: s.speaker })),
      createdAt: t.createdAt.toISOString(),
      duration: t.duration,
      language: t.language,
      model: t.model,
      audioSource: t.audioSource,
      fileName: t.fileName ?? null,
      corrected: t.corrected ?? null,
      correctedHtml: t.correctedHtml ?? null,
      format: t.format ?? DEFAULT_FORMAT,
      speakerNames: t.speakerNames ?? null,
    };
  }

  function fromStored(s: StoredTranscript): Transcript {
    return {
      id: s.id,
      title: s.title,
      text: s.text,
      segments: s.segments.map((seg) => ({ start: seg.start, end: seg.end, text: seg.text, speaker: seg.speaker })),
      createdAt: new Date(s.createdAt),
      duration: s.duration,
      language: s.language,
      model: s.model,
      audioSource: (s.audioSource as "recording" | "file") || "recording",
      fileName: s.fileName ?? undefined,
      corrected: s.corrected ?? undefined,
      correctedHtml: s.correctedHtml ?? undefined,
      format: { ...DEFAULT_FORMAT, ...(s.format ?? {}) },
      speakerNames: s.speakerNames ?? undefined,
    };
  }

  function persistTranscripts() {
    // Capture now and keep IPC writes ordered; delayed reads of mutable state
    // can otherwise acknowledge or persist the wrong revision.
    let snapshot: StoredTranscript[];
    try {
      snapshot = transcripts.map(toStored);
    } catch (e) {
      storageError = String(e);
      return;
    }
    pendingWrites += 1;
    saveQueue = saveQueue.then(async () => {
      try {
        await invoke("save_transcripts", { transcripts: snapshot });
        storageError = "";
      } catch (e) {
        storageError = String(e);
      } finally {
        pendingWrites -= 1;
      }
    });
  }

  async function refreshModelState() {
    try {
      const all = await invoke<{ whisper: { installed: boolean }[]; parakeet: { installed: boolean }[] }>("list_models");
      const voiceModelReady = all.whisper.some((m) => m.installed) || all.parakeet.some((m) => m.installed);
      firstRun = !voiceModelReady;
      showModels = firstRun;
    } catch {
      firstRun = true;
      showModels = true;
    }
  }

  async function hydrate() {
    if (booting) return;
    booting = true;
    storageError = "";
    try {
      await invoke("check_installation");
      const stored = await invoke<StoredTranscript[]>("load_transcripts");
      transcripts = stored.map(fromStored);
      hydrated = true;
    } catch (e) {
      storageError = String(e);
      return;
    } finally {
      booting = false;
    }
    await refreshModelState();
  }

  onMount(async () => {
    const storedTheme = localStorage.getItem("belegal-local-demo-theme");
    if (storedTheme === "light" || storedTheme === "dark") theme = storedTheme;
    await hydrate();
    if (localStorage.getItem(welcomeSeenKey) !== "true") openAbout();
    editId = localStorage.getItem("belegal-local-demo-edit-id");
  });

  onMount(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;
    getCurrentWindow().onCloseRequested((event) => {
      if (allowWindowClose || (!editorDirty && pendingWrites === 0 && !storageError)) return;
      event.preventDefault();
      requestNavigation(() => {
        allowWindowClose = true;
        void getCurrentWindow().close();
      });
    }).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    }).catch((error) => console.error(error));
    return () => {
      disposed = true;
      unlisten?.();
    };
  });

  $effect(() => {
    document.documentElement.dataset.theme = theme;
    localStorage.setItem("belegal-local-demo-theme", theme);
  });

  $effect(() => {
    transcripts;
    if (!hydrated) return;
    untrack(() => persistTranscripts());
  });

  function toggleTheme() {
    theme = theme === "dark" ? "light" : "dark";
  }

  function requestNavigation(action: () => void) {
    if ((editorDirty || pendingWrites > 0 || storageError) && (activeTranscript || privateEditVisible)) {
      pendingNavigation = action;
      return;
    }
    action();
  }

  function discardAndLeave() {
    const action = pendingNavigation;
    pendingNavigation = null;
    editorDirty = false;
    editorSave = null;
    action?.();
  }

  async function saveAndLeave() {
    if (savingBeforeLeave || (editorDirty && !editorSave)) return;
    savingBeforeLeave = true;
    if (editorDirty) await editorSave?.();
    else if (storageError) persistTranscripts();
    await tick();
    if (privateEditVisible && editorDirty) { savingBeforeLeave = false; return; }
    await saveQueue;
    savingBeforeLeave = false;
    if (storageError) return;
    discardAndLeave();
  }

  // La marca y Private Legal vuelven a la comparación, cerrando el mapa.
  function goPrivate() {
    if (recorderBusy || importing || privacyBusy || modelsBusy) return;
    requestNavigation(() => { workflow = "private"; privateEditVisible = false; showModels = false; privateComparisonRequest += 1; });
  }

  function goHome() {
    if (recorderBusy || importing || privacyBusy || modelsBusy) return;
    requestNavigation(() => {
      activeTranscript = null;
      workflow = "audio";
      privateEditVisible = false;
      showModels = firstRun;
    });
  }

  // "Editar" vuelve a la última transcripción abierta desde cualquier pestaña.
  function goEdit() {
    if (recorderBusy || importing || privacyBusy || modelsBusy || (!privateTranscript && !editTranscript)) return;
    requestNavigation(() => {
      showModels = false;
      if (privateTranscript) { workflow = "private"; privateEditVisible = true; }
      else { activeTranscript = editTranscript; workflow = "audio"; privateEditVisible = false; }
    });
  }

  function openModels() {
    if (recorderBusy || importing || privacyBusy || modelsBusy) return;
    requestNavigation(() => { workflow = "audio"; privateEditVisible = false; showModels = true; firstRun = false; });
  }

  const editTranscript = $derived(
    editId ? (transcripts.find((t) => t.id === editId) ?? null) : null
  );

  $effect(() => {
    editId;
    if (!hydrated) return;
    if (editId) localStorage.setItem("belegal-local-demo-edit-id", editId);
    else localStorage.removeItem("belegal-local-demo-edit-id");
  });

  function handleNewTranscript(transcript: Transcript) {
    transcripts = [transcript, ...transcripts];
    activeTranscript = transcript;
    editId = transcript.id;
  }

  function handleSelectTranscript(transcript: Transcript) {
    activeTranscript = transcript;
    editId = transcript.id;
  }

  function handleDeleteTranscript(id: string) {
    transcripts = transcripts.filter((t) => t.id !== id);
    if (activeTranscript?.id === id) {
      activeTranscript = transcripts[0] ?? null;
    }
    if (editId === id) {
      editId = transcripts[0]?.id ?? null;
    }
  }

  function handleUpdateTranscriptText(
    id: string,
    text: string,
    format?: DocFormat,
    html?: string | null,
    speakerNames?: Record<string, string>
  ) {
    // `corrected` es el texto plano (PDF); `correctedHtml` es el
    // contenido ricotexto del editor. Los segments originales se conservan.
    transcripts = transcripts.map((t) =>
      t.id === id
        ? {
            ...t,
            corrected: text,
            correctedHtml: html ?? undefined,
            ...(format ? { format } : {}),
            ...(speakerNames ? { speakerNames } : {}),
          }
        : t
    );
    if (activeTranscript?.id === id) {
      activeTranscript = {
        ...activeTranscript,
        corrected: text,
        correctedHtml: html ?? undefined,
        ...(format ? { format } : {}),
        ...(speakerNames ? { speakerNames } : {}),
      };
    }
  }

  function receivePrivateSession(next: PrivacySession | null) {
    if (!next || next.id !== privateSession?.id || next.sanitized !== privateSession.sanitized) {
      privateEditHtml = null;
      privateEditFormat = DEFAULT_FORMAT;
    }
    privateSession = next;
    if (!next?.analyzed) privateEditVisible = false;
  }

  async function savePrivateEdit(_id: string, text: string, format?: DocFormat, html?: string | null) {
    if (!privateSession) throw new Error(p("La sesión privada ya no está disponible."));
    const next = await invoke<PrivacySession>("update_private_document", {
      id: privateSession.id, revision: privateSession.revision, text,
    });
    privateSession = next;
    privateEditHtml = html ?? null;
    if (format) privateEditFormat = format;
  }

  async function confirmPrivateReview() {
    if (!privateSession) throw new Error(p("La sesión privada ya no está disponible."));
    const next = await invoke<PrivacySession>("review_private_document", {
      id: privateSession.id, revision: privateSession.revision,
      entities: $state.snapshot(privateSession.entities), confirm: true,
    });
    privateSession = next;
    if (!next.reviewed) throw new Error(next.warnings.join(" ") || p("Revisa el mapa privado antes de exportar."));
  }

  async function validatePrivateExport(text: string, title?: string) {
    if (!privateSession) throw new Error(p("La sesión privada ya no está disponible."));
    // El título acaba en el encabezado y en los metadatos del PDF, así que
    // Rust lo valida con las mismas reglas que el cuerpo.
    await invoke("validate_private_export", { id: privateSession.id, revision: privateSession.revision, text, title: title ?? null });
  }

  function toggle() {
    toggleLang();
  }

  // El botón "Importar audio" del top nav transcribe un archivo y crea una nueva transcripción.
  async function handleImportAudio() {
    if (importing) return;
    importError = "";
    importing = true;
    try {
      const all = await invoke<{
        whisper: { id: string; path: string; installed: boolean }[];
        parakeet: { id: string; path: string; installed: boolean }[];
      }>("list_models");
      const pool = [...all.parakeet, ...all.whisper];
      const selectedModel = pool.find((m) => m.id === getSttModel() && m.installed)
        ?? pool.find((m) => m.installed);
      if (!selectedModel) {
        importError = t.needsModel;
        return;
      }
      const path = await invoke<string | null>("choose_audio_file");
      if (!path) return;
      const fileName = path.split(/[\\/]/).pop() || path;
      const trans = await invoke<{ id: string; text: string; segments: { start: number; end: number; text: string; speaker?: number }[]; language: string; duration: number }>("transcribe_file", {
        filePath: path,
        config: {
          modelPath: selectedModel.path,
          language: "es",
          threads: 0,
          diarize: await invoke<boolean>("diarization_available").catch(() => false),
        },
      });
      const transcript: Transcript = {
        id: generateId(),
        title: fileName,
        text: trans.text,
        segments: trans.segments,
        createdAt: new Date(),
        duration: trans.duration,
        language: trans.language,
        model: selectedModel.id,
        audioSource: "file",
        fileName,
      };
      handleNewTranscript(transcript);
    } catch (e: any) {
      console.error(e);
      importError = String(e);
    } finally {
      importing = false;
    }
  }

  // El aviso de importación se auto-oculta a los pocos segundos.
  $effect(() => {
    if (importError) {
      clearTimeout(importToastTimer);
      importToastTimer = setTimeout(() => (importError = ""), 4000);
    }
  });
</script>

<div class="shell">
  <header class="masthead">
    <span class="brand" role="button" tabindex="0" title={p("Acerca de BeLegal")} onclick={openAbout} onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); openAbout(); } }}>
      <span class="brand-wordmark">Be<i>Legal</i></span>
      <span class="brand-demo">Local demo</span>
    </span>
    {#if workflow === "audio" && activeTranscript && !showModels}
      <span class="back-link" role="button" tabindex="0" onclick={goHome} onkeydown={(e) => { if (e.key === 'Enter') goHome(); }}>
        &larr; {t.newTranscript}
      </span>
    {/if}
    <div class="nav-right">
      <div class="nav-row">
        <button class="nav-models" class:nav-active={workflow === "private" && !privateEditVisible} onclick={goPrivate} disabled={recorderBusy || importing || privacyBusy || modelsBusy}>Private Legal</button>
        <button class="nav-models" class:nav-active={workflow === "audio" && !showModels && !activeTranscript} id="recordToggle" onclick={goHome} disabled={recorderBusy || importing || privacyBusy || modelsBusy} title={recorderBusy || importing ? t.navigationBlocked : t.recordNav}>{t.recordNav}</button>
        <button class="nav-models" class:nav-active={!showModels && (privateEditVisible || (workflow === "audio" && !!activeTranscript))} id="editToggle" onclick={goEdit} disabled={recorderBusy || importing || privacyBusy || modelsBusy || (!privateTranscript && !editTranscript)} title={recorderBusy || importing ? t.navigationBlocked : t.editNav}>{t.editNav}</button>
        <button class="nav-models" class:nav-active={showModels} id="modelsToggle" onclick={openModels} disabled={recorderBusy || importing || privacyBusy || modelsBusy} title={recorderBusy || importing ? t.navigationBlocked : t.modelsTitle}>{t.modelsTitle}</button>
        <button class="language-toggle" id="languageToggle" aria-label={t.switchLanguage} title={t.switchLanguage} onclick={toggle}>{t.langLabel}</button>
        <button class="theme-toggle" id="themeToggle" aria-label={theme === "dark" ? t.lightTheme : t.darkTheme} title={theme === "dark" ? t.lightTheme : t.darkTheme} onclick={toggleTheme}>
          <svg class="sun-icon" viewBox="0 0 20 20" aria-hidden="true">
            <circle cx="10" cy="10" r="3.2"></circle>
            <path d="M10 2v1.5M10 16.5V18M2 10h1.5M16.5 10H18M4.3 4.3l1.1 1.1M14.6 14.6l1.1 1.1M15.7 4.3l-1.1 1.1M5.4 14.6l-1.1 1.1"></path>
          </svg>
          <svg class="moon-icon" viewBox="0 0 20 20" aria-hidden="true">
            <path d="M16.2 12.8A6.7 6.7 0 0 1 7.2 3.8a6.7 6.7 0 1 0 9 9Z"></path>
          </svg>
        </button>
        <button class="fullscreen-toggle" id="fullscreenToggle" aria-label={fullScreen ? p("Salir de pantalla completa") : p("Pantalla completa")} title={fullScreen ? p("Salir de pantalla completa") : p("Pantalla completa")} onclick={toggleFullScreen}>
          <svg viewBox="0 0 20 20" aria-hidden="true"><path d="M7 3H3v4M13 3h4v4M3 13v4h4M17 13v4h-4" /></svg>
        </button>
      </div>
    </div>
  </header>

  {#if storageError && hydrated}
    <div role="alert" class="storage-error">{storageError}</div>
  {/if}

  {#if importError}
    <div class="app-toast" role="alert">{importError}</div>
  {/if}

  <div class="privacy-container" class:hidden={workflow !== "private" || privateEditVisible}>
    <PrivateAI comparisonRequest={privateComparisonRequest} externalSession={privateSession} onSessionChange={receivePrivateSession} onEdit={() => requestNavigation(() => { workflow = "private"; privateEditVisible = true; })} onBusyChange={(busy) => (privacyBusy = busy)} />
  </div>

  {#if workflow === "private" && privateEditVisible && privateTranscript}
    <TranscriptView transcript={privateTranscript} privateDocument privateReviewed={privateSession?.reviewed ?? false} privateId={privateSession?.id ?? null} privateRevision={privateSession?.revision ?? null} onBackToComparison={goPrivate} onConfirmReview={confirmPrivateReview} onValidateExport={validatePrivateExport} onSave={savePrivateEdit} onDirtyChange={(dirty, save) => { editorDirty = dirty; editorSave = save; }} />
  {/if}

  {#if workflow !== "private"}
  {#if !hydrated}
    <section class="boot-error" role="alert">
      <strong>{t.startupError}</strong>
      <span>{t.historyPreserved}</span>
      {#if storageError}<span class="boot-detail">{storageError}</span>{/if}
      <button class="boot-retry" onclick={hydrate} disabled={booting}>
        {booting ? t.retrying : t.retry}
      </button>
    </section>
  {:else if showModels}
    <ModelsView
      onClose={goPrivate}
      onDone={() => {
        firstRun = false;
        showModels = false;
      }}
      onSetupComplete={() => {
        modelsBusy = false;
        firstRun = false;
        goPrivate();
      }}
      firstRun={firstRun}
      onBusyChange={(busy) => (modelsBusy = busy)}
    />
  {:else if activeTranscript}
    <TranscriptView transcript={activeTranscript} onSave={handleUpdateTranscriptText} onDirtyChange={(dirty, save) => { editorDirty = dirty; editorSave = save; }} />
  {:else}
    <Recorder onTranscribe={handleNewTranscript} transcripts={transcripts} onSelect={handleSelectTranscript} onDelete={handleDeleteTranscript} activeTranscript={activeTranscript} onImport={handleImportAudio} importing={importing} onBusyChange={(busy) => (recorderBusy = busy)} />
  {/if}

  {/if}

  {#if welcomeVisible}
    <WelcomeSplash ready={welcomeReady} loading={welcomeLoading} onClose={closeWelcome} onContinue={continueWelcome} />
  {/if}

  {#if pendingNavigation}
    <div class="leave-overlay" role="presentation">
      <div class="leave-dialog" role="dialog" aria-modal="true" aria-labelledby="leave-title">
        <strong id="leave-title">{t.unsavedChangesTitle}</strong>
        <p>{t.unsavedChangesBody}</p>
        {#if storageError}<p class="leave-error" role="alert">{storageError}</p>{/if}
        <div class="leave-actions">
          <button onclick={() => (pendingNavigation = null)} disabled={savingBeforeLeave}>{t.cancel}</button>
          <button onclick={discardAndLeave} disabled={savingBeforeLeave}>{t.discardChanges}</button>
          <button class="leave-save" onclick={saveAndLeave} disabled={savingBeforeLeave}>{savingBeforeLeave ? t.retrying : t.saveAndLeave}</button>
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .leave-overlay { position: fixed; inset: 0; z-index: 1000; display: grid; place-items: center; background: rgba(0, 0, 0, .5); }
  .leave-dialog { width: min(420px, calc(100vw - 32px)); padding: 22px; border: 1px solid var(--line); border-radius: 12px; background: var(--surface); box-shadow: 0 12px 36px rgba(0, 0, 0, .35); }
  .leave-dialog strong { font-size: 16px; }
  .leave-dialog p { color: var(--muted); line-height: 1.5; }
  .leave-dialog .leave-error { color: var(--error); overflow-wrap: anywhere; }
  .leave-actions { display: flex; justify-content: flex-end; gap: 8px; flex-wrap: wrap; }
  .leave-actions button { padding: 8px 12px; border: 1px solid var(--line-dark); border-radius: 7px; color: var(--text); background: var(--surface); cursor: pointer; }
  .leave-actions button:disabled { opacity: .6; cursor: default; }
  .leave-actions .leave-save { border-color: var(--button); color: var(--button-text); background: var(--button); }
  .shell {
    min-height: 100vh;
    height: 100vh;
    padding: 16px 20px 20px;
    display: flex;
    flex-direction: column;
    gap: 0;
  }

  .masthead {
    min-height: 66px;
    padding: 10px 20px;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 6px 6px 0 0;
    display: flex;
    align-items: center;
    gap: 16px;
    color: var(--muted);
    font-size: 15px;
    font-weight: 500;
    letter-spacing: .01em;
  }

  .brand {
    display: inline-flex;
    align-items: center;
    color: var(--text);
    font-family: "Newsreader", Georgia, "Times New Roman", serif;
    font-size: 22px;
    letter-spacing: -.02em;
    line-height: 1;
    gap: 16px;
    font-weight: 400;
    white-space: nowrap;
    cursor: pointer;
    border-radius: 5px;
    padding: 3px 7px;
    margin-left: 2px;
    outline: none;
    -webkit-app-region: no-drag;
  }
  .brand-wordmark { display: inline-flex; align-items: baseline; }
  .brand-wordmark i { font-style: italic; font-weight: 300; }
  .brand-demo {
    padding-left: 16px;
    border-left: 1px solid var(--line-dark);
    font-family: "Helvetica Neue", Arial, sans-serif;
    font-size: 11px;
    font-weight: 400;
    letter-spacing: .08em;
    line-height: 1.4;
    color: var(--muted);
  }
  .brand:hover,
  .brand:focus-visible { background: var(--hover); }

  .back-link {
    padding: 2px 8px;
    border-radius: 5px;
    color: var(--accent);
    font-size: 14px;
    cursor: pointer;
    -webkit-app-region: no-drag;
  }
  .back-link:hover { background: var(--hover); }

  .nav-right {
    margin-left: auto;
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 5px;
    -webkit-app-region: no-drag;
  }
  .nav-row {
    display: flex;
    align-items: center;
    gap: 9px;
    flex-wrap: wrap;
    justify-content: flex-end;
  }

  .nav-models,
  .theme-toggle,
  .language-toggle,
  .fullscreen-toggle {
    display: grid;
    place-items: center;
    padding: 4px;
    border: 0;
    border-radius: 5px;
    background: transparent;
    color: var(--muted);
    cursor: pointer;
  }
  .nav-models:hover,
  .theme-toggle:hover,
  .language-toggle:hover,
  .fullscreen-toggle:hover { color: var(--text); background: var(--hover); }

  .nav-models {
    height: 28px;
    padding: 0 12px;
    font-size: 14px;
    font-weight: 600;
    color: var(--accent);
  }
  .nav-models:hover { color: var(--text); }
  .nav-active { background: var(--surface); border-bottom: 1px solid var(--text); }
  .privacy-container { display: flex; flex: 1; min-height: 0; }
  .nav-models:disabled { opacity: .45; cursor: default; }
  .nav-models:disabled:hover { color: var(--muted); background: transparent; }

  .theme-toggle svg { width: 19px; height: 19px; fill: none; stroke: currentColor; stroke-width: 1.35; stroke-linecap: round; stroke-linejoin: round; }
  .fullscreen-toggle svg { width: 19px; height: 19px; fill: none; stroke: currentColor; stroke-width: 1.5; stroke-linecap: round; stroke-linejoin: round; }
  @media (max-width: 1060px) { .shell { padding: 10px; } .masthead { flex-wrap: wrap; gap: 8px; } .nav-right { width: 100%; align-items: flex-start; margin-left: 0; } .nav-row { justify-content: flex-start; } }

  .language-toggle {
    width: 34px;
    height: 28px;
    padding: 0;
    font-size: 12px;
    font-weight: 600;
    color: var(--accent);
  }
  .language-toggle:hover { color: var(--text); }

  

  .boot-error {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    flex: 1;
    min-height: 220px;
    padding: 24px;
    text-align: center;
  }
  .boot-error strong { font-size: 18px; font-weight: 600; }
  .boot-error > span { color: var(--muted); }
  .boot-detail { max-width: 680px; user-select: text; }
  .boot-retry {
    height: 34px;
    margin-top: 4px;
    padding: 0 18px;
    border: 1px solid var(--button);
    border-radius: 7px;
    color: var(--button-text);
    background: var(--button);
    font-size: 14px;
    font-weight: 500;
    cursor: pointer;
  }
  .boot-retry:hover:not(:disabled) { border-color: var(--button-hover); background: var(--button-hover); }
  .boot-retry:disabled { opacity: .6; cursor: default; }

  .storage-error {
    padding: 10px;
    border: 1px solid var(--accent);
    color: var(--text);
    user-select: text;
  }

  .app-toast {
    position: fixed;
    left: 50%;
    bottom: 22px;
    transform: translateX(-50%);
    z-index: 80;
    max-width: 76vw;
    padding: 11px 16px;
    border: 1px solid var(--line-dark);
    border-radius: 9px;
    background: var(--surface);
    color: var(--text);
    font-size: 14px;
    box-shadow: 0 10px 30px rgba(0, 0, 0, .3);
    user-select: text;
  }

  .moon-icon { display: none; }
  :root[data-theme="light"] .sun-icon { display: none; }
  :root[data-theme="light"] .moon-icon { display: block; }
</style>
