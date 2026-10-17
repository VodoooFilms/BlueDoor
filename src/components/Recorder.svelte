<script lang="ts">
  import { PcmRecorder } from "../lib/pcm-recorder";
  import type { Transcript, RecordingState } from "../lib/types";
  import { STT_OPTIONS, sttFamily } from "../lib/types";
  import { getSttModel } from "../lib/models-store.svelte";
  import { generateId, formatDuration } from "../lib/utils";
  import { getCopy } from "../lib/i18n-store.svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onDestroy, onMount } from "svelte";

  let t = $derived(getCopy());

  interface Props {
    transcripts: Transcript[];
    activeTranscript: Transcript | null;
    onTranscribe: (t: Transcript) => void;
    onSelect: (t: Transcript) => void;
    onDelete: (id: string) => void;
    /** Importa audio desde el botón del área del micrófono. */
    onImport: () => void;
    /** True mientras se importa audio desde el masthead. */
    importing: boolean;
    onBusyChange: (busy: boolean) => void;
  }

  let { transcripts, activeTranscript, onTranscribe, onSelect, onDelete, onImport, importing, onBusyChange }: Props = $props();

  let recStatus = $state("idle" as RecordingState);
  const transcribing = $derived(recStatus === "transcribing" || recStatus === "processing" || importing);
  $effect(() => {
    const busy = recStatus === "recording" || transcribing;
    onBusyChange(busy);
    return () => onBusyChange(false);
  });
  let recordingTime = $state(0);
  let recordingInterval: ReturnType<typeof setInterval> | null = null;
  let transcriptionSeconds = $state(0);
  let pcmRecorder: PcmRecorder | null = null;
  let starting = $state(false);
  let disposed = false;
  let audioStream = $state(null as MediaStream | null);
  let errorMessage = $state("");
  let modelPath = $state("");
  let activeWhisperId = $state("");
  let modelMissing = $state(true);
  let anyWhisperInstalled = $state(false);
  let modelDownloading = $state(false);
  let downloadProgress = $state(0);
  let nemotronMissing = $state(false);
  let nemotronAvailable = $state(false);
  let nemotronDownloading = $state(false);
  let nemotronProgress = $state(0);
  let nemotronInited = false;
  const nemotronId = "nemotron3-diarization";
  let historyOpen = $state(true);

  // El modelo de transcripción (Whisper o Parakeet) lo elige la ventana de
  // Modelos; aquí solo se usa para transcribir.
  const modelId = $derived(getSttModel());

  // Nemotron corta los turnos por tiempo y cada intervalo se transcribe por
  // separado, asi que la diarizacion vale igual con Whisper que con Parakeet.
  const nemotronActive = $derived(
    nemotronAvailable && (recStatus === "transcribing" || importing)
  );

  // Modelos locales en uso, mostrados como texto en la esquina inferior derecha.
  const modelInfo = $derived.by(() => {
    const o = STT_OPTIONS.find((o) => o.id === (activeWhisperId || modelId));
    if (!o) return { whisper: modelId };
    return { whisper: o.family === "whisper" ? `Whisper ${o.label}` : o.label };
  });

  // --- Orb en vivo ---
  // Tamaño FIJO (198px, el mismo que el círculo en reposo) y SIEMPRE en el mismo
  // hueco, para que la transición grabar/no-grabar calce perfecto. El volumen
  // real sólo modula un brillo interior (--orb-glow), nunca el tamaño/posición.
  let orbEl = $state(null as HTMLElement | null);
  let audioCtx: AudioContext | null = null;
  let analyser: AnalyserNode | null = null;
  let orbActive = false;
  let orbRaf = 0;

  function startOrb(stream: MediaStream) {
    stopOrb();
    try {
      audioCtx = new AudioContext();
      const source = audioCtx.createMediaStreamSource(stream);
      analyser = audioCtx.createAnalyser();
      analyser.fftSize = 1024;
      analyser.smoothingTimeConstant = 0.75;
      source.connect(analyser);
      if (audioCtx.state === "suspended") {
        audioCtx.resume().catch(() => {});
      }
    } catch (err) {
      console.error("Orb no disponible:", err);
      analyser = null;
    }
    orbActive = true;
    orbRaf = requestAnimationFrame(orbLoop);
  }

  function stopOrb() {
    orbActive = false;
    if (orbRaf) {
      cancelAnimationFrame(orbRaf);
      orbRaf = 0;
    }
    if (audioCtx) {
      audioCtx.close().catch(() => {});
      audioCtx = null;
    }
    analyser = null;
    if (orbEl) orbEl.style.setProperty("--orb-glow", "0");
  }

  function orbLoop() {
    if (!orbActive) return;
    // El volumen modula el brillo interior; CSS controla el pulso suave.
    if (analyser && orbEl) {
      const buf = new Float32Array(analyser.fftSize);
      analyser.getFloatTimeDomainData(buf);
      let sum = 0;
      for (let i = 0; i < buf.length; i++) sum += buf[i] * buf[i];
      const rms = Math.sqrt(sum / buf.length);
      orbEl.style.setProperty("--orb-glow", Math.min(1, rms * 6).toFixed(3));
    }
    orbRaf = requestAnimationFrame(orbLoop);
  }

  let unlistenProgress: (() => void) | null = null;

  // Atajo global de grabación: Espacio (y Cmd/Ctrl+Enter) para empezar/parar,
  // igual que indica el hint. No se activa si el foco está en un control.
  function onGlobalKeydown(e: KeyboardEvent) {
    const tgt = e.target as HTMLElement | null;
    const tag = tgt?.tagName;
    if (
      tag === "INPUT" ||
      tag === "SELECT" ||
      tag === "TEXTAREA" ||
      tag === "BUTTON" ||
      tgt?.isContentEditable ||
      // Ítems del historial (div role="button"): Espacio/Enter los abre a ellos.
      tgt?.closest(".history") != null
    ) {
      return;
    }
    if (e.repeat || starting || importing) return;
    const isSpace = e.code === "Space";
    const isCmdEnter = (e.metaKey || e.ctrlKey) && e.code === "Enter";
    if (!isSpace && !isCmdEnter) return;
    if (recStatus === "idle") {
      e.preventDefault();
      startRecording();
    } else if (recStatus === "recording") {
      e.preventDefault();
      stopRecording();
    }
  }

  onMount(() => {
    window.addEventListener("keydown", onGlobalKeydown);
    syncNemotron();
  });

  onDestroy(() => {
    disposed = true;
    onBusyChange(false);
    pcmRecorder?.dispose();
    window.removeEventListener("keydown", onGlobalKeydown);
    if (unlistenProgress) {
      unlistenProgress();
      unlistenProgress = null;
    }
    stopOrb();
    if (recordingInterval) clearInterval(recordingInterval);
    if (audioStream) audioStream.getTracks().forEach((t) => t.stop());
  });

  $effect(() => {
    modelId;
    syncModel();
  });

  // Cuenta el tiempo real desde el inicio de la transcripción, tanto al grabar
  // como al importar. El efecto limpia el intervalo al terminar o salir de la vista.
  $effect(() => {
    if (!transcribing) {
      transcriptionSeconds = 0;
      return;
    }
    const startedAt = performance.now();
    transcriptionSeconds = 0;
    const interval = setInterval(() => {
      transcriptionSeconds = Math.floor((performance.now() - startedAt) / 1000);
    }, 250);
    return () => clearInterval(interval);
  });

  async function syncModel() {
    try {
      const all = await invoke<{
        whisper: { id: string; path: string; installed: boolean }[];
        parakeet: { id: string; path: string; installed: boolean }[];
      }>("list_models");
      const pool = [...all.parakeet, ...all.whisper];
      const selected = pool.find((m) => m.id === modelId);
      const anyInstalled = pool.find((m) => m.installed);
      // Si la selección no está instalada pero hay otro modelo, transcribimos con el instalado.
      modelPath = (selected?.installed ? selected.path : anyInstalled?.path) ?? "";
      activeWhisperId = (selected?.installed ? selected.id : anyInstalled?.id) ?? "";
      anyWhisperInstalled = pool.some((m) => m.installed);
      modelMissing = !anyWhisperInstalled;
      if (!modelMissing && modelDownloading) {
        modelDownloading = false;
        downloadProgress = 0;
      }
    } catch (e: any) {
      errorMessage = String(e);
    }
  }

  async function syncNemotron() {
    if (nemotronInited) return;
    nemotronInited = true;
    try {
      const all = await invoke<{ diarization: { id: string; installed: boolean }[] }>("list_models");
      const entry = all.diarization.find((m) => m.id === nemotronId);
      nemotronMissing = !entry?.installed;
      nemotronAvailable = entry?.installed
        ? await invoke<boolean>("diarization_available").catch(() => false)
        : false;
      if (nemotronMissing && nemotronDownloading) {
        nemotronDownloading = false;
        nemotronProgress = 0;
      }
    } catch {
      nemotronInited = false;
    }
  }

  async function downloadModel() {
    if (modelDownloading || !modelMissing) return;
    modelDownloading = true;
    downloadProgress = 0;
    errorMessage = "";
    try {
      if (!unlistenProgress) {
        unlistenProgress = await listen<[string, number]>("model-download-progress", (event) => {
          if (event.payload[0] === modelId) {
            downloadProgress = event.payload[1];
            if (downloadProgress >= 100) {
              modelMissing = false;
            }
          }
        });
      }
      const path = await invoke<string>(
        sttFamily(modelId) === "parakeet" ? "download_parakeet_model" : "download_model",
        sttFamily(modelId) === "parakeet" ? { id: modelId } : { size: modelId },
      );
      modelPath = path;
      activeWhisperId = modelId;
      modelMissing = false;
      modelDownloading = false;
      downloadProgress = 100;
    } catch (e: any) {
      modelDownloading = false;
      errorMessage = String(e);
    }
  }

  async function downloadNemotron() {
    if (nemotronDownloading || !nemotronMissing) return;
    nemotronDownloading = true;
    nemotronProgress = 0;
    errorMessage = "";
    try {
      let unlisten: (() => void) | null = null;
      unlisten = await listen<[string, number]>("model-download-progress", (event) => {
        if (event.payload[0] === nemotronId) {
          nemotronProgress = event.payload[1];
          if (event.payload[1] >= 100) {
            nemotronMissing = false;
          }
        }
      });
      await invoke<string>("download_diarization_model");
      if (unlisten) unlisten();
      nemotronMissing = false;
      nemotronAvailable = await invoke<boolean>("diarization_available").catch(() => false);
      nemotronDownloading = false;
      nemotronProgress = 100;
    } catch (e: any) {
      nemotronDownloading = false;
      errorMessage = String(e);
    }
  }

  async function startRecording() {
    if (starting || importing || recStatus !== "idle") return;
    errorMessage = "";
    if (modelMissing) {
      errorMessage = t.needsModel;
      return;
    }
    starting = true;
    try {
      if (!navigator.mediaDevices?.getUserMedia) {
        errorMessage = t.micUnavailable;
        return;
      }
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      if (disposed) { stream.getTracks().forEach((t) => t.stop()); return; }
      audioStream = stream;
      pcmRecorder = await PcmRecorder.create(stream);
      if (disposed) { pcmRecorder.dispose(); stream.getTracks().forEach((t) => t.stop()); return; }
      recStatus = "recording";
      recordingTime = 0;
      startOrb(stream);

      recordingInterval = setInterval(() => {
        recordingTime += 1;
      }, 1000);
    } catch (err: any) {
      console.error("Microphone access denied:", err);
      errorMessage =
        err?.name === "NotAllowedError"
          ? t.micDenied
          : err?.message || t.micUnavailable;
      stopCleaning();
      recStatus = "idle";
    } finally {
      starting = false;
    }
  }

  async function prepareRecordingBlob(blob: Blob) {
    const duration = recordingTime || 1;
    stopCleaning();
    recStatus = "transcribing";
    try {
      const arrayBuffer = await blob.arrayBuffer();
      const config = {
        modelPath,
        language: "es",
        threads: 0,
        diarize: nemotronAvailable,
      };
      const token = await invoke<string>("prepare_transcribe_blob", { config });
      const trans = await invoke<{ id: string; text: string; segments: { start: number; end: number; text: string; speaker?: number }[]; language: string; duration: number }>("transcribe_blob", new Uint8Array(arrayBuffer), {
        headers: { "X-Local Legal-Local-Demo-Config": token },
      });
      const transcript: Transcript = {
        id: generateId(),
        title: `Grabación ${new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}`,
        text: trans.text,
        segments: trans.segments,
        createdAt: new Date(),
        duration: trans.duration || duration,
        language: trans.language,
        model: activeWhisperId,
        audioSource: "recording",
      };
      recStatus = "idle";
      onTranscribe(transcript);
    } catch (e: any) {
      console.error(e);
      errorMessage = String(e);
      recStatus = "idle";
    }
  }

  function stopCleaning() {
    stopOrb();
    if (recordingInterval) { clearInterval(recordingInterval); recordingInterval = null; }
    if (audioStream) { audioStream.getTracks().forEach((t) => t.stop()); audioStream = null; }
    recordingTime = 0;
  }

  async function stopRecording() {
    if (!pcmRecorder || recStatus !== "recording") return;
    recStatus = "processing";
    const recorder = pcmRecorder;
    pcmRecorder = null;
    try {
      const blob = await recorder.stop();
      if (!disposed) await prepareRecordingBlob(blob);
    } catch (e) {
      errorMessage = String(e);
      stopCleaning();
      recStatus = "idle";
    }
  }

  function handlePanelClick() {
    if (transcribing) return;
    if (recStatus === "idle") startRecording();
    else if (recStatus === "recording") stopRecording();
  }

  function handlePanelKeydown(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      handlePanelClick();
    }
  }
</script>

<section class="recorder">
  <!-- Historial: columna izquierda desplegable -->
  <aside class="history" class:collapsed={!historyOpen}>
    {#if historyOpen}
      <div class="history-head">
        <span class="history-title">{t.history}</span>
        <button class="collapse-btn" title={t.collapseHistory} aria-label={t.collapseHistory} onclick={() => (historyOpen = false)}>
          <svg viewBox="0 0 20 20" aria-hidden="true"><path d="M12.5 4.5L7 10l5.5 5.5"></path></svg>
        </button>
      </div>
      {#if transcripts.length > 0}
        <div class="history-list">
          {#each transcripts as tr (tr.id)}
            <div
              class="history-item"
              class:active={activeTranscript?.id === tr.id}
              role="button"
              tabindex="0"
              onclick={() => onSelect(tr)}
              onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); onSelect(tr); } }}
            >
              <div class="hi-copy">
                <strong>{tr.title}</strong>
                <small>{formatDuration(tr.duration)} · {tr.language} · {tr.model}</small>
              </div>
              <button class="icon-button" title={t.deleteTranscript} onclick={(e) => { e.stopPropagation(); onDelete(tr.id); }} aria-label={t.deleteTranscript}>×</button>
            </div>
          {/each}
        </div>
      {:else}
        <div class="history-empty">
          <p>{t.noHistory}</p>
        </div>
      {/if}
    {:else}
      <button
        class="collapse-btn"
        title={t.expandHistory}
        aria-label={t.expandHistory}
        onclick={() => (historyOpen = true)}
      >
        <svg viewBox="0 0 20 20" aria-hidden="true"><path d="M7.5 4.5L13 10l-5.5 5.5"></path></svg>
      </button>
    {/if}
  </aside>

  <!-- Panel del micrófono: círculo central + texto de estado -->
  <div class="mic-panel">
    <div
      class="mic-btn"
      class:recording={recStatus === "recording"}
      class:busy={transcribing}
      role="button"
      tabindex="0"
      aria-label={t.recordTitle}
      onclick={handlePanelClick}
      onkeydown={handlePanelKeydown}
    >
      <span class="hero">
        {#if transcribing}
          <div class="spinner"></div>
        {:else}
          <!-- Un solo contenedor con la GEOMETRÍA del círculo del mic (mismo
               hueco, mismo anillo, mismo fondo): el mic se funde al orb animado
               y vuelve, sin sustituir elementos ni cambiar tamaño. -->
          <span class="orb-window" class:recording={recStatus === "recording"} bind:this={orbEl}>
            <span class="orb-visuals">
              <span class="orb-g1"></span>
              <span class="orb-g2"></span>
              <span class="orb-glow"></span>
              <span class="orb-shade"></span>
            </span>
            <span class="mic-glyph">
              <svg class="mic-icon" viewBox="0 0 20 20" aria-hidden="true">
                <rect x="6.5" y="2.5" width="7" height="10" rx="3.2"></rect>
                <path d="M4.5 10a5.5 5.5 0 0 0 11 0"></path>
                <path d="M10 15.5V17"></path>
              </svg>
            </span>
          </span>
        {/if}
      </span>
      <div class="status">
        {#if transcribing}
          <strong class="state-title">{t.transcribing}</strong>
          <span class="state-sub">{t.transcribingHint}</span>
        {:else if recStatus === "recording"}
          <strong class="rec-time">{Math.floor(recordingTime / 60)}:{(recordingTime % 60).toString().padStart(2, "0")}</strong>
          <span class="state-sub">{t.recordingLabel}</span>
        {:else}
          <strong class="state-title">{t.recordTitle}</strong>
          <span class="state-sub">{t.recordHint}</span>
          <span class="state-sub state-or">{t.or}</span>
          <button
            class="import-inline"
            id="importToggle"
            title={t.importAudio}
            aria-label={t.importAudio}
            disabled={importing}
            onclick={(e) => { e.stopPropagation(); onImport(); }}
          >
            <svg viewBox="0 0 20 20" aria-hidden="true">
              <path d="M10 13V3"></path>
              <path d="M6 7l4-4 4 4"></path>
              <path d="M4 13v2a1 1 0 0 0 1 1h10a1 1 0 0 0 1-1v-2"></path>
            </svg>
            <span>{t.importAudio}</span>
          </button>

        {/if}
      </div>
    </div>

    {#if errorMessage}
      <p class="error" role="alert">{errorMessage}</p>
    {/if}

{#if modelMissing}
        <div class="model-card">
          <div class="model-copy">
            <strong>{t.download} {t.model}: Whisper {modelInfo.whisper}</strong>
            <span>{t.setupWhisperLink}</span>
          </div>
          {#if !modelDownloading}
            <button class="download-btn" onclick={downloadModel}>{t.download}</button>
          {:else}
            <div class="download-progress">
              <div class="progress-track"><div class="progress-fill" style={`width: ${downloadProgress}%`}></div></div>
              <span>{downloadProgress}%</span>
            </div>
          {/if}
        </div>
      {/if}

      {#if nemotronMissing}
        <div class="model-card nemotron-card">
          <div class="model-copy">
            <strong>{t.download} {t.setupNemotronTitle}</strong>
            <span>{t.setupNemotronLink}</span>
          </div>
          {#if !nemotronDownloading}
            <button class="download-btn" onclick={downloadNemotron}>{t.download}</button>
          {:else}
            <div class="download-progress">
              <div class="progress-track"><div class="progress-fill" style={`width: ${nemotronProgress}%`}></div></div>
              <span>{nemotronProgress}%</span>
            </div>
          {/if}
        </div>
      {/if}

      <div class="local-models" title={t.usingModelsTitle}>
        {#if transcribing}
          <span class="transcription-clock" role="timer" aria-label={t.transcriptionTime} title={t.transcriptionTime}>
            <span class="clock-face" aria-hidden="true"></span>
            {formatDuration(transcriptionSeconds)}
          </span>
        {/if}
        <div class="local-model-line">
          <span class="lm-label">{t.localModels}</span>
          <span class="lm-item" class:on={recStatus === "recording" || recStatus === "transcribing" || importing}>Whisper · {modelInfo.whisper}</span>
          <span class="lm-separator">·</span>
          <span class="lm-item nemotron-item" class:on={nemotronActive}>NVIDIA Nemotron 3</span>
        </div>
      </div>
    </div>
  </section>

<style>
  .recorder {
    flex: 1;
    min-height: 0;
    display: flex;
    gap: 0;
    border: 1px solid var(--line);
    border-top: 0;
    border-radius: 0 0 6px 6px;
    overflow: hidden;
  }

  /* --- Historial (izquierda, desplegable) --- */
  .history {
    width: 264px;
    min-width: 264px;
    height: 100%;
    border: 1px solid var(--line);
    border-radius: 0;
    background: var(--panel);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    transition: width .15s ease, min-width .15s ease;
  }
  .history.collapsed { width: 46px; min-width: 46px; }

  .history-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--line);
  }
  .history-title {
    color: var(--muted);
    font-size: 12px;
    font-weight: 500;
    letter-spacing: .12em;
    text-transform: uppercase;
  }
  .collapse-btn {
    width: 26px;
    height: 26px;
    padding: 0;
    border: 0;
    border-radius: 5px;
    background: transparent;
    color: var(--muted);
    cursor: pointer;
    display: grid;
    place-items: center;
    flex: none;
  }
  .collapse-btn:hover { background: var(--hover); color: var(--text); }
  .collapse-btn svg { width: 15px; height: 15px; fill: none; stroke: currentColor; stroke-width: 1.6; stroke-linecap: round; stroke-linejoin: round; }

  /* Colapsado: solo el mismo chevron que abre/cierra, anclado arriba-derecha
     en el mismo sitio y altura que el botón de la cabecera abierta. */
  .history.collapsed .collapse-btn {
    margin: 10px 10px auto auto;
  }

  .history-list { flex: 1; min-height: 0; overflow-y: auto; padding: 5px; }
  .history-list::-webkit-scrollbar { width: 8px; }
  .history-list::-webkit-scrollbar-thumb { background: var(--line-dark); border-radius: 4px; }
  .history-empty {
    flex: 1;
    display: grid;
    place-items: center;
    padding: 16px;
    color: var(--muted);
    font-size: 13.5px;
    text-align: center;
    line-height: 1.5;
  }
  .history-empty p { margin: 0; }
  .history-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 9px 10px;
    border-radius: 6px;
    cursor: pointer;
  }
  .history-item:hover { background: var(--hover); }
  .history-item.active { background: var(--segment-active); }
  .hi-copy { min-width: 0; flex: 1; display: flex; flex-direction: column; gap: 3px; }
  .hi-copy strong { font-size: 14.5px; font-weight: 500; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .hi-copy small { color: var(--muted); font-size: 12px; }
  .icon-button {
    width: 22px;
    height: 22px;
    padding: 0;
    border: 0;
    border-radius: 4px;
    color: var(--muted);
    background: transparent;
    font-size: 20px;
    line-height: 20px;
    cursor: pointer;
    flex: none;
    opacity: 0;
    transition: opacity .12s;
  }
  .history-item:hover .icon-button { opacity: 1; }
  .icon-button:hover { color: var(--error); background: var(--hover); }

  /* --- Panel del micrófono (derecha, centrado) --- */
  .mic-panel {
    background: var(--surface);
    flex: 1;
    min-width: 0;
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 16px;
    padding: 18px;
  }

  /* Modelos locales en uso, en la esquina inferior derecha de la caja de grabar. */
  .local-models {
    position: absolute;
    right: 12px;
    bottom: 8px;
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 3px;
    color: var(--muted);
    font-size: 12.5px;
    font-weight: 500;
    letter-spacing: .01em;
    white-space: nowrap;
    user-select: none;
  }
  .local-model-line {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .local-model-line > span { color: var(--muted); }
  .local-models .lm-item.on {
    color: var(--lm-live);
    animation: lmPulse 200ms steps(1, end) infinite;
  }
  @keyframes lmPulse {
    0%, 49.9% { color: var(--lm-live); }
    50%, 100% { color: var(--muted); }
  }
  .local-models .lm-label { color: var(--muted); }
  .transcription-clock {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: #66666b;
    font-size: 23px;
    font-weight: 600;
    line-height: 1;
    font-variant-numeric: tabular-nums;
  }
  .clock-face {
    position: relative;
    display: inline-block;
    width: 18px;
    height: 18px;
    border: 1.5px solid currentColor;
    border-radius: 50%;
  }
  .clock-face::before {
    content: "";
    position: absolute;
    top: 2px;
    left: 7px;
    width: 1.5px;
    height: 5px;
    background: currentColor;
  }
  .clock-face::after {
    content: "";
    position: absolute;
    top: 7px;
    left: 7px;
    width: 5px;
    height: 1.5px;
    background: currentColor;
    transform-origin: left center;
    transform: rotate(28deg);
  }

  .mic-btn {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 20px;
    padding: 26px 30px 24px;
    border-radius: 16px;
    outline: none;
    cursor: pointer;
    transition: background-color .12s ease;
  }
  .mic-btn:hover { background: var(--hover); }
  .mic-btn:focus-visible { box-shadow: 0 0 0 2px var(--accent); }
  .mic-btn.busy { cursor: default; }

  /* Hueco fijo: el círculo y el orb ocupan EXACTAMENTE el mismo hueco, así la
     transición grabar/no-grabar no salta de tamaño ni de lugar. */
  .hero {
    width: 198px;
    height: 198px;
    flex: none;
    position: relative;
    display: grid;
    place-items: center;
  }

  /* Contenedor único: exactamente el círculo del mic (mismo hueco, mismas
     medidas, mismo anillo y sombra). Grabando muestra el orb animado en su
     interior; en reposo el mic. El pulso no modifica el espacio del layout. */
  .orb-window {
    position: relative;
    width: 100%;
    height: 100%;
    border-radius: 50%;
    border: 1.5px solid var(--line-dark);
    background: var(--surface);
    overflow: hidden;
    box-shadow: 0 6px 22px var(--shadow);
    transition: border-color .12s ease;
  }
  .mic-btn:hover .orb-window { border-color: var(--accent); }

  .orb-visuals {
    position: absolute;
    inset: 0;
    border-radius:50%;
    overflow:hidden;
    clip-path:circle(50% at 50% 50%);
    isolation:isolate;
    opacity: 0;
    transition: opacity .22s ease;
  }
  .orb-window.recording { animation:orbBreathe 2s ease-in-out infinite; border-color:#81907b; }
  .orb-window.recording .orb-visuals { opacity:1; background:linear-gradient(135deg,#ece9d8,#536650); }
  @keyframes orbBreathe { 0%,100% { transform:scale(1); } 50% { transform:scale(1.05); } }
  @media(prefers-reduced-motion:reduce) { .orb-window.recording,.orb-g1,.orb-g2 { animation:none; } }
  .orb-visuals > span { position: absolute; display: block; }
  .orb-g1 {
    inset: -22%;
    background: radial-gradient(circle at 32% 30%, #d8e2bc 0%, #6d8867 28%, rgba(109,136,103,0) 68%);
    filter: blur(7px);
    animation: orbDrift1 5.5s ease-in-out infinite alternate;
  }
  .orb-g2 {
    inset: -28%;
    background: radial-gradient(circle at 72% 74%, #e5bc8d 0%, #b77b58 28%, rgba(183,123,88,0) 66%);
    filter: blur(11px);
    animation: orbDrift2 6.5s ease-in-out infinite alternate;
  }
  .orb-glow {
    inset: 0;
    background: radial-gradient(circle at 50% 42%, rgba(255, 255, 255, .55) 0%, rgba(255, 255, 255, 0) 62%);
    opacity: var(--orb-glow, 0);
    mix-blend-mode: screen;
  }
  .orb-shade {
    inset: 0;
    background: radial-gradient(circle at 50% 24%, rgba(0, 0, 0, 0) 40%, rgba(0, 0, 0, .38) 100%);
  }
  .mic-glyph {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    opacity: 1;
    transition: opacity .22s ease;
  }
  .orb-window.recording .mic-glyph { opacity: 0; }
  .mic-icon { width: 88px; height: 88px; color: var(--accent); fill: none; stroke: currentColor; stroke-width: 1.3; stroke-linecap: round; stroke-linejoin: round; }

  /* Bloque de texto de estado con ALTURA FIJA igual al estado más alto (reposo:
     título + hint + "or" + botón importar). Así el círculo/orb NO se desplaza
     al empezar/detener la grabación ni al transcribir. */
  .status {
    min-height: 128px;
    flex: none;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 7px;
  }
  .state-title { font-family: var(--font-heading); font-size: 24px; font-weight: 400; }
  .state-sub { color: var(--muted); font-size: 15px; }
  .state-or { margin-top: -4px; }

  .import-inline {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    margin-top: 4px;
    height: 30px;
    padding: 0 12px;
    border: 1px solid var(--line-dark);
    border-radius: 7px;
    background: var(--surface);
    color: var(--text);
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
  }
  .import-inline:hover:not(:disabled) { background: var(--hover); border-color: var(--muted); }
  .import-inline:disabled { opacity: .55; cursor: default; }
  .import-inline svg { width: 15px; height: 15px; fill: none; stroke: currentColor; stroke-width: 1.5; stroke-linecap: round; stroke-linejoin: round; }
  @keyframes orbDrift1 {
    from { transform: translate3d(-6%, -4%, 0) scale(1); }
    to   { transform: translate3d(6%, 4%, 0) scale(1.12); }
  }
  @keyframes orbDrift2 {
    from { transform: translate3d(5%, 6%, 0) scale(1.08); }
    to   { transform: translate3d(-5%, -6%, 0) scale(0.96); }
  }

  .rec-time { font-size: 36px; font-weight: 500; letter-spacing: -.02em; font-variant-numeric: tabular-nums; }

  .spinner {
    width: 30px;
    height: 30px;
    margin-bottom: 2px;
    border: 2.5px solid var(--line-dark);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin .8s linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }

  .error {
    max-width: 420px;
    padding: 10px 12px;
    color: var(--error);
    background: var(--error-background);
    border: 1px solid var(--line);
    border-radius: 8px;
    font-size: 13.5px;
    user-select: text;
  }

  .model-card {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 11px 14px;
    border: 1px solid var(--line-dark);
    border-radius: 9px;
    background: var(--surface);
    max-width: 420px;
  }
  .nemotron-card { border-left: 3px solid var(--line-dark); }
  .model-copy { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px; }
  .model-copy strong { font-size: 14px; font-weight: 500; }
  .model-copy span { color: var(--muted); font-size: 13px; }
  .download-btn {
    flex: none;
    height: 34px;
    padding: 0 15px;
    border: 1px solid var(--button);
    border-radius: 7px;
    color: var(--button-text);
    background: var(--button);
    font-size: 13.5px;
    cursor: pointer;
  }
  .download-btn:hover { border-color: var(--button-hover); background: var(--button-hover); }
  .download-progress { flex: none; display: flex; align-items: center; gap: 8px; width: 150px; }
  .progress-track { flex: 1; height: 5px; border-radius: 3px; background: var(--line-dark); overflow: hidden; }
  .progress-fill { height: 100%; border-radius: 3px; background: var(--accent); transition: width .15s ease; }
  .download-progress span { color: var(--muted); font-size: 13px; font-variant-numeric: tabular-nums; }
</style>
