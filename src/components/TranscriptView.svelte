<script lang="ts">
  import { privateCopy as p } from "../lib/private-i18n";
  import { tick } from "svelte";
  import type { Transcript, DocFormat } from "../lib/types";
  import { DEFAULT_FORMAT, FONT_OPTIONS, SIZE_OPTIONS } from "../lib/types";
  import { formatDuration, sanitizeRichHtml } from "../lib/utils";
  import { getCopy } from "../lib/i18n-store.svelte";
  import { invoke } from "@tauri-apps/api/core";

  let t = $derived(getCopy());

  interface Props {
    transcript: Transcript;
    onSave: (
      id: string,
      text: string,
      format?: DocFormat,
      html?: string | null,
      speakerNames?: Record<string, string>
    ) => void | Promise<void>;
    onDirtyChange: (dirty: boolean, save: () => void | Promise<void>) => void;
    privateDocument?: boolean;
    privateReviewed?: boolean;
    privateId?: string | null;
    privateRevision?: number | null;
    onBackToComparison?: () => void;
    onConfirmReview?: () => Promise<void>;
    onValidateExport?: (text: string, title?: string) => Promise<void>;
  }

  let { transcript, onSave, onDirtyChange, privateDocument = false, privateReviewed = false, privateId = null, privateRevision = null, onBackToComparison, onConfirmReview, onValidateExport }: Props = $props();
  let exporting = $state(false);
  let statusMessage = $state("");
  let correctedText = $state("");
  let savedCorrection = $state(false);
  let copied = $state(false);
  let copyTimer: ReturnType<typeof setTimeout> | undefined;
  // Snapshot de HTML para Deshacer (recuerda un par de pasos) y Reset a la
  // transcripción original tal como vino de la grabación.
  const UNDO_DEPTH = 3;
  let history = $state<string[]>([]);
  let speakerNames = $state<Record<string, string>>({});
  let savedSnapshot = $state("");
  const speakerIds = $derived(
    [...new Set(transcript.segments.flatMap((segment) => segment.speaker == null ? [] : [segment.speaker]))]
      .sort((a, b) => a - b)
  );
  const hasSpeakers = $derived(speakerIds.length > 0);
  const originalText = $derived(htmlToPlain(conversationHtml(
    textToHtml(normalizeSpeakerText(transcript.text, transcript.segments, speakerNames)),
    speakerNames
  )));
  let canUndo = $derived(history.length > 0);
  let canReset = $derived(savedCorrection || correctedText.trim() !== originalText.trim());
  /** Formato de DOCUMENTO (fuente/tamaño/alineación generales) → PDF y vista base. */
  let docFormat = $state<DocFormat>(DEFAULT_FORMAT);

  // --- Editor ricotexto: B/I/U/color actúan sobre la selección ---
  let editorEl = $state<HTMLDivElement | null>(null);
  let prevEditorEl: HTMLDivElement | null = null;
  let rich = $state("");
  let loading = $state(true);
  let selBold = $state(false);
  let selItalic = $state(false);
  let selUnderline = $state(false);
  let selColor = $state("");
  let lastRange: Range | null = null;

  // --- Regla de márgenes/sangrías (aplicada al párrafo / zona seleccionada) ---
  let rulerRef = $state<HTMLDivElement | null>(null);
  let privateRulerStyle = $state("");
  $effect(() => {
    if (!privateDocument || !editorEl || !rulerRef) return;
    const editor = editorEl;
    const ruler = rulerRef;
    function alignRuler() {
      const paper = editor.getBoundingClientRect();
      const container = ruler.parentElement!;
      const css = getComputedStyle(editor);
      const left = parseFloat(css.paddingLeft);
      const right = parseFloat(css.paddingRight);
      privateRulerStyle = `width:${paper.width - left - right}px; margin-left:${paper.left + left - container.getBoundingClientRect().left - container.clientLeft}px; margin-right:0;`;
    }
    const observer = new ResizeObserver(alignRuler);
    observer.observe(editor);
    observer.observe(editor.parentElement!);
    alignRuler();
    return () => observer.disconnect();
  });
  let curPL = $state(0);
  let curTI = $state(0);
  let curPR = $state(0);
  const RULER_LABELS = [0, 2, 4, 6, 8, 10, 12, 14, 16, 18, 20];

  // Al cambiar de transcripción, se restaura el estado guardado (o el original).
  // OJO: NO debe leer `correctedText` ni `rich` (serían dependencias implícitas
  // que re-ejecutarían este efecto en cada tecleo, revirtiendo la edición).
  $effect(() => {
    const names = { ...(transcript.speakerNames ?? {}) };
    const savedHtml = transcript.correctedHtml ?? null;
    const saved = transcript.corrected !== null && transcript.corrected !== undefined;
    const base = saved ? transcript.corrected! : normalizeSpeakerText(transcript.text, transcript.segments, names);
    const format = transcript.format ?? DEFAULT_FORMAT;
    const sourceHtml = sanitizeRichHtml(savedHtml || textToHtml(base));
    const initialRich = conversationHtml(sourceHtml, names);
    speakerNames = names;
    correctedText = htmlToPlain(initialRich);
    savedCorrection = saved && initialRich === sourceHtml;
    docFormat = format;
    rich = initialRich;
    savedSnapshot = editorSnapshot(saved ? sourceHtml : initialRich, format, names);
    history = [];
    loading = true;
  });

  function editorSnapshot(html: string, format: DocFormat, names: Record<string, string>): string {
    return JSON.stringify({ html, format, names });
  }

  const dirty = $derived(savedSnapshot !== "" && editorSnapshot(rich, docFormat, speakerNames) !== savedSnapshot);
  $effect(() => {
    onDirtyChange(dirty, saveCorrection);
  });

  // Escribe el contenido en el editor cuando se monta o cambia la transcripción,
  // y lo deja editable/focalizado. No re-escribe mientras el usuario escribe.
  $effect(() => {
    if (editorEl && (editorEl !== prevEditorEl || loading)) {
      editorEl.innerHTML = rich;
      editorEl.contentEditable = "true";
      loading = false;
      refreshSelState();
      refreshIndents();
      editorEl.focus();
      const range = document.createRange();
      range.selectNodeContents(editorEl);
      range.collapse(false);
      const s = window.getSelection();
      if (s) {
        s.removeAllRanges();
        s.addRange(range);
      }
    }
    prevEditorEl = editorEl;
  });

  const fontCss = $derived(
    FONT_OPTIONS.find((f) => f.id === docFormat.font)?.css ?? FONT_OPTIONS[0].css
  );

  const editorStyle = $derived(
    `font-family: ${fontCss}; font-size: ${docFormat.size}px; text-align: ${docFormat.align};` +
      ` user-select: text; -webkit-user-select: text; -webkit-user-modify: read-write;`
  );

  const displayColor = $derived(toHex(selColor));

  // Cambia el formato de DOCUMENTO (fuente/tamaño/alineación) y re-habilita Guardar.
  function setDocFormat(patch: Partial<DocFormat>) {
    docFormat = { ...docFormat, ...patch };
    if (savedCorrection) savedCorrection = false;
  }

  function escapeHtml(s: string): string {
    return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  }
  function textToHtml(s: string): string {
    return s
      .split("\n")
      .map((l) => `<div>${escapeHtml(l)}</div>`)
      .join("");
  }
  function htmlToPlain(html: string): string {
    return html
      .replace(/<br\s*\/?>/gi, "\n")
      .replace(/<\/(?:div|p)>\s*<(?:div|p)[^>]*>/gi, "\n")
      .replace(/<[^>]+>/g, "")
      .replace(/&nbsp;/gi, " ")
      .replace(/&amp;/g, "&")
      .replace(/&lt;/g, "<")
      .replace(/&gt;/g, ">")
      .replace(/\n{3,}/g, "\n\n")
      .trim();
  }
  function defaultSpeakerLabel(speaker: number): string {
    return `Hablante ${speaker}`;
  }

  function speakerLabel(speaker: number, names: Record<string, string>): string {
    return names[String(speaker)]?.trim() || defaultSpeakerLabel(speaker);
  }

  function speakerPrefixCandidates(
    speaker: number,
    names: Record<string, string>,
    previousNames: Record<string, string>
  ): string[] {
    return [
      defaultSpeakerLabel(speaker),
      `Speaker ${speaker}`,
      names[String(speaker)],
      previousNames[String(speaker)],
    ]
      .filter((label): label is string => Boolean(label?.trim()))
      .sort((a, b) => b.length - a.length);
  }

  function findSpeakerPrefix(
    line: string,
    names: Record<string, string>,
    previousNames: Record<string, string>
  ): { speaker: number; content: string } | null {
    const defaultMatch = line.match(/^(?:Hablante|Speaker)\s+(\d+)\s*:\s*/i);
    if (defaultMatch) {
      return { speaker: Number(defaultMatch[1]), content: line.slice(defaultMatch[0].length) };
    }
    for (const speaker of new Set([
      ...Object.keys(names),
      ...Object.keys(previousNames),
    ].map((key) => Number(key)).filter((key) => Number.isFinite(key)))) {
      for (const label of speakerPrefixCandidates(speaker, names, previousNames)) {
        const prefix = `${label}:`;
        if (line.startsWith(prefix)) {
          return { speaker, content: line.slice(prefix.length).replace(/^\s+/, "") };
        }
      }
    }
    return null;
  }

  function stripSpeakerPrefix(
    line: string,
    speaker: number,
    names: Record<string, string>,
    previousNames: Record<string, string>
  ): string {
    for (const label of speakerPrefixCandidates(speaker, names, previousNames)) {
      const prefix = `${label}:`;
      if (line.startsWith(prefix)) return line.slice(prefix.length).replace(/^\s+/, "");
    }
    return line;
  }

  function normalizeSpeakerText(
    text: string,
    segments: Transcript["segments"],
    names: Record<string, string>,
    previousNames: Record<string, string> = names
  ): string {
    if (!segments.some((segment) => segment.speaker != null)) return text;
    let previousSpeaker: number | null = null;
    return text
      .split("\n")
      .map((line, index) => {
        if (!line.trim()) return line;
        const detected = findSpeakerPrefix(line, names, previousNames);
        const speaker = detected?.speaker ?? segments[index]?.speaker;
        if (speaker == null) return line;
        const content = stripSpeakerPrefix(line, speaker, names, previousNames);
        if (previousSpeaker === speaker) return content;
        previousSpeaker = speaker;
        return `${speakerLabel(speaker, names)}: ${content}`;
      })
      .join("\n");
  }

  // Convert old inline speaker prefixes once, retaining inline formatting in
  // the body. Already structured documents retain their user-edited indents.
  function conversationHtml(html: string, names: Record<string, string>): string {
    if (!hasSpeakers) return html;
    const container = document.createElement("div");
    container.innerHTML = html;
    const blocks = Array.from(container.children) as HTMLElement[];
    if (!blocks.some((block) => findSpeakerPrefix(block.textContent ?? "", names, names)?.content)) {
      return html;
    }
    for (const block of blocks) {
      const line = block.textContent ?? "";
      const detected = findSpeakerPrefix(line, names, names);
      if (detected) {
        const heading = document.createElement("div");
        const label = document.createElement("strong");
        label.textContent = `${speakerLabel(detected.speaker, names)}:`;
        heading.append(label);
        block.before(heading);
        // Delete only the prefix, even when split across styled text nodes.
        let remaining = line.length - detected.content.length;
        for (const node of textNodes(block)) {
          const count = Math.min(remaining, node.length);
          node.deleteData(0, count);
          remaining -= count;
          if (!remaining) break;
        }
        if (!detected.content) {
          block.remove();
          continue;
        }
      }
      if (!block.style.paddingLeft) block.style.paddingLeft = "48px";
    }
    return container.innerHTML;
  }

  function textNodes(element: Element): Text[] {
    const walker = document.createTreeWalker(element, NodeFilter.SHOW_TEXT);
    const nodes: Text[] = [];
    let current = walker.nextNode();
    while (current) {
      if (current.textContent) nodes.push(current as Text);
      current = walker.nextNode();
    }
    return nodes;
  }

  function renameSpeakerHtml(
    html: string,
    names: Record<string, string>,
    previousNames: Record<string, string>
  ): string {
    const container = document.createElement("div");
    container.innerHTML = html;
    Array.from(container.children).forEach((block) => {
      const nodes = textNodes(block);
      if (nodes.length === 0) return;
      for (const node of nodes) {
        const detected = findSpeakerPrefix(node.data, names, previousNames);
        if (detected) {
          node.data = `${speakerLabel(detected.speaker, names)}:${detected.content ? ` ${detected.content}` : ""}`;
          break;
        }
      }
    });
    return container.innerHTML;
  }

  function toHex(v: string): string {
    const m = v && v.match(/^rgb\(\s*(\d+),\s*(\d+),\s*(\d+)\s*\)$/);
    if (m) {
      return (
        "#" +
        [1, 2, 3].map((i) => (+m[i]).toString(16).padStart(2, "0")).join("")
      );
    }
    if (v && /^#[0-9a-fA-F]{6}$/.test(v)) return v;
    return document.documentElement.dataset.theme === "light" ? "#202020" : "#ededed";
  }

  function onEditorInteract() {
    if (editorEl) editorEl.contentEditable = "true";
    const sel = window.getSelection();
    if (editorEl && sel && sel.rangeCount > 0 && editorEl.contains(sel.anchorNode)) {
      lastRange = sel.getRangeAt(0).cloneRange();
      selBold = document.queryCommandState("bold");
      selItalic = document.queryCommandState("italic");
      selUnderline = document.queryCommandState("underline");
      selColor = String(document.queryCommandValue("foreColor") ?? "");
    } else {
      selBold = selItalic = selUnderline = false;
      selColor = "";
    }
    refreshIndents();
  }

  function refreshSelState() {
    const sel = window.getSelection();
    if (editorEl && sel && sel.rangeCount > 0 && editorEl.contains(sel.anchorNode)) {
      selBold = document.queryCommandState("bold");
      selItalic = document.queryCommandState("italic");
      selUnderline = document.queryCommandState("underline");
    }
  }

  function remember(prev: string) {
    history.push(prev);
    if (history.length > UNDO_DEPTH) history.shift();
  }

  function syncFromEditor() {
    if (editorEl) {
      editorEl.contentEditable = "true";
      const current = editorEl.innerHTML;
      const next = sanitizeRichHtml(current);
      if (next !== current) editorEl.innerHTML = next;
      const prev = rich;
      rich = next;
      if (rich !== prev) remember(prev);
      correctedText = htmlToPlain(rich);
    }
    if (savedCorrection) savedCorrection = false;
  }

  function onPaste(event: ClipboardEvent) {
    const clipboard = event.clipboardData;
    if (!clipboard) return;
    const html = clipboard.getData("text/html");
    const text = clipboard.getData("text/plain");
    if (!html && !text) return;
    event.preventDefault();
    const content = html
      ? sanitizeRichHtml(html)
      : text.split(/\r?\n/).map((line) => escapeHtml(line)).join("<br>");
    document.execCommand("insertHTML", false, content);
    syncFromEditor();
  }

  function applyFormat(cmd: string, value?: string) {
    if (!editorEl) return;
    editorEl.contentEditable = "true";
    // Captura la selección actual del editor (antes de tocar el foco) para no
    // aplicar el comando a todo el documento.
    const sel = window.getSelection();
    if (sel && sel.rangeCount > 0 && editorEl.contains(sel.anchorNode)) {
      lastRange = sel.getRangeAt(0).cloneRange();
    }
    editorEl.focus();
    if (lastRange) {
      const s = window.getSelection();
      if (s) {
        s.removeAllRanges();
        s.addRange(lastRange.cloneRange());
      }
    }
    document.execCommand("styleWithCSS", false, "true");
    document.execCommand(cmd, false, value);
    syncFromEditor();
    refreshSelState();
  }

  function hasSelection(): boolean {
    const sel = window.getSelection();
    return !!(
      editorEl &&
      sel &&
      sel.rangeCount > 0 &&
      !sel.isCollapsed &&
      editorEl.contains(sel.anchorNode)
    );
  }

  // El toolbox debe actuar sobre la selección hecha en el editor, incluso si al
  // abrir un dropdown (fuente/tamaño) el navegador colapsa la selección viva:
  // usa la captura previa (`lastRange`) hecha al interactuar en el editor.
  function inEditorSelection(): boolean {
    return (
      hasSelection() ||
      !!(
        lastRange &&
        editorEl &&
        editorEl.contains(lastRange.commonAncestorContainer)
      )
    );
  }

  // Si el clic va fuera del editor y de sus toolbars, la selección previa deja
  // de aplicarse (evita aplicar formato a una selección antigua).
  function onGlobalPointerDown(e: Event) {
    const t = e.target as Node;
    if (editorEl && editorEl.contains(t)) return;
    if (t instanceof Element && t.closest(".editor-toolbar")) return;
    if (t instanceof Element && t.closest(".ruler")) return;
    lastRange = null;
  }

  // Mapea tamaños px a los niveles 1-7 de execCommand (aprox., para la selección).
  function sizeToLevel(px: number): string {
    if (px <= 11) return "1";
    if (px <= 13) return "2";
    if (px <= 15) return "3";
    if (px <= 18) return "4";
    if (px <= 22) return "5";
    if (px <= 30) return "6";
    return "7";
  }

  // --- Regla de márgenes/sangrías ---
  function currentBlock(): HTMLElement | null {
    if (!editorEl) return null;
    const sel = window.getSelection();
    let el: HTMLElement | null = null;
    if (sel && sel.anchorNode) {
      const n = sel.anchorNode;
      el = n.nodeType === Node.ELEMENT_NODE ? (n as HTMLElement) : (n.parentElement as HTMLElement);
      while (el && el.parentElement !== editorEl) el = el.parentElement;
      if (!el || el.parentElement !== editorEl) el = null;
    }
    return el ?? (editorEl.firstElementChild as HTMLElement | null);
  }

  function affectedBlocks(): HTMLElement[] {
    if (!editorEl) return [];
    const sel = window.getSelection();
    if (!sel || sel.rangeCount === 0) return [];
    const range = sel.getRangeAt(0);
    const blocks: HTMLElement[] = [];
    for (const child of Array.from(editorEl.children)) {
      const el = child as HTMLElement;
      if (range.intersectsNode(el)) blocks.push(el);
    }
    return blocks;
  }

  function readIndents(el: HTMLElement): { pl: number; ti: number; pr: number } {
    return {
      pl: parseFloat(el.style.paddingLeft) || 0,
      ti: parseFloat(el.style.textIndent) || 0,
      pr: parseFloat(el.style.paddingRight) || 0,
    };
  }

  function refreshIndents() {
    const b = affectedBlocks()[0] ?? currentBlock();
    if (b) {
      const i = readIndents(b);
      curPL = i.pl;
      curTI = i.ti;
      curPR = i.pr;
    } else {
      curPL = 0;
      curTI = 0;
      curPR = 0;
    }
  }

  function setBlockIndent(b: HTMLElement, pl: number, ti: number, pr: number) {
    const width = rulerRef?.clientWidth ?? 660;
    pl = Math.max(0, Math.min(width - 40, pl));
    pr = Math.max(0, Math.min(width - pl - 40, pr));
    ti = Math.max(-pl, Math.min(width - pl - pr - 20, ti));
    b.style.paddingLeft = pl > 0 ? `${pl}px` : "";
    b.style.textIndent = ti !== 0 ? `${ti}px` : "";
    b.style.paddingRight = pr > 0 ? `${pr}px` : "";
  }

  function nudge(kind: "left" | "first" | "right", dir: 1 | -1) {
    const b = currentBlock();
    if (!b) return;
    const i = readIndents(b);
    let pl = i.pl;
    let ti = i.ti;
    let pr = i.pr;
    if (kind === "left") pl = Math.max(0, pl + dir * 5);
    else if (kind === "first") ti += dir * 5;
    else pr = Math.max(0, pr + dir * 5);
    setBlockIndent(b, pl, ti, pr);
    refreshIndents();
    syncFromEditor();
  }

  function dragRuler(kind: "left" | "first" | "right", e: PointerEvent) {
    e.preventDefault();
    const r = rulerRef;
    if (!r) return;
    const rect = r.getBoundingClientRect();
    const blocks = affectedBlocks();
    const list = blocks.length ? blocks : (currentBlock() ? [currentBlock()!] : []);
    if (list.length === 0) return;
    const start = readIndents(list[0]);
    const max = rect.width;
    function move(ev: PointerEvent) {
      const pos = Math.round(Math.max(0, Math.min(max, ev.clientX - rect.left)));
      let pl = start.pl;
      let ti = start.ti;
      let pr = start.pr;
      if (kind === "left") pl = pos;
      else if (kind === "first") ti = pos - start.pl;
      else if (kind === "right") pr = Math.round(Math.max(0, max - pos));
      pl = Math.max(0, Math.min(max, pl));
      ti = Math.max(-max, Math.min(max, ti));
      pr = Math.max(0, Math.min(max, pr));
      for (const b of list) {
        setBlockIndent(b, pl, ti, pr);
      }
      refreshIndents();
      syncFromEditor();
    }
    function up() {
      document.removeEventListener("pointermove", move);
      document.removeEventListener("pointerup", up);
    }
    document.addEventListener("pointermove", move);
    document.addEventListener("pointerup", up);
  }

  // --- PDF ---
  async function exportAsPDF() {
    if (exporting) return;
    exporting = true;
    statusMessage = "";
    const exportHtml = sanitizeRichHtml(rich);
    const exportText = htmlToPlain(exportHtml);
    const exportFormat = { ...docFormat };
    try {
      if (privateDocument) {
        // El escritor vuelve a validar en Rust contra la sesión revisada, así
        // que solo puede exportarse exactamente el texto ya revisado.
        if (!privateId) throw new Error(p("No se pudo identificar la sesión privada revisada."));
        if (dirty) {
          await saveCorrection();
          await tick();
          if (dirty) throw new Error(statusMessage || p("No se pudieron guardar los cambios."));
        }
        if (!onValidateExport) throw new Error(p("No se pudo validar el documento privado."));
        if (!privateReviewed) {
          if (!onConfirmReview) throw new Error(p("No se pudo validar el documento privado."));
          await onConfirmReview();
          await tick();
        }
        await onValidateExport(exportText, transcript.title.replace(/^Documento sanitizado/, p("Documento sanitizado")));
      }
      const exportTitle = privateDocument ? transcript.title.replace(/^Documento sanitizado/, p("Documento sanitizado")) : transcript.title;
      const base = (exportTitle || "transcripcion").replace(/[\\/:*?"<>|]/g, "-");
      const path = await invoke<string | null>("choose_pdf_destination", {
        defaultName: `${base}.pdf`,
      });
      if (!path) return;
      const saved = await invoke<string>("generate_pdf", {
        title: exportTitle,
        segments: transcript.segments,
        language: transcript.language,
        model: transcript.model,
        duration: transcript.duration,
        destPath: path,
        text: exportText,
        html: exportHtml,
        speakerNames,
        font: exportFormat.font,
        size: exportFormat.size,
        align: exportFormat.align,
        private: privateDocument && privateId ? { id: privateId, revision: privateRevision ?? 0 } : null,
      });
      statusMessage = t.pdfSaved(saved);
    } catch (e: any) {
      console.error(e);
      statusMessage = String(e);
    } finally {
      exporting = false;
    }
  }

  async function saveCorrection() {
    if (savedCorrection) return;
    const safeHtml = sanitizeRichHtml(rich);
    rich = safeHtml;
    correctedText = htmlToPlain(safeHtml);
    try { await onSave(transcript.id, correctedText, docFormat, safeHtml, speakerNames); }
    catch (e) { statusMessage = String(e); return; }
    savedSnapshot = editorSnapshot(safeHtml, docFormat, speakerNames);
    savedCorrection = true;
  }

  // Renombra un hablante y reescribe los prefijos del editor en el acto,
  // conservando el formato rico y el resto de la transcripción.
  function updateSpeakerName(speaker: number, value: string) {
    const previousNames = speakerNames;
    const nextNames = { ...speakerNames };
    const trimmed = value.trim();
    if (trimmed) nextNames[String(speaker)] = trimmed;
    else delete nextNames[String(speaker)];
    speakerNames = nextNames;
    const rebuilt = sanitizeRichHtml(
      renameSpeakerHtml(rich, nextNames, previousNames)
    );
    rich = rebuilt;
    correctedText = htmlToPlain(rebuilt);
    history = [];
    savedCorrection = false;
    loading = true;
  }

  function copyToClipboard() {
    navigator.clipboard.writeText(correctedText).catch(() => {});
    copied = true;
    clearTimeout(copyTimer);
    copyTimer = setTimeout(() => (copied = false), 1200);
  }

  // Deshace el último paso del editor (restaura el snapshot previo guardado).
  function undo() {
    const prev = history.pop();
    if (prev === undefined) return;
    rich = prev;
    correctedText = htmlToPlain(prev);
    savedCorrection = false;
    loading = true;
    requestAnimationFrame(() => editorEl?.focus());
  }

  // Ctrl/Cmd+Z = Deshacer del editor (hueco nativo del contenteditable).
  function onUndoKeydown(e: KeyboardEvent) {
    if ((e.metaKey || e.ctrlKey) && !e.shiftKey && !e.altKey && e.key.toLowerCase() === "z") {
      e.preventDefault();
      undo();
    }
  }

  // Vuelve el editor al texto original tal como salió de la transcripción,
  // descartando correcciones y pasos del historial.
  function resetToOriginal() {
    const original = normalizeSpeakerText(transcript.text, transcript.segments, speakerNames);
    rich = conversationHtml(textToHtml(original), speakerNames);
    correctedText = htmlToPlain(rich);
    savedCorrection = false;
    history = [];
    loading = true;
    requestAnimationFrame(() => editorEl?.focus());
  }
</script>

<svelte:window onpointerdown={onGlobalPointerDown} onkeydown={onUndoKeydown} />

<section class="viewer" class:private-document={privateDocument}>
  <div class="viewer-head">
    <div class="head-copy">
      <strong>{privateDocument ? transcript.title.replace(/^Documento sanitizado/, p("Documento sanitizado")) : transcript.title}</strong>
      <span>{privateDocument ? p("Documento privado · solo en memoria") : t.meta(formatDuration(transcript.duration), transcript.language, transcript.model)}</span>
    </div>
    <div class="head-actions">
      <button class="head-btn" onclick={resetToOriginal} disabled={!canReset} title={t.reset}>
        <svg viewBox="0 0 20 20" aria-hidden="true">
          <path d="M21 12a9 9 0 1 1-9-9 9.75 9.75 0 0 1 6.74 2.74L21 8"></path>
          <path d="M21 3v5h-5"></path>
        </svg>
        {t.reset}
      </button>
      <button class="head-btn" onclick={undo} disabled={!canUndo} title={`${t.undo} (Ctrl+Z)`}>
        <svg viewBox="0 0 20 20" aria-hidden="true">
          <path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8"></path>
          <path d="M3 3v5h5"></path>
        </svg>
        {t.undo}
      </button>
      <button class="head-btn" class:copied={copied} onclick={copyToClipboard}>{copied ? t.copied : t.copy}</button>
      <button class="head-btn save-btn" class:saved={savedCorrection} onclick={saveCorrection} disabled={savedCorrection}>
        <span class="save-label">
          <span class="save-label-measure" aria-hidden="true">{t.save}</span>
          <span class="save-label-measure" aria-hidden="true">{t.saved}</span>
          <span>{savedCorrection ? privateDocument ? p("Guardado") : t.saved : t.save}</span>
        </span>
      </button>
      <button class="head-btn primary" onclick={() => exportAsPDF()} disabled={exporting} title={privateDocument ? p("Exportar el documento actual a un PDF local") : t.exportPdf}>
        <svg viewBox="0 0 20 20" aria-hidden="true">
          <path d="M10 13V3"></path>
          <path d="M6 7l4-4 4 4"></path>
          <path d="M4 13v2a1 1 0 0 0 1 1h10a1 1 0 0 0 1-1v-2"></path>
        </svg>
        {exporting ? t.exportingPdf : privateDocument ? p("Exportar PDF") : t.exportPdf}
      </button>
    </div>
  </div>

  {#if statusMessage}
    <p class="status">{p(statusMessage)}</p>
  {/if}

  {#if hasSpeakers}
    <div class="speaker-panel">
      <span class="speaker-panel-label">{t.speakers}</span>
      <div class="speaker-fields">
        {#each speakerIds as speaker (speaker)}
          <label class="speaker-field">
            <span class="speaker-field-id">{defaultSpeakerLabel(speaker)}</span>
            <input
              type="text"
              value={speakerNames[String(speaker)] ?? ""}
              placeholder={t.speakerNamePlaceholder}
              onchange={(e) => updateSpeakerName(speaker, e.currentTarget.value)}
            />
          </label>
        {/each}
      </div>
    </div>
  {/if}

  <div class="editor-toolbar" aria-label={t.alignLabel}>
      <label class="tb-field" title={t.fontLabel}>
        <select
          class="tb-font"
          value={docFormat.font}
          onchange={(e) => {
            const v = (e.currentTarget as HTMLSelectElement).value;
            if (inEditorSelection()) applyFormat("fontName", v);
            else setDocFormat({ font: v });
          }}
          aria-label={t.fontLabel}
        >
          {#each FONT_OPTIONS as f}
            <option value={f.id}>{f.label}</option>
          {/each}
        </select>
      </label>
      <label class="tb-field" title={t.sizeLabel}>
        <select
          class="tb-size"
          value={docFormat.size}
          onchange={(e) => {
            const v = Number((e.currentTarget as HTMLSelectElement).value);
            if (inEditorSelection()) applyFormat("fontSize", sizeToLevel(v));
            else setDocFormat({ size: v });
          }}
          aria-label={t.sizeLabel}
        >
          {#each SIZE_OPTIONS as s}
            <option value={s}>{s}</option>
          {/each}
        </select>
      </label>
      <span class="tb-sep"></span>
      <div class="tb-styles" role="group" aria-label={t.boldLabel}>
        <button class="tb-btn" class:active={selBold} title={t.boldLabel} aria-label={t.boldLabel} onmousedown={(e) => e.preventDefault()} onclick={() => applyFormat("bold")}>
          <svg viewBox="0 0 20 20" aria-hidden="true"><path d="M6 4h5a3 3 0 0 1 0 6H6zM6 10h6a3 3 0 0 1 0 6H6z"></path></svg>
        </button>
        <button class="tb-btn" class:active={selItalic} title={t.italicLabel} aria-label={t.italicLabel} onmousedown={(e) => e.preventDefault()} onclick={() => applyFormat("italic")}>
          <svg viewBox="0 0 20 20" aria-hidden="true"><path d="M9 4l2 12M12.5 4H7M13 16h-5.5"></path></svg>
        </button>
        <button class="tb-btn" class:active={selUnderline} title={t.underlineLabel} aria-label={t.underlineLabel} onmousedown={(e) => e.preventDefault()} onclick={() => applyFormat("underline")}>
          <svg viewBox="0 0 20 20" aria-hidden="true"><path d="M6 4v6a4 4 0 0 0 8 0V4"></path><path d="M5 16.5h10"></path></svg>
        </button>
      </div>
      <div class="tb-color" role="group" aria-label={t.textColor}>
        <input
          type="color"
          class="tb-color-input"
          value={displayColor}
          onchange={(e) => applyFormat("foreColor", (e.currentTarget as HTMLInputElement).value)}
          aria-label={t.textColor}
          title={t.textColor}
        />
      </div>
      <span class="tb-sep"></span>
      <div class="tb-align" role="group" aria-label={t.alignLabel}>
        <button class="tb-btn" class:active={docFormat.align === "left"} title={t.alignLeft} aria-label={t.alignLeft} onclick={() => { if (inEditorSelection()) applyFormat("justifyLeft"); else setDocFormat({ align: "left" }); }}>
          <svg viewBox="0 0 20 20" aria-hidden="true"><path d="M3.5 5h13M3.5 10h8M3.5 15h13"></path></svg>
        </button>
        <button class="tb-btn" class:active={docFormat.align === "center"} title={t.alignCenter} aria-label={t.alignCenter} onclick={() => { if (inEditorSelection()) applyFormat("justifyCenter"); else setDocFormat({ align: "center" }); }}>
          <svg viewBox="0 0 20 20" aria-hidden="true"><path d="M3.5 5h13M6 10h8M3.5 15h13"></path></svg>
        </button>
        <button class="tb-btn" class:active={docFormat.align === "right"} title={t.alignRight} aria-label={t.alignRight} onclick={() => { if (inEditorSelection()) applyFormat("justifyRight"); else setDocFormat({ align: "right" }); }}>
          <svg viewBox="0 0 20 20" aria-hidden="true"><path d="M3.5 5h13M9 10h8M3.5 15h13"></path></svg>
        </button>
      </div>
    </div>

  <div class="ruler" bind:this={rulerRef} style={privateDocument ? privateRulerStyle : undefined}>
      <div class="ruler-ticks"></div>
      <div class="ruler-nums">
        {#each RULER_LABELS as n}
          <span class="ruler-num">{n}</span>
        {/each}
      </div>
      <div class="ruler-marker first" role="button" tabindex="0" style={`left: ${curPL + curTI}px`} onpointerdown={(e) => dragRuler("first", e)} onkeydown={(e) => { if (e.key === "ArrowLeft") nudge("first", -1); else if (e.key === "ArrowRight") nudge("first", 1); }} title={t.firstLineIndent}></div>
      <div class="ruler-marker left" role="button" tabindex="0" style={`left: ${curPL}px`} onpointerdown={(e) => dragRuler("left", e)} onkeydown={(e) => { if (e.key === "ArrowLeft") nudge("left", -1); else if (e.key === "ArrowRight") nudge("left", 1); }} title={t.leftIndent}></div>
      <div class="ruler-marker right" role="button" tabindex="0" style={`right: ${curPR}px`} onpointerdown={(e) => dragRuler("right", e)} onkeydown={(e) => { if (e.key === "ArrowLeft") nudge("right", -1); else if (e.key === "ArrowRight") nudge("right", 1); }} title={t.rightIndent}></div>
    </div>

  <div class="viewer-body">
    {#if privateDocument}<button class="return-comparison" onclick={onBackToComparison} title={p("Volver a pantalla dividida")} aria-label={p("Volver a pantalla dividida")}><svg viewBox="0 0 20 20" aria-hidden="true"><path d="M12.5 4.5 7 10l5.5 5.5M7 10h10" /></svg></button>{/if}
    <div
      class="corrected rich-editor"
      contenteditable="true"
      role="textbox"
      aria-multiline="true"
      spellcheck="false"
      tabindex="0"
      aria-label={t.corrected}
      style={editorStyle}
      bind:this={editorEl}
      oninput={syncFromEditor}
      onpaste={onPaste}
      onkeyup={onEditorInteract}
      onmouseup={onEditorInteract}
    ></div>
  </div>
</section>

<style>
  .viewer {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--surface);
    overflow: hidden;
    user-select: text;
    -webkit-user-select: text;
  }

  .viewer-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--line);
  }
  .head-copy { min-width: 0; display: flex; flex-direction: column; gap: 3px; }
  .head-copy strong { font-family: var(--font-heading); font-size: 17px; font-weight: 500; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .head-copy span { color: var(--muted); font-size: 13px; }
  .head-actions { display: flex; align-items: center; justify-content:flex-end; flex-wrap:wrap; gap: 7px; }

  /* Mini toolbar de formato del documento */
  .editor-toolbar {
    flex: none;
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--line);
    background: var(--surface);
  }
  .tb-field { display: inline-flex; }
  .tb-font,
  .tb-size {
    height: 30px;
    padding: 0 8px;
    border: 1px solid var(--line-dark);
    border-radius: 6px;
    color: var(--text);
    background: var(--surface);
    font-size: 13px;
    font-weight: 500;
    outline: none;
    cursor: pointer;
    white-space: nowrap;
  }
  .tb-font:hover,
  .tb-size:hover { border-color: var(--muted); }
  .tb-font:focus,
  .tb-size:focus { border-color: var(--accent); }
  .tb-sep { width: 1px; height: 20px; background: var(--line-dark); flex: none; }
  .tb-styles,
  .tb-align { display: inline-flex; align-items: center; gap: 3px; }
  .tb-btn {
    width: 30px;
    height: 30px;
    padding: 0;
    display: grid;
    place-items: center;
    border: 1px solid transparent;
    border-radius: 6px;
    background: transparent;
    color: var(--muted);
    cursor: pointer;
  }
  .tb-btn:hover { background: var(--hover); color: var(--text); }
  .tb-btn.active { color: var(--accent); border-color: var(--accent); background: var(--accent-soft); }
  .tb-btn svg { width: 16px; height: 16px; fill: none; stroke: currentColor; stroke-width: 1.6; stroke-linecap: round; stroke-linejoin: round; }

  .tb-color { display: inline-flex; align-items: center; gap: 3px; }
  .tb-color-input {
    width: 30px;
    height: 30px;
    padding: 2px;
    border: 1px solid var(--line-dark);
    border-radius: 6px;
    background: var(--surface);
    cursor: pointer;
  }
  .tb-color-input::-webkit-color-swatch-wrapper { padding: 2px; }
  .tb-color-input::-webkit-color-swatch { border: none; border-radius: 4px; }

  .head-btn {
    height: 36px;
    padding: 0 14px;
    border: 1px solid var(--line-dark);
    border-radius: 7px;
    color: var(--text);
    background: var(--surface);
    font-size: 13.5px;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    white-space: nowrap;
  }
  .head-btn:hover { background: var(--hover); }
  .head-btn:disabled { opacity: .6; cursor: default; }
  .head-btn.copied { color: var(--success); border-color: var(--success); }
  .head-btn.saved { color: var(--success); border-color: var(--success); opacity: 1; }
  .save-label { display: inline-grid; }
  .save-label > span { grid-area: 1 / 1; }
  .save-label-measure { visibility: hidden; }
  .head-btn.primary { border-color: var(--button); color: var(--button-text); background: var(--button); }
  .head-btn.primary:hover { border-color: var(--button-hover); background: var(--button-hover); }
  .head-btn.primary:disabled { opacity: .6; cursor: default; }
  .head-btn svg { width: 14px; height: 14px; fill: none; stroke: currentColor; stroke-width: 1.6; stroke-linecap: round; stroke-linejoin: round; }

  .status { padding: 6px 10px; color: var(--muted); font-size: 13px; user-select: text; }

  .speaker-panel {
    flex: none;
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
    padding: 8px 12px;
    border-bottom: 1px solid var(--line);
    background: var(--surface);
  }
  .speaker-panel-label { color: var(--muted); font-size: 13px; white-space: nowrap; }
  .speaker-fields { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
  .speaker-field { display: inline-flex; align-items: center; gap: 6px; }
  .speaker-field-id { color: var(--muted); font-size: 12.5px; white-space: nowrap; }
  .speaker-field input {
    width: 150px;
    height: 28px;
    padding: 0 8px;
    border: 1px solid var(--line-dark);
    border-radius: 6px;
    color: var(--text);
    background: var(--surface);
    font-size: 13px;
    outline: none;
  }
  .speaker-field input:hover { border-color: var(--muted); }
  .speaker-field input:focus { border-color: var(--accent); }


  .return-comparison { position:sticky; top:0; float:right; z-index:2; width:28px; height:28px; display:grid; place-items:center; padding:5px; margin-top:8px; margin-right:max(8px,calc((100% - 816px)/2 + 8px)); margin-bottom:-36px; border:1px solid var(--line); border-radius:4px; background:var(--surface); color:var(--muted); cursor:pointer; }
  .return-comparison:hover { color:var(--text); background:var(--hover); }
  .return-comparison svg { width:18px; height:18px; fill:none; stroke:currentColor; stroke-width:1.5; stroke-linecap:round; stroke-linejoin:round; }
  .viewer-body { flex: 1; min-height: 0; overflow-y: auto; padding: 10px 14px; user-select: text; }
  .viewer-body::-webkit-scrollbar { width: 8px; }
  .viewer-body::-webkit-scrollbar-thumb { background: var(--line-dark); border-radius: 4px; }

  .corrected {
    min-height: 220px;
    padding: 0;
    border: 0;
    outline: none;
    color: var(--text);
    background: transparent;
    caret-color: var(--accent);
    line-height: 1.65;
    word-break: break-word;
    user-select: text !important;
    -webkit-user-select: text !important;
    -webkit-user-modify: read-write;
    cursor: text;
  }
  .corrected:focus { outline: none; }
  .private-document .viewer-body { background:var(--panel); padding:24px; }
  .private-document .corrected { box-sizing:border-box; width:min(100%,816px); min-height:1056px; margin:0 auto; padding:72px 78px; background:var(--surface); box-shadow:0 3px 18px rgba(55,55,49,.1); }
  @media(max-width:760px) { .private-document .viewer-body { padding:10px; } .private-document .corrected { min-height:70vh; padding:28px 22px; } }

  /* Regla de márgenes/sangrías */
  .ruler {
    flex: none;
    position: relative;
    height: 30px;
    margin: 0 14px;
    border-bottom: 1px solid var(--line);
    user-select: none;
    cursor: default;
  }
  .ruler-ticks {
    position: absolute;
    inset: 0;
    background: repeating-linear-gradient(to right, var(--line-dark) 0 1px, transparent 1px 10px);
    opacity: .55;
  }
  .ruler-nums {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: flex-end;
    padding: 0 0 1px;
  }
  .ruler-num {
    flex: 1;
    text-align: center;
    color: var(--muted);
    font-size: 10px;
    font-variant-numeric: tabular-nums;
    line-height: 1;
  }
  .ruler-marker {
    position: absolute;
    top: 2px;
    width: 0;
    height: 24px;
    border-left: 3px solid currentColor;
    cursor: ew-resize;
    touch-action: none;
    z-index: 2;
  }
  .ruler-marker::before {
    content: "";
    position: absolute;
    top: 0;
    left: -7px;
    border-left: 5px solid transparent;
    border-right: 5px solid transparent;
    border-bottom: 6px solid currentColor;
  }
  .ruler-marker.first { color: #4da3ff; filter: drop-shadow(0 0 1px rgba(77, 163, 255, .45)); }
  .ruler-marker.left { color: var(--accent); }
  .ruler-marker.right { color: var(--muted); }
</style>
