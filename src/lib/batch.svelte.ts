import { invoke } from "@tauri-apps/api/core";
import type { PrivacySession, PrivateEntity, EntityKind } from "./privacy";
import { privateCopy as p } from "./private-i18n";
export interface BatchItem { id:string; fileName:string; status:"pending"|"processing"|"review"|"approved"|"exported"|"error"|"cancelled"; error:string; sessionId:string|null; }
export interface BatchView { id:string; items:BatchItem[]; active:string|null; processing:boolean; started:boolean; }
export interface BatchIdentity { id:string;kind:EntityKind;variants:string[];alias:string;documents:number;reviewed:number; }
export interface AliasCatalog { groups:BatchIdentity[];suggestions:[string,string][]; }
type AliasAction = {type:"map";entities:PrivateEntity[]}|{type:"rename";groupId:string;alias:string}|{type:"merge";sourceId:string;targetId:string};
interface AliasImpact { documents:number;reviewed:number;exported?:number;rebuilt?:number;session:PrivacySession|null; }
interface AliasRequest {batchId:string;id:string;revision:number;action:AliasAction;commit:boolean;}
export class BatchController {
  view = $state<BatchView|null>(null);
  running = $state(false);
  working = $state(false);
  mode = $state<"prepare"|"list"|"export"|"close"|"aliases"|"aliasChange"|null>(null);
  selected = $state<string[]>([]);
  aliases = $state<AliasCatalog>({groups:[],suggestions:[]});
  aliasPending = $state<{request:AliasRequest;impact:AliasImpact;returnTo:"aliases"|null}|null>(null);
  private aliasResolve:((session:PrivacySession|null)=>void)|null=null;
  error = $state("");
  notice = $state("");
  readonly locked = $derived(this.running || this.working);
  readonly approved = $derived(this.view?.items.filter(i=>i.status==="approved"||i.status==="exported").length ?? 0);
  readonly reviewable = $derived(this.view?.items.filter(i=>!!i.sessionId) ?? []);
  readonly index = $derived(this.view?.items.findIndex(i=>i.id===this.view?.active) ?? -1);
  readonly ready = $derived(this.view?.items.filter(i=>i.status==="review").length ?? 0);
  constructor(private activate:(session:PrivacySession|null)=>void, private dirty:()=>boolean, private missingModel:()=>void, private currentSession:()=>PrivacySession|null=()=>null) {}
  async loadAliases() {
    if(!this.view?.started||this.running)return;
    try {this.aliases=await invoke<AliasCatalog>("get_private_batch_aliases");}catch(e){this.error=String(e);}
  }
  async openAliases() {
    if(this.locked||this.dirty())return;
    this.error="";await this.loadAliases();this.mode="aliases";
  }
  async changeAlias(action:AliasAction,alwaysConfirm=true):Promise<PrivacySession|null> {
    const session=this.currentSession();if(!session||!this.view||this.locked)return null;
    this.error="";this.working=true;
    const request:AliasRequest={batchId:this.view.id,id:session.id,revision:session.revision,action,commit:false};
    try {
      const impact=await invoke<AliasImpact>("change_private_batch_aliases",{...request});
      if(alwaysConfirm||impact.documents>1) {
        const returnTo=action.type==="map"?null:"aliases";
        this.aliasPending={request,impact,returnTo};this.mode="aliasChange";
        return await new Promise<PrivacySession|null>(resolve=>{this.aliasResolve=resolve;this.working=false;});
      }
      const result=await invoke<AliasImpact>("change_private_batch_aliases",{...request,commit:true});
      return result.session;
    }catch(e){this.error=String(e);return null;}finally{this.working=false;await this.refresh();}
  }
  cancelAliasChange() {
    const back=this.aliasPending?.returnTo??null;this.aliasPending=null;this.mode=back;
    this.aliasResolve?.(null);this.aliasResolve=null;
  }
  async confirmAliasChange() {
    if(!this.aliasPending||this.working)return;
    const pending=this.aliasPending;this.working=true;
    try {
      const result=await invoke<AliasImpact>("change_private_batch_aliases",{...pending.request,commit:true});
      this.aliasPending=null;this.mode=pending.returnTo;
      if(pending.request.action.type!=="map"&&result.session)this.activate(result.session);
      await this.loadAliases();
      this.aliasResolve?.(result.session);this.aliasResolve=null;
    }catch(e){this.error=String(e);}finally{this.working=false;}
  }
  async restore() {
    this.working=true;
    try {
      this.view=await invoke<BatchView|null>("get_private_batch");
      if(this.view?.processing) {
        this.running=true;
        // A WebView reload must not expose a document still owned by native
        // inference/export. Let that operation finish before offering review.
        while(this.view?.processing) {
          await new Promise(resolve=>setTimeout(resolve,500));
          this.view=await invoke<BatchView|null>("get_private_batch");
        }
      }
      if(this.view&&!this.view.started) {
        this.selected=this.view.items.filter(i=>i.status==="pending").map(i=>i.id);
        this.mode="prepare";
      } else if(this.view?.items.some(i=>i.status==="pending")) this.mode="list";
    } catch { /* No batch is available if the optional recovery call fails. */ }
    finally {this.running=false;this.working=false;}
    if(this.view?.started) {
      const current=this.view.items.find(i=>i.id===this.view?.active&&i.sessionId) ?? this.view.items.find(i=>i.sessionId);
      if(current)await this.select(current.id);
    }
  }
  async refresh() {
    if(!this.view || this.locked) return;
    try { const id=this.view.id; const next=await invoke<BatchView|null>("get_private_batch"); if(this.view?.id===id) this.view=next; } catch(e) {this.error=String(e);}
  }
  async choose() {
    if(this.locked || this.dirty() || this.view) return;
    this.working=true;this.error="";this.notice="";
    try {
      this.view=await invoke<BatchView|null>("choose_private_batch");
      if(this.view) {this.selected=this.view.items.filter(i=>i.status==="pending").map(i=>i.id);this.mode="prepare";}
    } catch(e){this.error=String(e);} finally{this.working=false;}
  }
  async start(modelId:string) {
    if(!this.view || this.locked || (!this.view.started&&!this.selected.length)) return;
    this.working=true;this.error="";
    try {
      const all=await invoke<{privacy:{id:string;installed:boolean}[]}>("list_models");
      if(!all.privacy.some(m=>m.id===modelId&&m.installed)) {this.mode=null;this.missingModel();return;}
      if(!this.view.started)this.view=await invoke<BatchView>("start_private_batch",{id:this.view.id,selected:[...this.selected]});
    }catch(e){this.error=String(e);return;}finally{this.working=false;}
    this.mode=null;this.running=true;
    try {
      while(this.view?.items.some(i=>i.status==="pending")) {
        const item:BatchItem=this.view.items.find(i=>i.status==="pending")!;
        // Show progress immediately; results carry no document text until selection.
        this.view={...this.view,items:this.view.items.map(i=>i.id===item.id?{...i,status:"processing"}:i)};
        this.view=await invoke<BatchView>("process_private_batch_item",{batchId:this.view.id,itemId:item.id,modelId});
      }
    }catch(e){this.error=String(e);}finally{
      this.running=false;
      await this.refresh();
      const first=this.view?.items.find(i=>i.sessionId);
      if(first) await this.select(first.id);
      else this.mode="list";
    }
  }
  async cancelPending() {
    if(!this.view) return;
    try {
      const next=await invoke<BatchView>("cancel_private_batch_pending");
      // Do not let a late cancellation snapshot replace a completed item.
      const cancelled=new Set(next.items.filter(i=>i.status==="cancelled").map(i=>i.id));
      this.view={...this.view,items:this.view.items.map(i=>cancelled.has(i.id)&&i.status==="pending"?{...i,status:"cancelled"}:i)};
    }catch(e){this.error=String(e);}
  }
  async select(id:string) {
    if(this.locked || this.dirty()) return;
    this.working=true;this.error="";
    try {const session=await invoke<PrivacySession>("select_private_batch_item",{id});this.activate(session);this.mode=null;}
    catch(e){this.error=String(e);}finally{this.working=false;await this.refresh();}
  }
  async navigate(delta:number) {
    if(!this.view) return;
    const current=this.reviewable.findIndex(i=>i.id===this.view?.active);
    const next=this.reviewable[current+delta];if(next)await this.select(next.id);
  }
  async close() {
    if(this.locked || this.dirty())return;
    this.working=true;
    try {const previous=await invoke<PrivacySession|null>("close_private_batch");this.view=null;this.mode=null;this.aliases={groups:[],suggestions:[]};this.notice="";this.activate(previous);}
    catch(e){this.error=String(e);}finally{this.working=false;}
  }
  async export() {
    if(this.locked || this.dirty() || !this.approved)return;
    this.mode=null;this.working=true;this.error="";
    try {
      const report=await invoke<{exported:number;failed:number}|null>("export_private_batch",{lang:document.documentElement.lang});
      if(report)this.notice=p("Exportados: {0} · Errores: {1}").replace("{0}",String(report.exported)).replace("{1}",String(report.failed));
    }catch(e){this.error=String(e);}finally{this.working=false;await this.refresh();}
  }
}
