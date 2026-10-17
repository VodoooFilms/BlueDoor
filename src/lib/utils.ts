export function formatDuration(seconds: number): string {
  const mins = Math.floor(seconds / 60);
  const secs = Math.floor(seconds % 60);
  return `${mins}:${secs.toString().padStart(2, "0")}`;
}

export function generateId(): string {
  return Date.now().toString(36) + Math.random().toString(36).slice(2, 8);
}

const allowedRichTags = new Set(["b", "strong", "i", "em", "u", "ins", "span", "div", "br", "p", "font"]);
const droppedRichTags = new Set([
  "script",
  "style",
  "iframe",
  "object",
  "embed",
  "link",
  "meta",
  "base",
  "form",
  "input",
  "textarea",
  "button",
  "svg",
  "math",
  "template",
  "noscript",
]);
const allowedStyleProperties = [
  "color",
  "font-family",
  "font-size",
  "font-weight",
  "font-style",
  "text-decoration",
  "text-align",
  "padding-left",
  "text-indent",
  "padding-right",
];

function sanitizeStyle(value: string): string {
  const probe = document.createElement("span");
  probe.style.cssText = value;
  const declarations: string[] = [];
  for (const property of allowedStyleProperties) {
    const parsed = probe.style.getPropertyValue(property).trim();
    if (!parsed || /url\s*\(|expression\s*\(|javascript:/i.test(parsed)) continue;
    if (property === "text-align" && !/^(left|right|center|justify)$/i.test(parsed)) continue;
    if (property === "font-weight" && !/^(?:normal|bold|bolder|lighter|[1-9]00)$/i.test(parsed)) continue;
    if (property === "font-style" && !/^(?:normal|italic|oblique)$/i.test(parsed)) continue;
    if (property === "text-decoration" && !/^(?:none|underline)$/i.test(parsed)) continue;
    if (property === "font-size" && !/^(?:[1-7]|xx-small|x-small|small|medium|large|x-large|xx-large|xxx-large|smaller|larger|[-+]?\d+(?:\.\d+)?(?:px|pt|em|rem|%)?)$/i.test(parsed)) continue;
    if (property === "padding-left" || property === "text-indent" || property === "padding-right") {
      if (!/^[-+]?\d+(?:\.\d+)?(?:px|pt|em|rem|%)?$/.test(parsed)) continue;
    }
    declarations.push(`${property}: ${parsed}`);
  }
  return declarations.join("; ");
}

function sanitizeRichAttribute(name: string, value: string): string | null {
  const lower = name.toLowerCase();
  if (lower === "style") return sanitizeStyle(value);
  if (lower === "face" && /^[a-z0-9 _-]{1,80}$/i.test(value)) return value;
  if (lower === "size" && /^[1-7]$/.test(value)) return value;
  if (lower === "color" && /^(?:#[0-9a-f]{3,8}|(?:rgb|hsl)a?\([^)]{1,80}\)|[a-z]{1,30})$/i.test(value)) return value;
  return null;
}

// Fast path: the editor only marks up with canonical labels (div/br/b/span…)
// and its own attributes (style/face/size/color). Verifying with string
// expressions costs linear scans without touching the DOM; if anything is
// outside the canon, take the full path (parse + rebuild), which is the
// current behavior and remains the security gate.
function scanVerbatimTag(match: RegExpExecArray): string | null {
  const tag = match[1].toLowerCase();
  // Unknown tags receive child nodes via slow path treatment: refuse fast path.
  if (!allowedRichTags.has(tag) || droppedRichTags.has(tag)) return null;
  const markup = match[2];
  const attrPattern = / ([a-z-]+)="([^"]*)"/g;
  let checked = 0;
  let attr: RegExpExecArray | null;
  while ((attr = attrPattern.exec(markup)) !== null) {
    // Non-attribute leftovers (spaces other than the separator, users with unquoted
    // values) fall through to the slow path.
    if (attr.index !== checked || markup.charCodeAt(checked) !== 32) return null;
    checked = attr.index + attr[0].length;
    // Only canonical attributes and values that the sanitizer must not change:
    // any difference invokes the full path.
    if (sanitizeRichAttribute(attr[1], attr[2]) !== attr[2]) return null;
  }
  return checked === markup.length ? tag : null;
}

// `indexOf` end pointer: the previous end byte is exclusive, so the text
// between tags never contains a stray `<`.
function isVerbatimRichHtml(html: string): boolean {
  // Canonical tags: lowercase, quoted attributes, no space before `>`, no
  // trailing `<` outside the tags (real text with `<` falls to the slow path).
  const tagPattern = /<[/]?([a-z][a-z0-9]*)([^<>]*)>/g;
  let tail = 0;
  let tag: RegExpExecArray | null;
  while ((tag = tagPattern.exec(html)) !== null) {
    if (html.slice(tail, tag.index).indexOf("<") !== -1) return false;
    if (scanVerbatimTag(tag) === null) return false;
    tail = tagPattern.lastIndex;
  }
  return html.indexOf("<", tail) === -1;
}

function appendSanitizedNode(source: Node, target: Node): void {
  if (source.nodeType === Node.TEXT_NODE) {
    target.appendChild(document.createTextNode(source.nodeValue ?? ""));
    return;
  }
  if (!(source instanceof Element)) return;
  const tag = source.tagName.toLowerCase();
  if (droppedRichTags.has(tag)) return;
  if (!allowedRichTags.has(tag)) {
    for (const child of Array.from(source.childNodes)) appendSanitizedNode(child, target);
    return;
  }
  const clean = document.createElement(tag);
  for (const attribute of Array.from(source.attributes)) {
    const value = sanitizeRichAttribute(attribute.name, attribute.value);
    if (value !== null) clean.setAttribute(attribute.name, value);
  }
  for (const child of Array.from(source.childNodes)) appendSanitizedNode(child, clean);
  target.appendChild(clean);
}

export function sanitizeRichHtml(html: string): string {
  // Editor markup canonically is identical before and after sanitizing; the
  // rebuild is only necessary when a non-editor node (drop, paste, external
  // HTML) sneaks in. Reading is a linear scan that does not mutate the DOM.
  if (isVerbatimRichHtml(html)) return html;
  const template = document.createElement("template");
  template.innerHTML = html;
  const output = document.createDocumentFragment();
  for (const child of Array.from(template.content.childNodes)) appendSanitizedNode(child, output);
  const holder = document.createElement("div");
  holder.append(output);
  return holder.innerHTML;
}
