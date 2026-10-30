<script lang="ts">
  import { privacyModel, MOBILE_PRIVACY_MODEL } from "../lib/privacy-model";
  import Feedback from "./Feedback.svelte";
  import { microphoneError } from "../lib/user-errors";
  import Icon from "./Icon.svelte";
  import { privateCopy as p } from "../lib/private-i18n";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { getCopy } from "../lib/i18n-store.svelte";
  import { onMount, onDestroy, untrack } from "svelte";
  import { DEFAULT_STT, STT_OPTIONS } from "../lib/types";
  import type { SttModel } from "../lib/types";
  import ModelIcon from "./ModelIcon.svelte";
  import {
    getSttModel,
    setSttModel,
  } from "../lib/models-store.svelte";

  let t = $derived(getCopy());

  interface Props {
    mobile?: boolean;
    onClose: () => void;
    onDone: () => void;
    onSetupComplete: () => void;
    firstRun: boolean;
    onBusyChange?: (busy: boolean) => void;
  }

  let { mobile = false, onClose, onDone, onSetupComplete, firstRun, onBusyChange }: Props = $props();

  // Estado de ficheros: id de modelo -> ¿instalado?
  let installed = $state<Record<string, boolean>>({});
  let installingId = $state<string | null>(null);
  let progress = $state<Record<string, number>>({});
  let message = $state("");
  let messageKind = $state<"error" | "status">("error");
  let menu = $state<"stt" | "privacy" | null>(null);
  let modelPicker = $state<HTMLDialogElement>();
  let pickerReturnFocus: HTMLButtonElement | undefined;
  function openPicker(kind: "stt" | "privacy", trigger: HTMLButtonElement) {
    pickerReturnFocus = trigger;
    menu = kind;
  }
  $effect(() => {
    if (!modelPicker) return;
    if (menu && !modelPicker.open) modelPicker.showModal();
    else if (!menu && modelPicker.open) {
      modelPicker.close();
      pickerReturnFocus?.focus();
    }
  });
  let pendingDelete = $state<SttModel | "diarization" | "privacy" | null>(null);
  let privacyOptions = $state<{ id: string; name: string; size: string; installed: boolean }[]>([]);
  let privacySelected = $state(untrack(() => privacyModel(mobile)));
  let nemotronAvailable = $state(false);
  let loaded = $state(false);
  let setupRunning = $state(false);
  let setupCancelled = false;
  let setupStatus = $state("");
  const setupKey = "bluedoor-setup-pending";
  let setupPending = $state(localStorage.getItem(setupKey) === "true");
  const busy = $derived(setupRunning || installingId !== null);
  const offerSetup = $derived(!mobile && loaded && (setupPending || !Object.values(installed).some(Boolean)));
  $effect(() => { onBusyChange?.(busy); });
  const nemotronId = "nemotron3-diarization";
  let unlistenProgress: (() => void) | null = null;

  const sttCurrent = $derived(
    STT_OPTIONS.find((o) => o.id === getSttModel()) ?? STT_OPTIONS[0]
  );
  const ready = $derived(STT_OPTIONS.some((o) => installed[o.id]));

  onMount(async () => {
    privacySelected = privacyModel(mobile);
    // Si venía guardado un modelo que ya no ofrecemos (tiny/base), volver al recomendado.
    if (!STT_OPTIONS.some((o) => o.id === getSttModel())) setSttModel(sttCurrent.id);
    unlistenProgress = await listen<[string, number]>("model-download-progress", (event) => {
      const [label, pct] = event.payload;
      const id = label === "gliner-multi-pii-tokenizer" ? MOBILE_PRIVACY_MODEL : label === "gliner-pii-tokenizer" ? "gliner-pii-small-int8" : label;
      const total = label === "gliner-multi-pii-tokenizer" ? 95 + Math.round(pct * .05) : label === MOBILE_PRIVACY_MODEL ? Math.round(pct * .95) : label === "gliner-pii-tokenizer" ? 96 + Math.round(pct * .04) : label === "gliner-pii-small-int8" ? Math.round(pct * .96) : pct;
      progress = { ...progress, [id]: total };
    });
    try { await refresh(); } catch (e) { messageKind = "error"; message = String(e); }
    window.addEventListener("keydown", onKeyDown);
  });

  onDestroy(() => {
    if (unlistenProgress) {
      unlistenProgress();
      unlistenProgress = null;
    }
    window.removeEventListener("keydown", onKeyDown);
  });

  function onKeyDown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      menu = null;
      if (pendingDelete) pendingDelete = null;
    }
  }

  async function refresh(allowFallback = false) {
    const all = await invoke<{
      whisper: { id: string; installed: boolean }[];
      parakeet: { id: string; installed: boolean }[];
      diarization: { id: string; installed: boolean }[];
      privacy: { id: string; name: string; size: string; installed: boolean }[];
    }>("list_models");
    const map: Record<string, boolean> = {};
    for (const m of all.whisper) map[m.id] = m.installed;
    for (const m of all.parakeet) map[m.id] = m.installed;
    for (const m of all.diarization) map[m.id] = m.installed;
    for (const m of all.privacy) map[m.id] = m.installed;
    privacyOptions = all.privacy;
    if (!all.privacy.some(m => m.id === privacySelected && m.installed) && all.privacy.some(m => m.installed) && (allowFallback || privacySelected !== MOBILE_PRIVACY_MODEL || !all.privacy.some(m => m.id === privacySelected))) {
      privacySelected = all.privacy.find(m => m.installed)!.id;
      localStorage.setItem("bluedoor-privacy-model", privacySelected);
    }
    installed = map;
    loaded = true;
    nemotronAvailable = await invoke<boolean>("diarization_available").catch(() => false);
  }

  async function cancelDownload() {
    if (setupRunning) setupCancelled = true;
    await invoke("cancel_model_download");
  }

  async function install(family: SttModel | "diarization" | "privacy") {
    if (busy) return;
    message = ""; messageKind = "error";
    try { await downloadCurrent(family); } catch (e) { messageKind = "error"; message = String(e); }
  }

  async function downloadCurrent(family: SttModel | "diarization" | "privacy") {
    const current = family === "diarization" ? { id: nemotronId } : family === "privacy" ? { id: privacySelected } : sttCurrent;
    if (installed[current.id]) return;
    installingId = current.id;
    progress = { ...progress, [current.id]: 0 };
    try {
      if (family === "privacy") {
        await invoke<string>("download_privacy_model", { id: current.id });
      } else if (family === "diarization") {
        await invoke<string>("download_diarization_model");
      } else if (sttCurrent.family === "parakeet") {
        await invoke<string>("download_parakeet_model", { id: current.id });
      } else {
        await invoke<string>("download_model", { size: current.id });
      }
      await refresh();
    } finally {
      installingId = null;
    }
  }

  async function setupAll() {
    if (busy || !loaded) return;
    setupRunning = true;
    setupCancelled = false;
    setupPending = true;
    localStorage.setItem(setupKey, "true");
    message = ""; messageKind = "error";
    menu = null;
    setSttModel(DEFAULT_STT);
    pickPrivacy("qwen3-fast");
    const es = t.langLabel === "ES";
    setupStatus = es ? "1/2 · Aprueba el micrófono y el acceso a carpetas en las ventanas del sistema." : "1/2 · Allow microphone and folder access in the system prompts.";
    try {
      // Request from this click, using the same WebView path as Recorder.
      // Stop immediately: setup does not record or retain any audio.
      try {
        const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
        stream.getTracks().forEach(track => track.stop());
      } catch (error) {
        throw new Error(microphoneError(error, es ? "es" : "en"));
      }
      await invoke("prepare_local_setup");
      await refresh();
      const queue = [
        { family: DEFAULT_STT, name: "Parakeet TDT v3" },
        { family: "privacy" as const, name: "Qwen3-1.7B Q4_K_M" },
        { family: "diarization" as const, name: "Nemotron 3 Diarization" },
      ];
      for (const [index, model] of queue.entries()) {
        if (setupCancelled) throw new Error(es ? "Preparación cancelada. Puedes reanudarla sin descargar de nuevo los modelos completados." : "Setup cancelled. Resume without downloading completed models again.");
        setupStatus = `${es ? "2/2 · Descargando" : "2/2 · Downloading"} ${index + 1}/3 · ${model.name}`;
        await downloadCurrent(model.family);
      }
      if (setupCancelled) throw new Error(es ? "Preparación cancelada." : "Setup cancelled.");
      setupPending = false;
      localStorage.removeItem(setupKey);
      setupStatus = "";
      messageKind = "status";
      message = nemotronAvailable
        ? (es ? "Todo listo: permisos preparados y los tres modelos descargados." : "Ready: permissions prepared and all three models downloaded.")
        : (es ? "Permisos preparados y los tres modelos descargados. La identificación de voces aún requiere configurar el motor NeMo-Speech.cpp." : "Permissions prepared and all three models downloaded. Speaker identification still requires the NeMo-Speech.cpp runtime.");
      onSetupComplete();
    } catch (e) {
      setupStatus = "";
      messageKind = "error"; message = String(e);
    } finally {
      setupRunning = false;
    }
  }

  function remove(family: SttModel | "diarization" | "privacy") {
    if (busy) return;
    const current = family === "diarization" ? { id: nemotronId } : family === "privacy" ? { id: privacySelected } : sttCurrent;
    if (!installed[current.id]) return;
    pendingDelete = family;
  }

  async function confirmRemove() {
    const family = pendingDelete;
    pendingDelete = null;
    if (!family || busy) return;
    message = ""; messageKind = "error";
    try {
      if (family === "privacy") {
        await invoke("delete_privacy_model", { id: privacySelected });
      } else if (family === "diarization") {
        await invoke("delete_diarization_model");
      } else if (sttCurrent.family === "parakeet") {
        await invoke("delete_parakeet_model", { id: family });
      } else {
        await invoke("delete_model", { size: family });
      }
      await refresh(true);
      messageKind = "status"; message = t.deleted;
    } catch (e: any) {
      messageKind = "error"; message = String(e);
    }
  }

  function cancelDelete() {
    pendingDelete = null;
  }

  function pickStt(id: string) {
    menu = null;
    setSttModel(id as SttModel);
  }
  function pickPrivacy(id: string) {
    menu = null;
    privacySelected = id;
    localStorage.setItem("bluedoor-privacy-model", id);
  }
</script>

<section class="models">
  <Feedback message={setupStatus || message} kind={setupStatus ? "status" : messageKind} onDismiss={setupStatus ? undefined : () => message = ""} />
  <div class="models-head">
    <div class="head-top">
    <div class="head-copy">
      <Icon name="settings" size={36} /><h1>{t.langLabel === "ES" ? "Modelos locales:" : "Local models:"}</h1>
    </div>
    {#if offerSetup || setupRunning || installingId}
      <div class="head-actions">
        {#if offerSetup || setupRunning}
          <button class="download-btn setup-btn" style:--setup-width={t.langLabel === "ES" ? "264px" : "248px"} onclick={setupAll} disabled={busy} aria-label={setupRunning ? (t.langLabel === "ES" ? "Preparando…" : "Preparing…") : setupPending ? (t.langLabel === "ES" ? "Reanudar preparación" : "Resume setup") : (t.langLabel === "ES" ? "Descargar todos los modelos y aprobar todos los permisos" : "Download all models and approve all permissions")}>
            {#if setupRunning}{t.langLabel === "ES" ? "Preparando…" : "Preparing…"}
            {:else if setupPending}{t.langLabel === "ES" ? "Reanudar preparación" : "Resume setup"}
            {:else}
              <Icon name="download" size={24} />
              <span class="setup-copy">
                <span>{t.langLabel === "ES" ? "Descargar todos los modelos" : "Download all models"}</span>
                <span>{t.langLabel === "ES" ? "Aprobar todos los permisos" : "Approve all permissions"}</span>
              </span>
            {/if}
          </button>
        {/if}
        {#if installingId}
          <button class="change-btn cancel-download-btn" onclick={cancelDownload}>{t.langLabel === "ES" ? "Cancelar descarga" : "Cancel download"}</button>
        {/if}
      </div>
    {/if}
    </div>
  </div>

  <div class="list">
    {#if !mobile}
    <!-- Whisper -->
    <div class="card">
      <div class="card-head">
        <ModelIcon kind="speech" />
        <div class="card-copy">
          <strong>{t.setupWhisperTitle}</strong>
          <span>{t.setupWhisperLink}</span>
        </div>
      </div>
      <div class="row">
        <div class="row-copy">
          <strong>{sttCurrent.family === "whisper" ? `Whisper ${sttCurrent.label}` : sttCurrent.label}</strong>
          <span class="row-size">{sttCurrent.size}</span>
        </div>
        <button class="change-btn model-picker-trigger" aria-label={t.changeModel} title={t.changeModel} onclick={(event) => openPicker("stt", event.currentTarget)} disabled={busy}>
          <Icon name="edit" size={16} /><span>{t.changeModel}</span>
        </button>

        <div class="spacer"></div>

        <div class="right-actions">
          {#if installingId === sttCurrent.id}
            <div class="download-progress">
              <div class="progress-track"><div class="progress-fill" style={`width: ${progress[sttCurrent.id] ?? 0}%`}></div></div>
              <span>{progress[sttCurrent.id] ?? 0}%</span>
            </div>
          {:else if installed[sttCurrent.id]}
            <span class="ok">{mobile ? (t.langLabel === "ES" ? "Instalado" : "Installed") : t.downloaded}</span>
          {:else}
            <button class="download-btn" disabled={busy} onclick={() => install(sttCurrent.id)}><Icon name="download" size={20} /><span>{t.download}</span></button>
          {/if}

          <button
            class="icon-btn"
            title={t.delete}
            aria-label={t.delete}
            disabled={!installed[sttCurrent.id] || busy}
            onclick={() => remove(sttCurrent.id)}
          >
            <svg viewBox="0 0 20 20" aria-hidden="true">
              <path d="M4.5 6h11"></path>
              <path d="M6.5 6v10a1 1 0 0 0 1 1h5a1 1 0 0 0 1-1V6"></path>
              <path d="M8 6V4.5a1 1 0 0 1 1-1h2a1 1 0 0 1 1 1V6"></path>
              <path d="M10 9v5"></path>
            </svg>
          </button>
        </div>
      </div>
    </div>

    {/if}
    <!-- Qwen3: local document privacy -->
    <div class="card">
      <div class="card-head">
        <ModelIcon kind="privacy" />
        <div class="card-copy"><strong>{t.langLabel === "ES" ? "Privacidad de documentos" : "Document privacy"}</strong><span>{p("Extracción local de entidades privadas en documentos")}</span></div>
      </div>
      <div class="row privacy-model-row">
        <div class="row-copy">
          <strong title={privacyOptions.find(m => m.id === privacySelected)?.name}>{privacyOptions.find(m => m.id === privacySelected)?.name ?? "Qwen3-1.7B Q4_K_M"}</strong>
          <span class="row-size">{privacyOptions.find(m => m.id === privacySelected)?.size ?? "1.36 GB"}</span>
        </div>
        <button class="change-btn model-picker-trigger" aria-label={t.changeModel} title={t.changeModel} onclick={(event) => openPicker("privacy", event.currentTarget)} disabled={busy}>
          <Icon name="edit" size={16} /><span>{t.changeModel}</span>
        </button>
          <button class="icon-btn privacy-model-delete" title={t.delete} aria-label={t.delete} disabled={!installed[privacySelected] || busy} onclick={() => remove("privacy") }>
            <svg viewBox="0 0 20 20" aria-hidden="true"><path d="M4.5 6h11"></path><path d="M6.5 6v10a1 1 0 0 0 1 1h5a1 1 0 0 0 1-1V6"></path><path d="M8 6V4.5a1 1 0 0 1 1-1h2a1 1 0 0 1 1 1V6"></path><path d="M10 9v5"></path></svg>
          </button>
        <div class="spacer"></div>
        <div class="right-actions privacy-model-status">
          {#if mobile && installingId !== privacySelected}<span class="mobile-model-size">{privacySelected === MOBILE_PRIVACY_MODEL ? "596,0 MB total" : privacySelected === "gliner-pii-small-int8" ? "86,3 MB total" : privacyOptions.find(m => m.id === privacySelected)?.size}</span>{/if}
          {#if installingId === privacySelected}
            <div class="download-progress"><div class="progress-track"><div class="progress-fill" style={`width: ${progress[privacySelected] ?? 0}%`}></div></div><span>{progress[privacySelected] ?? 0}%</span></div>
          {:else if installed[privacySelected]}
            <span class="ok">{mobile ? (t.langLabel === "ES" ? "Instalado" : "Installed") : t.downloaded}</span>
          {:else if privacySelected === "belegal-pii-v1"}
            <span>{p("Restaura la copia local del modelo entrenado.")}</span>
          {:else}
            <button class="download-btn" disabled={busy} onclick={() => install("privacy")}><Icon name="download" size={20} /><span>{t.download}</span></button>
          {/if}

        </div>
      </div>
    </div>

    {#if !mobile}
    <!-- Nemotron 3: local diarization trial -->
    <div class="card">
      <div class="card-head">
        <ModelIcon kind="speakers" />
        <div class="card-copy">
          <strong>NVIDIA Nemotron 3</strong>
          <span>{t.langLabel === "ES" ? "Identificación de voces en el audio" : "Speaker identification in audio"}</span>
        </div>
      </div>
      <div class="row">
        <div class="row-copy">
          <strong>Nemotron 3 Diarization</strong>
          <span class="row-size">GGUF Q8 · 102 MB</span>
        </div>
        <div class="spacer"></div>
        <div class="right-actions">
          {#if installingId === nemotronId}
            <div class="download-progress">
              <div class="progress-track"><div class="progress-fill" style={`width: ${progress[nemotronId] ?? 0}%`}></div></div>
              <span>{progress[nemotronId] ?? 0}%</span>
            </div>
          {:else if installed[nemotronId]}
            <span class="ok">{mobile ? (t.langLabel === "ES" ? "Instalado" : "Installed") : t.downloaded}</span>
          {:else}
            <button class="download-btn" disabled={busy} onclick={() => install("diarization")}><Icon name="download" size={20} /><span>{t.download}</span></button>
          {/if}
          <button
            class="icon-btn"
            title={t.delete}
            aria-label={t.delete}
            disabled={!installed[nemotronId] || busy}
            onclick={() => remove("diarization")}
          >
            <svg viewBox="0 0 20 20" aria-hidden="true">
              <path d="M4.5 6h11"></path>
              <path d="M6.5 6v10a1 1 0 0 0 1 1h5a1 1 0 0 0 1-1V6"></path>
              <path d="M8 6V4.5a1 1 0 0 1 1-1h2a1 1 0 0 1 1 1h2V6"></path>
              <path d="M10 9v5"></path>
            </svg>
          </button>
        </div>
      </div>
      {#if installed[nemotronId] && !nemotronAvailable}
        <p class="model-note">{t.langLabel === "ES" ? "Modelo descargado. Falta configurar el motor NeMo-Speech.cpp en esta prueba local." : "Model downloaded. Configure the NeMo-Speech.cpp runtime for this local trial."}</p>
      {/if}
    </div>
    {/if}
  </div>

  <dialog class="model-picker" bind:this={modelPicker} aria-labelledby="model-picker-title" oncancel={(event) => { event.preventDefault(); menu = null; }}>
    <div class="picker-heading">
      <div><h2 id="model-picker-title">{t.langLabel === "ES" ? "Elegir modelo" : "Choose a model"}</h2><p>{menu === "stt" ? t.setupWhisperTitle : (t.langLabel === "ES" ? "Privacidad de documentos" : "Document privacy")}</p></div>
      <button class="picker-close" aria-label={t.langLabel === "ES" ? "Cerrar selector" : "Close selector"} onclick={() => menu = null}><Icon name="close" /></button>
    </div>
    <div class="model-choices" role="group" aria-label={t.langLabel === "ES" ? "Modelos disponibles" : "Available models"}>
      {#if menu === "stt"}
        {#each STT_OPTIONS as option (option.id)}
          <button class="model-choice" class:selected={option.id === sttCurrent.id} aria-pressed={option.id === sttCurrent.id} disabled={busy} onclick={() => pickStt(option.id)}>
            <span class="choice-copy"><strong>{option.family === "whisper" ? `Whisper ${option.label}` : option.label}</strong><span>{option.size} · {option.family === "whisper" ? "Whisper" : "NVIDIA Parakeet"}</span></span>
            {#if option.id === sttCurrent.id}<Icon name="check" size={20} />{/if}
          </button>
        {/each}
      {:else}
        {#each privacyOptions as option (option.id)}
          <button class="model-choice" class:selected={option.id === privacySelected} aria-pressed={option.id === privacySelected} disabled={busy} onclick={() => pickPrivacy(option.id)}>
            <span class="choice-copy"><strong>{option.name}</strong><span>{option.size}</span></span>
            {#if option.id === privacySelected}<Icon name="check" size={20} />{/if}
          </button>
        {/each}
      {/if}
    </div>
  </dialog>

  {#if firstRun}
    <div class="footer">
      <p class="footer-note" role="status">{ready ? t.continueReadyHint : t.continueDisabledHint}</p>
      <button class="continue" onclick={onDone} disabled={!ready || busy}>
        <span>{t.continue}</span>
        <svg viewBox="0 0 20 20" aria-hidden="true"><path d="M4 10h11m-4-4 4 4-4 4" /></svg>
      </button>
    </div>
  {:else}
    <div class="footer">
      <button class="back-btn" onclick={onClose} disabled={busy}>&larr; {t.back}</button>
    </div>
  {/if}

  {#if pendingDelete}
    <div class="overlay" role="dialog" aria-modal="true" tabindex="-1" onclick={cancelDelete} onkeydown={(e) => { if (e.key === "Escape") cancelDelete(); }}>
      <div class="dialog" role="none" onclick={(e) => e.stopPropagation()}>
        <strong>{t.deleteTitle}</strong>
        <p>{pendingDelete === "privacy" ? p("El análisis de documentos quedará deshabilitado hasta reinstalar este peso.") : pendingDelete === "diarization" ? (t.langLabel === "ES" ? "La detección de voces dejará de estar disponible hasta volver a instalar el modelo." : "Speaker detection will be unavailable until the model is installed again.") : t.deleteWhisperBody}</p>
        <p class="dialog-model">{pendingDelete === "privacy" ? (privacyOptions.find(m => m.id === privacySelected)?.name ?? "Qwen3") : pendingDelete === "diarization" ? "NVIDIA Nemotron 3" : sttCurrent.family === "whisper" ? `Whisper ${sttCurrent.label}` : sttCurrent.label}</p>
        <div class="dialog-actions">
          <button class="cancel" onclick={cancelDelete}>{t.cancel}</button>
          <button class="danger" onclick={confirmRemove}>{t.deleteConfirm}</button>
        </div>
      </div>
    </div>
  {/if}
</section>

<style>
  .models {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 20px;
  }

  .models-head { display: flex; flex-direction: column; gap: 7px; padding: 24px 30px 4px; }
  .head-top { display: flex; align-items: center; justify-content: space-between; gap: 24px; }
  .head-copy { flex: 1; min-width: 0; }
  .head-actions { flex: none; display: flex; align-items: center; justify-content: flex-end; gap: 10px; min-height: 60px; }
  .download-btn.setup-btn { display:flex; flex-direction:row; align-items:center; justify-content:center; gap:12px; flex: none; width:var(--setup-width,248px); max-width:320px; height:auto; min-height:66px; padding: 10px 16px; line-height: 1.4; text-align: center; white-space: normal; }
  .setup-copy { display:flex; flex-direction:column; gap:2px; text-align:center; font-weight:700; flex:none; white-space:nowrap; }
  .back-btn:disabled { opacity: .55; cursor: default; }
  .head-copy { display:flex; flex-direction:row; align-items:center; gap:14px; }
  .head-copy h1 { margin:0; font-family:var(--font-heading); font-size:20px; font-weight:650; line-height:1.2; white-space:nowrap; }

  .list { flex: 1; min-height: 0; display: flex; flex-direction: column; gap: 14px; overflow-y: auto; }

  .card {
    padding: 16px 18px;
    border: 1px solid var(--line);
    border-radius: 12px;
    background: var(--surface);
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .card-head { display: flex; align-items: center; gap: 13px; min-width:0; }
  .model-note { color: var(--muted); font-size: 12px; margin: -4px 2px 0; }
  .card-copy { display: flex; flex-direction: column; gap: 3px; min-width: 0; }
  .card-copy strong { font-size: 15px; font-weight: 500; }
  .card-copy span { color: var(--muted); font-size: 13px; line-height: 1.45; }

  .row {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 12px 16px;
    border: 1px solid var(--line-dark);
    border-radius: 10px;
    background: var(--surface);
  }

  .row-copy { flex: none; width: clamp(160px, 20vw, 230px); min-width: 0; display: flex; flex-direction: column; gap: 3px; }
  .row-copy strong { font-size: 15px; font-weight: 500; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .row-size { color: var(--muted); font-size: 13px; }

  .spacer { flex: 1; }
  .right-actions { flex: none; display: flex; align-items: center; gap: 20px; }

  .change-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    height: 38px;
    padding: 0 13px;
    border: 1px solid var(--line-dark);
    border-radius: 7px;
    color: var(--text);
    background: var(--surface);
    font-size: 13.5px;
    font-weight: 500;
    cursor: pointer;
    white-space: nowrap;
  }
  .change-btn:hover:not(:disabled) { background: var(--hover); border-color: var(--muted); }
  .change-btn:disabled { opacity: .55; cursor: default; }
  .change-btn :global(svg) { width: 12px; height: 12px; fill: none; stroke: currentColor; stroke-width: 1.6; stroke-linecap: round; stroke-linejoin: round; }

  .ok { font-size: 14px; font-weight: 500; white-space: nowrap; color: var(--success); }

  .download-btn {
    height: 38px;
    padding: 0 19px;
    border: 1px solid var(--button);
    border-radius: 7px;
    color: var(--button-text);
    background: var(--button);
    font-size: 13.5px;
    font-weight: 500;
    cursor: pointer;
    white-space: nowrap;
  }
  .download-btn:disabled { opacity: .55; cursor: default; }
  .download-btn:hover:not(:disabled) { border-color: var(--button-hover); background: var(--button-hover); }

  .download-progress { display: flex; align-items: center; gap: 10px; width: 160px; }
  .progress-track { flex: 1; height: 7px; border-radius: 4px; background: var(--line-dark); overflow: hidden; }
  .progress-fill { height: 100%; border-radius: 4px; background: var(--accent); transition: width .15s ease; }
  .download-progress span { color: var(--muted); font-size: 13px; font-variant-numeric: tabular-nums; }

  .icon-btn {
    width: 36px;
    height: 36px;
    display: grid;
    place-items: center;
    border: 1px solid var(--error);
    border-radius: 7px;
    background: transparent;
    color: var(--error);
    cursor: pointer;
  }
  .icon-btn svg { width: 17px; height: 17px; fill: none; stroke: currentColor; stroke-width: 1.35; stroke-linecap: round; stroke-linejoin: round; }
  .icon-btn:hover:not(:disabled) { background: var(--error-background); color: #fff; }
  .icon-btn:disabled { opacity: .3; cursor: default; }

  .footer { flex: none; display: flex; align-items: center; justify-content: flex-end; gap: 18px; padding-top: 4px; }
  .footer-note { margin: 0; color: var(--muted); font-size: 13px; line-height: 1.45; text-align: right; }
  .continue {
    flex: none;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 14px;
    min-width: 150px;
    height: 46px;
    padding: 0 20px;
    border: 1px solid var(--button);
    border-radius: 8px;
    color: var(--button-text);
    background: var(--button);
    font-size: 14px;
    font-weight: 600;
    line-height: 1;
    cursor: pointer;
    transition: background .18s ease, border-color .18s ease, transform .18s ease;
  }
  .continue svg { width: 16px; height: 16px; fill: none; stroke: currentColor; stroke-width: 1.7; stroke-linecap: round; stroke-linejoin: round; }
  .continue:hover:not(:disabled) { border-color: var(--button-hover); background: var(--button-hover); transform: translateY(-1px); }
  .continue:focus-visible { outline: 2px solid var(--accent); outline-offset: 3px; }
  .continue:disabled { border-color: var(--line-dark); color: var(--muted); background: var(--panel); cursor: default; }

  .back-btn {
    height: 44px;
    padding: 0 21px;
    border: 1px solid var(--line-dark);
    border-radius: 8px;
    color: var(--text);
    background: var(--surface);
    font-size: 14px;
    cursor: pointer;
  }
  .back-btn:hover { background: var(--hover); }

  /* Diálogo de confirmación Panto */
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 60;
    display: grid;
    place-items: center;
    background: rgba(0, 0, 0, .45);
  }
  .dialog {
    width: 290px;
    padding: 18px;
    border: 1px solid var(--line);
    border-radius: 12px;
    background: var(--surface);
    display: flex;
    flex-direction: column;
    gap: 8px;
    box-shadow: 0 12px 36px rgba(0, 0, 0, .35);
  }
  .dialog strong { font-size: 15px; font-weight: 600; }
  .dialog p { margin: 0; color: var(--muted); font-size: 13.5px; line-height: 1.5; user-select: text; }
  .dialog .dialog-model { color: var(--text); font-weight: 500; }
  .dialog-actions { display: flex; justify-content: flex-end; gap: 9px; margin-top: 10px; }
  .dialog-actions button {
    height: 36px;
    padding: 0 17px;
    border-radius: 7px;
    border: 1px solid var(--line-dark);
    background: var(--surface);
    color: var(--text);
    font-size: 13.5px;
    font-weight: 500;
    cursor: pointer;
  }
  .dialog-actions .cancel:hover { background: var(--hover); }
  .dialog-actions .danger { border-color: var(--error); color: var(--error); }
  .dialog-actions .danger:hover { background: var(--error-background); color: #fff; }
  .model-picker { width:min(760px,calc(100vw - 48px)); max-height:calc(100vh - 64px); padding:26px; border:1px solid #a7caff66; border-radius:18px; background:#1745a5; color:white; box-shadow:0 28px 90px #00194766; }
  .model-picker::backdrop { background:#051d54a6; backdrop-filter:blur(5px); }
  .picker-heading { display:flex; align-items:center; justify-content:space-between; gap:20px; margin-bottom:22px; }
  .picker-heading h2 { font:650 23px/1.3 var(--font-heading); margin:0 0 5px; }
  .picker-heading p { margin:0; color:#c5dbff; font-size:14px; }
  .picker-close { display:grid; place-items:center; width:36px; height:36px; border:1px solid #c5dbff55; border-radius:9px; background:transparent; color:white; cursor:pointer; }
  .model-choices { display:grid; grid-template-columns:repeat(2,minmax(0,1fr)); gap:12px; }
  .model-choice { display:flex; justify-content:space-between; align-items:center; gap:12px; padding:17px; min-height:78px; border:1px solid #b8d4ff55; border-radius:12px; background:#ffffff08; color:white; text-align:left; cursor:pointer; }
  .model-choice:hover { background:#ffffff16; }
  .model-choice.selected { border-color:white; background:#ffffff1f; }
  .choice-copy { display:flex; flex-direction:column; gap:5px; min-width:0; }
  .choice-copy strong { font-size:14px; font-weight:650; }
  .choice-copy > span { font-size:12px; color:#c5dbff; }
  @media (max-width:600px) { .model-choices { grid-template-columns:1fr; } }
  @media (max-width: 900px) {
    .head-top { flex-wrap: wrap; gap: 14px; }
    .card { padding:14px; }
    .row { flex-wrap:wrap; gap:12px; }
    .row-copy { flex:1 1 190px; }
    .right-actions { margin-left:auto; }
    .spacer { display:none; }
  }
  @media (max-width: 540px) {
    .row { align-items:stretch; }
    .row-copy, .model-picker-trigger, .right-actions { width:100%; }
    .right-actions { justify-content:space-between; }
    .footer { flex-wrap:wrap; }
    .footer-note { flex:1 1 100%; text-align:left; }
  }
</style>
