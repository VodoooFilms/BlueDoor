<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { getLang } from "../lib/i18n-store.svelte";
  import { DOCUMENTATION } from "../lib/documentation";
  import Icon from "./Icon.svelte";

  let { onClose }: { onClose: () => void } = $props();
  const copy = $derived(DOCUMENTATION[getLang()]);
  let dialog: HTMLDialogElement;
  let error = $state("");
  let opening = $state(false);

  onMount(() => {
    dialog.showModal();
    return () => dialog.close();
  });

  async function openSupport() {
    if (opening) return;
    opening = true;
    error = "";
    try {
      await invoke("open_bluedoor_support", { lang: getLang() });
    }
    catch { error = copy.supportError; }
    finally { opening = false; }
  }
</script>

<dialog class="documentation" bind:this={dialog} aria-labelledby="documentation-title" onclose={onClose}>
  <header>
    <h1 id="documentation-title">{copy.title}</h1>
    <button class="close" onclick={() => dialog.close()} aria-label={copy.close} title={copy.close}><svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true"><path d="m6 6 12 12M18 6 6 18" /></svg></button>
  </header>
  <article lang={getLang()}>
    <div class="isotype"><Icon name="door" size={100} /></div>
    <section aria-labelledby="documentation-about">
      <h2 id="documentation-about">{copy.aboutTitle}</h2>
      {#each copy.about.split("\n\n") as paragraph}<p>{paragraph}</p>{/each}
      <p><strong>{copy.review}</strong></p>
    </section>
    <section aria-labelledby="documentation-usage">
      <h2 id="documentation-usage">{copy.usageTitle}</h2>
      {#each copy.usage.split("\n\n") as paragraph}<p>{paragraph}</p>{/each}
    </section>
    <section aria-labelledby="documentation-audio">
      <h2 id="documentation-audio">{copy.audioTitle}</h2>
      {#each copy.audio.split("\n\n") as paragraph}<p>{paragraph}</p>{/each}
    </section>
    <section aria-labelledby="documentation-vault">
      <h2 id="documentation-vault">{copy.vaultTitle}</h2>
      <p>{copy.vault}</p>
      <p><strong>{copy.vaultWarning}</strong></p>
      {#each copy.vaultDetails.split("\n\n") as paragraph}<p>{paragraph}</p>{/each}
    </section>
    <footer>
      <h2>{copy.credits}</h2>
      <p>{copy.createdBy}</p>
      <p><strong>VodoooFilms</strong></p>
      <div class="credit-links">
      <button class="repository support" onclick={openSupport} disabled={opening} title={copy.support}><span>{copy.support}</span><Icon name="arrow" size={18} /></button>
      </div>
      {#if error}<p class="link-error" role="alert">{error}</p>{/if}
    </footer>
  </article>
</dialog>

<style>
  .documentation { position:fixed; inset:0; box-sizing:border-box; width:100%; max-width:none; height:100dvh; max-height:none; margin:0; padding:0; border:0; background:var(--background); color:white; overflow:auto; }
  .documentation::backdrop { background:#1248bc; }
  header { position:sticky; top:0; z-index:1; display:flex; align-items:center; justify-content:space-between; gap:16px; padding:20px 28px; background:var(--background); border-bottom:1px solid #b8d4ff55; }
  h1 { margin:0; font:600 18px/1.4 var(--font-heading); }
  .close { display:grid; place-items:center; flex:none; width:40px; height:40px; padding:0; border:0; border-radius:6px; color:var(--text); background:transparent; cursor:pointer; }
  .close:hover { background:var(--hover); }
  article { box-sizing:border-box; width:min(100%,800px); margin:0 auto; padding:40px 32px 64px; }
  .isotype { display:flex; justify-content:center; margin:0 0 48px; color:white; }
  section { margin:0 0 36px; }
  h2 { margin:0 0 14px; font:600 22px/1.35 var(--font-heading); letter-spacing:-.4px; }
  p { margin:0; color:#b8d4ff; font:400 16px/1.85 var(--font-body, sans-serif); overflow-wrap:break-word; text-align:justify; hyphens:auto; }
  p + p { margin-top:20px; }
  strong { color:white; font-weight:650; }
  footer { border-top:1px solid #b8d4ff55; padding-top:28px; }
  footer h2 { font-size:18px; }
  .credit-links { display:flex; gap:12px; margin-top:18px; }
  .repository.support { width:100%; height:48px; min-height:48px; justify-content:center; gap:8px; border-radius:9px; background:transparent; color:white; }
  .repository { display:inline-flex; align-items:center; gap:10px; margin-top:0; min-height:44px; max-width:100%; padding:10px 16px; border:1px solid white; border-radius:8px; background:white; color:#1248bc; font:600 14px/1.5 var(--font-body, sans-serif); cursor:pointer; }
  button:disabled { opacity:.6; cursor:wait; }
  button:focus-visible { outline:2px solid white; outline-offset:4px; }
  .link-error { margin-top:16px; color:white; }
  @media(max-width:600px) { header { padding:14px 20px; } article { padding:32px 24px 48px; } .isotype { margin-bottom:36px; } h2 { font-size:21px; } p { font-size:15px; line-height:1.8; } }
</style>
