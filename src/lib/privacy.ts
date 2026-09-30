export type EntityKind = "PERSON" | "COMPANY" | "ID" | "EMAIL" | "PHONE" | "ADDRESS" | "CASE" | "OTHER";
export interface PrivateEntity { kind: EntityKind; original: string; alias: string; source: string; }
export interface OcrSuggestion { original: string; replacement: string; }
export interface PrivacySession {
  id: string; fileName: string; original: string; entities: PrivateEntity[]; sanitized: string;
  warnings: string[]; advisories: string[]; analyzed: boolean; reviewed: boolean; edited: boolean; revision: number;
}
export interface OutboundPreview { schemaVersion: number; document: string; }
export const ENTITY_KINDS: { value: EntityKind; label: string }[] = [
  { value: "PERSON", label: "Persona" }, { value: "COMPANY", label: "Organización" },
  { value: "ID", label: "Identificación" }, { value: "EMAIL", label: "Email" },
  { value: "PHONE", label: "Teléfono" }, { value: "ADDRESS", label: "Dirección" },
  { value: "CASE", label: "Proceso" }, { value: "OTHER", label: "Otro identificador" },
];
