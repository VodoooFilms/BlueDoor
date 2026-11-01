<script lang="ts">
  import { onDestroy, tick } from "svelte";
  import { BatchController, type BatchItem } from "../lib/batch.svelte";
  import { privateCopy as p } from "../lib/private-i18n";
  import Icon from "./Icon.svelte";
  import Feedback from "./Feedback.svelte";
  let { controller:c, modelId, dirty=false }:{controller:BatchController;modelId:()=>string;dirty?:boolean}=$props();
  let dialog = $state<HTMLDialogElement>();
  let returnFocus:HTMLElement|null=null;
  let wasOpen=false;
  let aliasQuery=$state("");
  let aliasDrafts=$state<Record<string,string>>({});
  const filteredAliases=$derived(c.aliases.groups.filter(g=>[g.alias,...g.variants].join(" ").toLocaleLowerCase().includes(aliasQuery.toLocaleLowerCase())));
  const aliasMerge=$derived(c.aliasPending?.request.action.type==="merge");
  const mergeTarget=$derived.by(()=>{const action=c.aliasPending?.request.action;if(action?.type!=="merge")return undefined;return c.aliases.groups.find(g=>g.id===action.targetId);});
  function identity(id:string){return c.aliases.groups.find(g=>g.id===id);}
  let elapsedSeconds=$state(0);
  $effect(()=>{
    elapsedSeconds=0;
    if(!c.running)return;
    const started=performance.now();
    const timer=setInterval(()=>{elapsedSeconds=Math.floor((performance.now()-started)/1000);},250);
    return ()=>clearInterval(timer);
  });
  const position=$derived(c.reviewable.findIndex(i=>i.id===c.view?.active));
  const current=$derived(c.view?.items.find(i=>i.id===c.view?.active));
  const total=$derived(c.view?.items.length ?? 0);
  const processing=$derived(c.view?.items.findIndex(i=>i.status==="processing") ?? -1);
  const pending=$derived(c.view?.items.filter(i=>i.status==="pending").length ?? 0);
  function status(item:BatchItem) {
    return p(({pending:"Pendiente",processing:"Analizando",review:"Por revisar",approved:"Aprobado",exported:"Exportado",error:"Error",cancelled:"Cancelado"})[item.status]);
  }
  $effect(()=>{
    if(!dialog)return;
    if(c.mode&&!dialog.open){returnFocus=document.activeElement instanceof HTMLElement?document.activeElement:null;dialog.showModal();wasOpen=true;}
    else if(!c.mode&&dialog.open){dialog.close();if(wasOpen){wasOpen=false;void tick().then(()=>returnFocus?.isConnected&&returnFocus.focus());}}
  });
  onDestroy(()=>dialog?.close());
  function cancelDialog(){if(c.working)return;if(c.mode==="aliasChange"){c.cancelAliasChange();return;}if(c.mode==="prepare")void c.close();else c.mode=null;}
</script>

  {#if c.view?.started && !c.running && current?.sessionId}
    <nav class="batch-footer" aria-label={p("Navegación del lote")}>
      <div class="batch-footer-status">{p("Documento {0} de {1}").replace("{0}",String(c.index+1)).replace("{1}",String(total))} · {status(current)}</div>
      <div class="batch-footer-actions">
        <button disabled={c.locked||dirty||position<=0} onclick={()=>c.navigate(-1)} title={p("Anterior")}><Icon name="arrow-left" size={16}/><span>{p("Anterior")}</span></button>
        <button disabled={c.locked||dirty} onclick={()=>c.mode="list"} title={p("Ver lista")}><Icon name="list" size={16}/><span>{p("Ver lista")}</span></button>
        <button disabled={c.locked||dirty||position>=c.reviewable.length-1} onclick={()=>c.navigate(1)} title={p("Siguiente")}><span>{p("Siguiente")}</span><Icon name="arrow" size={16}/></button>
      </div>
    </nav>
  {:else if c.view?.started}
    <section class="batch-summary" aria-label={p("Estado del lote")} aria-live="polite">
      {#if c.running}
        <strong class="batch-processing-label"><span class="processing-orbit" aria-hidden="true"></span><span>{p("Analizando {0} de {1} documentos").replace("{0}",String(processing+1||total)).replace("{1}",String(total))}</span></strong>
        <div class="batch-summary-line batch-progress-actions"><span>{p("{0} por revisar · {1} pendientes").replace("{0}",String(c.ready)).replace("{1}",String(pending))}</span><button disabled={!pending} onclick={()=>c.cancelPending()}><Icon name="close" size={16}/><span>{p("Cancelar pendientes")}</span></button><span class="processing-elapsed" aria-live="off" aria-label={p("Tiempo transcurrido en segundos")}>{String(elapsedSeconds).padStart(3, "0")} s</span></div>
        <div class="processing-track" role="progressbar" aria-label={p("Procesando documento")} aria-busy="true"><span></span></div>
      {:else}
        <strong>{p("{0} documentos · {1} aprobados").replace("{0}",String(total)).replace("{1}",String(c.approved))}</strong>
        <div class="batch-summary-line"><span>{p("{0} por revisar · {1} errores").replace("{0}",String(c.ready)).replace("{1}",String(c.view.items.filter(i=>i.status==="error").length))}</span><button disabled={c.locked||dirty} onclick={()=>c.mode="list"}><Icon name="list" size={16}/><span>{p("Ver lista")}</span></button></div>
      {/if}
    </section>
  {/if}
  <Feedback message={c.error?p(c.error):""} onDismiss={()=>c.error=""}/>
  <Feedback message={c.notice} kind="status" onDismiss={()=>c.notice=""}/>
  <dialog bind:this={dialog} class="batch-dialog" aria-labelledby="batch-dialog-title" oncancel={e=>{e.preventDefault();cancelDialog();}}>
    <header><h2 id="batch-dialog-title">{p(c.mode==="prepare"?"Detectar varios archivos":c.mode==="export"?"Exportar aprobados":c.mode==="close"?"Cerrar lote":c.mode==="aliases"?"Alias del lote":c.mode==="aliasChange"?(aliasMerge?"Unir variantes":"Actualizar alias compartido"):"Documentos del lote")}</h2><button class="batch-close" onclick={cancelDialog} disabled={c.working} aria-label={p("Cerrar ventana")} title={p("Cerrar ventana")}><Icon name="close" size={18}/><span>{p("Cerrar")}</span></button></header>
    {#if c.mode==="prepare"}
      <p>{p("Selecciona los documentos que quieres detectar. Se procesarán uno por uno; tú revisarás y aprobarás cada resultado.")}</p>
      <p class="batch-note">{p("Hasta 20 documentos · 50 MB en total · Sin subcarpetas. Los originales no se modifican. El lote permanece solo en memoria y se pierde al cerrar la app.")}</p>
    {:else if c.mode==="export"}
      <p>{p("{0} aprobados · {1} sin aprobar").replace("{0}",String(c.approved)).replace("{1}",String(total-c.approved))}</p>
      <p>{p("Elige una carpeta de salida. Solo se exportarán los aprobados, con nombres nuevos y sin sobrescribir archivos.")}</p>
    {:else if c.mode==="close"}
      <p>{p("Se descartará el lote de la memoria, incluidos los documentos pendientes de revisión o exportación. Los archivos ya exportados se conservan.")}</p>
    {/if}
    {#if c.mode==="aliases"}
      <p>{p("Una identidad, un alias en todo el lote. Las sugerencias no se unen automáticamente.")}</p>
      <label class="alias-search"><Icon name="search" size={18}/><input type="search" bind:value={aliasQuery} aria-label={p("Buscar identidad o alias")} placeholder={p("Buscar identidad o alias")}/></label>
      {#if c.aliases.suggestions.length&&!aliasQuery}
        <section class="alias-suggestions" aria-label={p("Posibles variantes")}>
          <h3>{p("Posibles variantes")}</h3>
          {#each c.aliases.suggestions as [target,source]}
            <div class="alias-suggestion">
              <span>{identity(target)?.variants[0]} <span aria-hidden="true">↔</span> {identity(source)?.variants[0]}</span>
              <button disabled={c.locked||dirty} onclick={()=>c.changeAlias({type:"merge",sourceId:source,targetId:target})}><Icon name="map" size={16}/><span>{p("Revisar unión")}</span></button>
            </div>
          {/each}
        </section>
      {/if}
      <div class="shared-alias-list">
        {#each filteredAliases as group (group.id)}
          <section class="shared-identity">
            <div class="shared-identity-heading"><strong>{group.variants[0]}</strong><span>{p(group.documents===1?"1 documento":"{0} documentos").replace("{0}",String(group.documents))}</span></div>
            {#if group.variants.length>1}<details><summary>{p("{0} variantes").replace("{0}",String(group.variants.length))}</summary><p>{group.variants.join(" · ")}</p></details>{/if}
            <div class="shared-alias-edit">
              <input aria-label={p("Alias compartido para {0}").replace("{0}",group.variants[0])} value={aliasDrafts[group.id]??group.alias} oninput={e=>aliasDrafts[group.id]=e.currentTarget.value} spellcheck="false" maxlength="120" disabled={c.locked}/>
              <button disabled={c.locked||dirty||!aliasDrafts[group.id]?.trim()||aliasDrafts[group.id]===group.alias} onclick={()=>c.changeAlias({type:"rename",groupId:group.id,alias:aliasDrafts[group.id]})}><Icon name="check" size={18}/><span>{p("Guardar cambios")}</span></button>
            </div>
          </section>
        {/each}
        {#if !filteredAliases.length}<p>{p("No hay coincidencias.")}</p>{/if}
      </div>
    {:else if c.mode==="aliasChange"&&c.aliasPending}
      <p class="alias-impact">{p(c.aliasPending.impact.documents===1?"Se actualizará 1 documento.":"Se actualizarán {0} documentos.").replace("{0}",String(c.aliasPending.impact.documents))}</p>
      {#if aliasMerge&&mergeTarget}<p>{p("Las variantes usarán el alias {0}.").replace("{0}",mergeTarget.alias)}</p>{/if}
      <p>{p(c.aliasPending.impact.reviewed===1?"1 documento aprobado volverá a Por revisar.":"{0} documentos aprobados volverán a Por revisar.").replace("{0}",String(c.aliasPending.impact.reviewed))}</p>
      {#if c.aliasPending.impact.rebuilt}<p>{p("El cambio de valores privados reconstruirá el texto de {0} documentos; se perderán sus correcciones de texto.").replace("{0}",String(c.aliasPending.impact.rebuilt))}</p>{:else}<p>{p("Las correcciones guardadas se conservarán.")}</p>{/if}
      {#if c.aliasPending.impact.exported}<p>{p("Los PDF ya exportados no se actualizan. Exporta de nuevo los documentos afectados después de revisarlos.")}</p>{/if}
      {#if aliasMerge}<p>{p("Confirma únicamente si estas variantes representan a la misma persona. Compartir apellido no basta.")}</p>{/if}
    {/if}
    {#if c.mode==="prepare"||c.mode==="list"}
      <div class="batch-file-list">
        {#each c.view?.items??[] as item (item.id)}
          <div class="batch-file" class:current={item.id===c.view?.active}>
            {#if c.mode==="prepare"}<label><input type="checkbox" value={item.id} bind:group={c.selected} disabled={item.status==="error"||c.working}/><span>{item.fileName}</span></label>
            {:else}<button class="batch-file-name" disabled={c.locked||dirty||!item.sessionId} onclick={()=>c.select(item.id)}><Icon name="document" size={18}/><span>{item.fileName}</span></button>{/if}
            <span class="batch-file-status">{status(item)}</span>
            {#if item.error}<p class="batch-file-error">{p(item.error)}</p>{/if}
          </div>
        {/each}
      </div>
    {/if}
    {#if c.error}<p role="alert">{p(c.error)}</p>{/if}
    {#if c.mode==="prepare"}<p class="batch-selection-count">{p("{0} seleccionados").replace("{0}",String(c.selected.length))}</p>{/if}
    <div class="batch-dialog-actions" class:long-actions={c.mode==="export"||c.mode==="aliasChange"||c.mode==="list"}>
      {#if c.mode==="aliasChange"}<button onclick={()=>c.cancelAliasChange()} disabled={c.working}><Icon name="close" size={18}/>{p("Cancelar")}</button><button class="primary" onclick={()=>c.confirmAliasChange()} disabled={c.working}><Icon name="check" size={18}/>{p(aliasMerge?"Confirmar unión":"Actualizar documentos")}</button>
      {:else if c.mode==="aliases"}<button onclick={cancelDialog} disabled={c.working}><Icon name="check" size={18}/>{p("Listo")}</button>
      {:else if c.mode==="prepare"}<button onclick={cancelDialog} disabled={c.working}><Icon name="close" size={18}/>{p("Cancelar")}</button><button class="primary" onclick={()=>c.start(modelId())} disabled={!c.selected.length||c.working}><Icon name="search" size={18}/><span>{p("Detectar")}</span></button>
      {:else if c.mode==="export"}<button onclick={cancelDialog}><Icon name="close" size={18}/>{p("Cancelar")}</button><button class="primary" onclick={()=>c.export()} disabled={!c.approved||c.locked||dirty}><Icon name="folder" size={18}/>{p("Elegir carpeta y exportar")}</button>
      {:else if c.mode==="close"}<button onclick={cancelDialog}><Icon name="arrow-left" size={18}/>{p("Conservar lote")}</button><button class="primary" onclick={()=>c.close()} disabled={c.locked||dirty}><Icon name="close" size={18}/>{p("Cerrar lote")}</button>
      {:else}{#if pending}<button class="primary" disabled={c.locked||dirty} onclick={()=>c.start(modelId())}><Icon name="search" size={18}/>{p("Continuar detección")}</button>{/if}<button onclick={cancelDialog}><Icon name="check" size={18}/>{p("Listo")}</button>{/if}
    </div>
  </dialog>

<style>
  .batch-summary { flex:none; display:grid; gap:8px; padding:12px 0; text-align:center; border-bottom:1px solid var(--line); font-size:13px; }
  .batch-processing-label { display:flex; align-items:center; justify-content:center; gap:8px; }
  .processing-orbit { width:12px; height:12px; flex:none; border:1.5px solid var(--line-dark); border-top-color:var(--text); border-radius:50%; animation:processing-orbit 1.1s linear infinite; }
  .processing-elapsed { min-width:6ch; text-align:right; font-size:11px; font-variant-numeric:tabular-nums; color:#c5dbff; white-space:nowrap; }
  .processing-track { height:3px; overflow:hidden; background:var(--panel); }
  .processing-track span { display:block; width:35%; height:100%; background:var(--text); animation:processing 1.8s ease-in-out infinite alternate; }
  @keyframes processing-orbit { to { transform:rotate(360deg); } }
  @keyframes processing { to { transform:translateX(180%); } }
  @media(prefers-reduced-motion:reduce) { .processing-orbit { animation:none; } .processing-track span { animation:none; width:100%; } }
  .batch-summary strong { font-weight:600; }
  .batch-summary-line { display:flex; flex-wrap:wrap; align-items:center; justify-content:center; gap:8px 12px; color:var(--muted); }
  .batch-summary-line.batch-progress-actions { display:grid; grid-template-columns:minmax(0,1fr) auto auto; gap:8px; font-size:12px; }
  .batch-progress-actions > span:first-child { min-width:0; }
  .batch-summary-line button { display:inline-flex; align-items:center; justify-content:center; gap:6px; border:0; background:transparent; color:var(--text); text-decoration:underline; text-underline-offset:3px; padding:4px 8px; min-height:44px; font:inherit; cursor:pointer; }
  .batch-footer { flex:none; display:grid; gap:4px; width:100%; box-sizing:border-box; padding:8px; border:1px solid var(--line); border-radius:9px; text-align:center; }
  .batch-footer-status { color:var(--muted); font-size:12px; padding:4px; }
  .batch-footer-actions { display:grid; grid-template-columns:repeat(3,minmax(0,1fr)); gap:4px; }
  button { cursor:pointer; font:inherit; }
  button:disabled { cursor:default; opacity:.45; }
  .batch-footer-actions button { display:flex; align-items:center; justify-content:center; gap:6px; min-width:0; min-height:48px; border:0; background:transparent; color:var(--text); padding:6px 2px; font-size:12px; }
  .batch-dialog { width:min(640px,calc(100vw - 32px)); max-height:calc(100dvh - 40px); box-sizing:border-box; overflow:auto; padding:24px; color:var(--text); background:var(--modal-surface); border:1px solid var(--line); border-radius:16px; }
  .batch-dialog::backdrop { background:var(--modal-backdrop); backdrop-filter:blur(var(--modal-blur)); }
  header { display:flex; align-items:center; justify-content:space-between; gap:12px; }
  h2 { margin:0; font-size:18px; }
  p { font-size:13px; line-height:1.6; color:var(--muted); }
  .batch-close { display:flex; align-items:center; justify-content:center; gap:6px; flex:none; min-height:44px; padding:8px; font-size:12px; border:0; background:transparent; color:var(--text); }
  .batch-note { font-size:12px; }
  .batch-selection-count { margin:16px 0 0; text-align:center; font-variant-numeric:tabular-nums; }
  button :global(svg) { flex:none; }
  .batch-file-list { display:grid; margin-block:16px; }
  .batch-file { display:grid; grid-template-columns:minmax(0,1fr) auto; gap:8px; align-items:center; padding:12px 0; border-bottom:1px solid var(--line); }
  .batch-file.current { border-bottom-color:var(--text); }
  .batch-file label,.batch-file-name { display:flex; align-items:center; gap:10px; min-height:48px; min-width:0; font-size:13px; overflow-wrap:anywhere; }
  .batch-file label input { flex:none; width:18px; min-height:18px; height:18px; }
  .batch-file-name { text-align:left; border:0; padding:0; color:var(--text); background:transparent; }
  .batch-file-name span { min-width:0; }
  .batch-file-status { font-size:11px; color:var(--muted); }
  .batch-file-error { grid-column:1/-1; margin:0; }
  .alias-search { display:flex; align-items:center; gap:8px; margin:16px 0; }
  .alias-search input { flex:1; min-width:0; min-height:48px; padding:10px 12px; font:inherit; }
  .alias-suggestions { border:1px solid var(--line); border-radius:9px; padding:12px; margin:16px 0; }
  .alias-suggestions h3 { font-size:13px; margin:0 0 8px; }
  .alias-suggestion { display:grid; gap:8px; padding:12px 0; border-top:1px solid var(--line); font-size:13px; line-height:1.5; }
  .alias-suggestion button,.shared-alias-edit button { display:flex; align-items:center; justify-content:center; gap:8px; min-height:48px; padding:8px 12px; border:1px solid var(--line); border-radius:9px; background:transparent; color:var(--text); font-size:12px; }
  .shared-alias-list { display:grid; gap:12px; }
  .shared-identity { border-bottom:1px solid var(--line); padding:12px 0 20px; }
  .shared-identity-heading { display:flex; justify-content:space-between; align-items:baseline; flex-wrap:wrap; gap:8px; font-size:13px; margin-bottom:12px; }
  .shared-identity-heading strong { overflow-wrap:anywhere; }
  .shared-identity-heading>span,.shared-identity summary { color:var(--muted); font-size:12px; }
  .shared-identity summary { cursor:pointer; margin-bottom:12px; }
  .shared-alias-edit { display:grid; grid-template-columns:minmax(0,1fr) auto; gap:12px; }
  .shared-alias-edit input { box-sizing:border-box; width:100%; min-width:0; min-height:48px; padding:10px 12px; font:inherit; font-size:13px; }
  .alias-impact { font-weight:600; color:var(--text); }
  .batch-dialog-actions { display:grid; grid-template-columns:repeat(2,minmax(0,1fr)); gap:12px; margin-top:20px; }
  .batch-dialog-actions button { box-sizing:border-box; min-width:0; white-space:nowrap; display:flex; align-items:center; justify-content:center; gap:8px; min-height:48px; padding:8px 12px; border:1px solid var(--line); border-radius:9px; background:transparent; color:var(--text); font-size:13px; }
  .batch-dialog-actions .primary { background:var(--button); color:var(--button-text); border-color:var(--button); }
  button:focus-visible { outline:2px solid var(--text); outline-offset:3px; }
  .batch-dialog-actions > button:only-child { grid-column:1/-1; }
  @media(max-width:480px) {
    .batch-dialog { padding:16px; }
    .batch-dialog-actions.long-actions { grid-template-columns:minmax(0,1fr); }
    .shared-alias-edit { grid-template-columns:minmax(0,1fr); }
    .batch-summary-line.batch-progress-actions { grid-template-columns:minmax(0,1fr) auto; }
    .batch-progress-actions > span:first-child { grid-column:1/-1; }
  }
</style>
