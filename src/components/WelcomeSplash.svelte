<script lang="ts">
  import { onMount } from "svelte";
  import { privateCopy as p } from "../lib/private-i18n";
  import mark from "../assets/local-legal-mark.svg";

  let { ready, loading, onClose, onContinue }: {
    ready: boolean; loading: boolean; onClose: () => void; onContinue: () => void;
  } = $props();
  let dialog: HTMLDivElement;
  let closeButton: HTMLButtonElement;
  onMount(() => {
    const previous = document.activeElement as HTMLElement | null;
    closeButton.focus();
    return () => previous?.focus();
  });
  function handleKey(event: KeyboardEvent) {
    if (event.key === "Escape") { event.preventDefault(); onClose(); }
    if (event.key !== "Tab") return;
    const buttons = Array.from(dialog.querySelectorAll<HTMLButtonElement>("button:not(:disabled)"));
    const first = buttons[0], last = buttons[buttons.length - 1];
    if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus(); }
    else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus(); }
  }
</script>

<div class="welcome" bind:this={dialog} role="dialog" aria-modal="true" aria-labelledby="welcome-title" tabindex="-1" onkeydown={handleKey}>
  <button class="close" bind:this={closeButton} onclick={onClose} aria-label={p("Cerrar bienvenida")}>
    <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true"><path d="m6 6 12 12M18 6 6 18" /></svg>
  </button>
  <div class="content">
    <h1 id="welcome-title">{p("Bienvenido a")}</h1>
    <img src={mark} alt="Local Legal" />
    <p>{p("Prepara y protege documentos y audio localmente de manera privada antes de enviarlos al servidor.")}</p>
    <button class="continue" onclick={onContinue} disabled={loading}>{loading ? p("Comprobando modelos…") : ready ? p("Comenzar") : p("Descargar modelos locales")}</button>
  </div>
</div>

<style>
  .welcome { position: fixed; inset: 0; z-index: 900; background: #fafaf7; color: #373731; display: grid; place-items: center; overflow: auto; padding: 64px 24px 40px; }
  .close { position: absolute; top: 24px; right: 28px; width: 40px; height: 40px; display: grid; place-items: center; border: 0; background: transparent; color: #373731; cursor: pointer; border-radius: 6px; }
  .close:hover { background: #f2efe7; }
  .content { width: min(100%, 620px); text-align: center; display: flex; flex-direction: column; align-items: center; }
  h1 { font-family: 'Newsreader', Georgia, serif; font-size: clamp(30px, 4vw, 46px); font-weight: 400; margin: 0 0 12px; }
  img { width: clamp(150px, 29vh, 260px); height: auto; object-fit: contain; margin: 8px 0 22px; }
  p { font-size: 17px; line-height: 1.6; max-width: 460px; margin: 0 0 30px; }
  .continue { border: 1px solid #373731; background: #373731; color: #fafaf7; padding: 13px 26px; font-size: 15px; border-radius: 6px; cursor: pointer; }
  .continue:hover { background: #4a4a42; }
  .continue:disabled { opacity: .5; cursor: wait; }
  button:focus-visible { outline: 2px solid #82715a; outline-offset: 4px; }
</style>
