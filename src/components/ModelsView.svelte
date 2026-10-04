<script lang="ts">
  import { privateCopy as p } from "../lib/private-i18n";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { getCopy } from "../lib/i18n-store.svelte";
  import { onMount, onDestroy } from "svelte";
  import { DEFAULT_STT, STT_OPTIONS } from "../lib/types";
  import type { SttModel } from "../lib/types";
  import whisperLogo from "../assets/logos/whisper.png";
  import {
    getSttModel,
    setSttModel,
  } from "../lib/models-store.svelte";

  let t = $derived(getCopy());

  interface Props {
    onClose: () => void;
    onDone: () => void;
    onSetupComplete: () => void;
    firstRun: boolean;
    onBusyChange?: (busy: boolean) => void;
  }

  let { onClose, onDone, onSetupComplete, firstRun, onBusyChange }: Props = $props();

  // Estado de ficheros: id de modelo -> ¿instalado?
  let installed = $state<Record<string, boolean>>({});
  let installingId = $state<string | null>(null);
  let progress = $state<Record<string, number>>({});
  let message = $state("");
  let menu = $state<"stt" | "privacy" | null>(null);
  let pendingDelete = $state<SttModel | "diarization" | "privacy" | null>(null);
  let privacyOptions = $state<{ id: string; name: string; size: string; installed: boolean }[]>([]);
  let privacySelected = $state("qwen3-fast");
  let nemotronAvailable = $state(false);
  let loaded = $state(false);
  let setupRunning = $state(false);
  let setupCancelled = false;
  let setupStatus = $state("");
  const setupKey = "belegal-local-demo-setup-pending";
  let setupPending = $state(localStorage.getItem(setupKey) === "true");
  const busy = $derived(setupRunning || installingId !== null);
  const offerSetup = $derived(loaded && (setupPending || !Object.values(installed).some(Boolean)));
  $effect(() => { onBusyChange?.(busy); });
  const nemotronId = "nemotron3-diarization";
  let unlistenProgress: (() => void) | null = null;

  const sttCurrent = $derived(
    STT_OPTIONS.find((o) => o.id === getSttModel()) ?? STT_OPTIONS[0]
  );
  const ready = $derived(STT_OPTIONS.some((o) => installed[o.id]));

  onMount(async () => {
    privacySelected = localStorage.getItem("belegal-privacy-model") || "qwen3-fast";
    // Si venía guardado un modelo que ya no ofrecemos (tiny/base), volver al recomendado.
    if (!STT_OPTIONS.some((o) => o.id === getSttModel())) setSttModel(sttCurrent.id);
    unlistenProgress = await listen<[string, number]>("model-download-progress", (event) => {
      const [label, pct] = event.payload;
      progress = { ...progress, [label]: pct };
    });
    try { await refresh(); } catch (e) { message = String(e); }
    window.addEventListener("pointerdown", onPointerDown);
    window.addEventListener("keydown", onKeyDown);
  });

  onDestroy(() => {
    if (unlistenProgress) {
      unlistenProgress();
      unlistenProgress = null;
    }
    window.removeEventListener("pointerdown", onPointerDown);
    window.removeEventListener("keydown", onKeyDown);
  });

  function onPointerDown(e: PointerEvent) {
    if (!menu) return;
    const target = e.target as Element | null;
    if (!target || !target.closest(".dd")) menu = null;
  }

  function onKeyDown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      menu = null;
      if (pendingDelete) pendingDelete = null;
    }
  }

  async function refresh() {
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
    if (!all.privacy.some(m => m.id === privacySelected)) privacySelected = all.privacy[0]?.id || "qwen3-fast";
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
    message = "";
    try { await downloadCurrent(family); } catch (e) { message = String(e); }
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
    message = "";
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
      } catch {
        throw new Error(es ? "No se pudo acceder al micrófono. Conecta uno y permite su uso en Ajustes del Sistema → Privacidad y seguridad → Micrófono; después vuelve a intentarlo." : "Microphone access failed. Connect a microphone and allow access in your system privacy settings, then retry.");
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
      message = nemotronAvailable
        ? (es ? "Todo listo: permisos preparados y los tres modelos descargados." : "Ready: permissions prepared and all three models downloaded.")
        : (es ? "Permisos preparados y los tres modelos descargados. La identificación de voces aún requiere configurar el motor NeMo-Speech.cpp." : "Permissions prepared and all three models downloaded. Speaker identification still requires the NeMo-Speech.cpp runtime.");
      onSetupComplete();
    } catch (e) {
      setupStatus = "";
      message = String(e);
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
    message = "";
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
      await refresh();
      message = t.deleted;
    } catch (e: any) {
      message = String(e);
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
    localStorage.setItem("belegal-privacy-model", id);
  }
</script>

<section class="models">
  <div class="models-head">
    <div class="head-top">
    <div class="head-copy">
      <strong>{t.modelsTitle}</strong>
      {#if setupStatus}
        <span role="status" aria-live="polite" title={setupStatus}>{setupStatus}</span>
      {:else if message}
        <span role="alert" title={message}>{message}</span>
      {:else}
        <span title={t.modelsSubtitle}>{t.modelsSubtitle}</span>
      {/if}
    </div>
    {#if offerSetup || setupRunning || installingId}
      <div class="head-actions">
        {#if offerSetup || setupRunning}
          <button class="download-btn setup-btn" onclick={setupAll} disabled={busy}>
            {setupRunning ? (t.langLabel === "ES" ? "Preparando…" : "Preparing…") : setupPending ? (t.langLabel === "ES" ? "Reanudar preparación" : "Resume setup") : (t.langLabel === "ES" ? "Descargar todos los modelos y aprobar permisos" : "Download all models and approve permissions")}
          </button>
        {/if}
        {#if installingId}
          <button class="change-btn" onclick={cancelDownload}>{t.langLabel === "ES" ? "Cancelar descarga" : "Cancel download"}</button>
        {/if}
      </div>
    {/if}
    </div>
  </div>

  <div class="list">
    <!-- Whisper -->
    <div class="card">
      <div class="card-head">
        <span class="card-icon"><img src={whisperLogo} alt="Whisper" /></span>
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
        <div class="dd">
          <button
            class="change-btn"
            onclick={() => (menu = menu === "stt" ? null : "stt")}
            disabled={busy}
          >
            <span>{t.changeModel}</span>
            <svg viewBox="0 0 20 20" aria-hidden="true"><path d="M5.5 8l4.5 4.5L14.5 8"></path></svg>
          </button>
          {#if menu === "stt"}
            <div class="dd-menu">
              {#each STT_OPTIONS as o, i (o.id)}
                {#if i > 0 && STT_OPTIONS[i - 1].family !== o.family}
                  <div class="dd-sep" role="separator"></div>
                {/if}
                <button
                  class="dd-item"
                  class:sel={o.id === sttCurrent.id}
                  onclick={() => pickStt(o.id)}
                >
                  <span class="dd-name">{o.family === "whisper" ? `Whisper ${o.label}` : o.label}</span>
                  <span class="dd-size">{o.size}</span>
                  {#if o.id === sttCurrent.id}
                    <span class="dd-check">✓</span>
                  {/if}
                </button>
              {/each}
            </div>
          {/if}
        </div>

        <div class="spacer"></div>

        <div class="right-actions">
          {#if installingId === sttCurrent.id}
            <div class="download-progress">
              <div class="progress-track"><div class="progress-fill" style={`width: ${progress[sttCurrent.id] ?? 0}%`}></div></div>
              <span>{progress[sttCurrent.id] ?? 0}%</span>
            </div>
          {:else if installed[sttCurrent.id]}
            <span class="ok">{t.downloaded}</span>
          {:else}
            <button class="download-btn" disabled={busy} onclick={() => install(sttCurrent.id)}>{t.download}</button>
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

    <!-- Qwen3: local document privacy -->
    <div class="card">
      <div class="card-head">
        <span class="card-icon qwen-icon" aria-hidden="true">Q</span>
        <div class="card-copy"><strong>Qwen3 · Private Legal</strong><span>{p("Extracción local de entidades privadas en documentos")}</span></div>
      </div>
      <div class="row">
        <div class="row-copy">
          <strong>{privacyOptions.find(m => m.id === privacySelected)?.name ?? "Qwen3-1.7B Q4_K_M"}</strong>
          <span class="row-size">{privacyOptions.find(m => m.id === privacySelected)?.size ?? "1.36 GB"}</span>
        </div>
        <div class="dd">
          <button class="change-btn" onclick={() => (menu = menu === "privacy" ? null : "privacy")} disabled={busy}>
            <span>{t.changeModel}</span><svg viewBox="0 0 20 20" aria-hidden="true"><path d="M5.5 8l4.5 4.5L14.5 8"></path></svg>
          </button>
          {#if menu === "privacy"}
            <div class="dd-menu">
              {#each privacyOptions as option (option.id)}
                <button class="dd-item" class:sel={option.id === privacySelected} onclick={() => pickPrivacy(option.id)}>
                  <span class="dd-name">{option.name}</span><span class="dd-size">{option.size}</span>{#if option.id === privacySelected}<span class="dd-check">✓</span>{/if}
                </button>
              {/each}
            </div>
          {/if}
        </div>
        <div class="spacer"></div>
        <div class="right-actions">
          {#if installingId === privacySelected}
            <div class="download-progress"><div class="progress-track"><div class="progress-fill" style={`width: ${progress[privacySelected] ?? 0}%`}></div></div><span>{progress[privacySelected] ?? 0}%</span></div>
          {:else if installed[privacySelected]}
            <span class="ok">{t.downloaded}</span>
          {:else if privacySelected === "belegal-pii-v1"}
            <span>{p("Restaura la copia local del modelo entrenado.")}</span>
          {:else}
            <button class="download-btn" disabled={busy} onclick={() => install("privacy")}>{t.download}</button>
          {/if}
          <button class="icon-btn" title={t.delete} aria-label={t.delete} disabled={!installed[privacySelected] || busy} onclick={() => remove("privacy") }>
            <svg viewBox="0 0 20 20" aria-hidden="true"><path d="M4.5 6h11"></path><path d="M6.5 6v10a1 1 0 0 0 1 1h5a1 1 0 0 0 1-1V6"></path><path d="M8 6V4.5a1 1 0 0 1 1-1h2a1 1 0 0 1 1 1V6"></path><path d="M10 9v5"></path></svg>
          </button>
        </div>
      </div>
    </div>

    <!-- Nemotron 3: local diarization trial -->
    <div class="card">
      <div class="card-head">
        <span class="card-icon nemotron-icon" aria-hidden="true">N</span>
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
            <span class="ok">{t.downloaded}</span>
          {:else}
            <button class="download-btn" disabled={busy} onclick={() => install("diarization")}>{t.download}</button>
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
  </div>

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
  .download-btn.setup-btn { flex: none; max-width: 320px; height: auto; min-height: 38px; padding: 10px 16px; line-height: 1.4; text-align: center; white-space: normal; }
  .back-btn:disabled { opacity: .55; cursor: default; }
  .head-copy { display: flex; flex-direction: column; gap: 8px; }
  .head-copy strong { font-family: var(--font-heading); font-size: 28px; font-weight: 400; line-height: 1.2; }
  .head-copy span { color: var(--muted); font-size: 14px; line-height: 1.5; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; user-select: text; }

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
  .card-icon {
    flex: none;
    width: 34px;
    height: 34px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    overflow: hidden;
    border: 1px solid var(--line-dark);
    background: var(--hover);
  }
  .card-icon img {
    width: 82%;
    height: 82%;
    object-fit: contain;
    display: block;
    user-select: none;
    -webkit-user-drag: none;
  }
  .nemotron-icon { background: var(--panel); border-color: var(--line-dark); color: var(--text); font-weight: 800; font-size: 19px; }
  .qwen-icon { background: var(--panel); color: var(--text); font-family: var(--font-heading); font-size: 21px; }
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

  /* Dropdown cambiar modelo */
  .dd { position: relative; }
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
  .change-btn svg { width: 12px; height: 12px; fill: none; stroke: currentColor; stroke-width: 1.6; stroke-linecap: round; stroke-linejoin: round; }

  .dd-menu {
    position: absolute;
    top: calc(100% + 7px);
    right: 0;
    z-index: 30;
    min-width: 250px;
    padding: 5px;
    border: 1px solid var(--line);
    border-radius: 9px;
    background: var(--surface);
    box-shadow: 0 8px 24px rgba(0, 0, 0, .25);
    display: flex;
    flex-direction: column;
  }
  .dd-sep {
    height: 1px;
    margin: 5px 8px;
    background: var(--line);
  }
  .dd-item {
    display: grid;
    grid-template-columns: 1fr auto auto;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--text);
    text-align: left;
    font-size: 14px;
    cursor: pointer;
  }
  .dd-item:hover { background: var(--hover); }
  .dd-item.sel { color: var(--accent); }
  .dd-name { font-weight: 500; white-space: nowrap; }
  .dd-size { color: var(--muted); font-size: 12px; white-space: nowrap; }
  .dd-check { color: var(--accent); font-size: 13px; }

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
  .icon-btn:hover:not(:disabled) { background: var(--error); color: #fff; }
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
  .dialog-actions .danger:hover { background: var(--error); color: #fff; }
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
    .row-copy, .dd, .right-actions { width:100%; }
    .right-actions { justify-content:space-between; }
    .dd-menu { left:0; right:auto; max-width:calc(100vw - 60px); }
    .footer { flex-wrap:wrap; }
    .footer-note { flex:1 1 100%; text-align:left; }
  }
</style>
