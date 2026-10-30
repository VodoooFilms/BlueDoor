<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { privateCopy as p } from "../lib/private-i18n";
  import logo from "../assets/openai-white.png";
  import Feedback from "./Feedback.svelte";
  let { disabled = false }: { disabled?: boolean } = $props();
  let opening = $state(false);
  let error = $state("");
  let message = $state("");
  async function openChatGPT() {
    if (opening || disabled) return;
    opening = true; error = ""; message = "";
    try {
      // Opens a fixed destination only. No document, title, path or text leaves BlueDoor.
      await invoke("open_chatgpt_home");
      message = p("ChatGPT abierto. Exporta el PDF protegido y adjúntalo en el chat.");
    } catch {
      error = p("No se pudo abrir ChatGPT. Abre chatgpt.com en tu navegador y vuelve a intentarlo.");
    } finally { opening = false; }
  }
</script>
<button class="secondary gpt-button" onclick={openChatGPT} disabled={disabled || opening} aria-label={p("Enviar a GPT")} title={p("Abrir ChatGPT en el navegador; el PDF se adjunta manualmente.")}>
  <img src={logo} alt="" /><span>GPT</span>
</button>
<Feedback message={error} onDismiss={() => error = ""} />
<Feedback {message} kind="status" onDismiss={() => message = ""} />
<style>
  .gpt-button { display:inline-flex; align-items:center; justify-content:center; gap:9px; min-height:40px; padding:8px 12px; flex:none; color:white; font-size:13px; font-weight:700; white-space:nowrap; cursor:pointer; }
  .gpt-button:disabled { opacity:.5; cursor:default; }
  img { width:20px; height:20px; object-fit:contain; flex:none; }
</style>
