import type { Transcript } from "../types";

export function escapeHtml(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}
export function textToHtml(s: string): string {
  return s
    .split("\n")
    .map((l) => `<div>${escapeHtml(l)}</div>`)
    .join("");
}
export function htmlToPlain(html: string): string {
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
export function defaultSpeakerLabel(speaker: number): string {
  return `Hablante ${speaker}`;
}

export function speakerLabel(speaker: number, names: Record<string, string>): string {
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

export function normalizeSpeakerText(
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
export function conversationHtml(html: string, names: Record<string, string>, hasSpeakers: boolean): string {
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

export function renameSpeakerHtml(
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
