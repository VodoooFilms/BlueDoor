export type Align = "left" | "center" | "right";

/** Formato de documento (vista previa base + PDF): fuente, tamaño y alineación generales. */
export interface DocFormat {
  font: string;
  size: number;
  align: Align;
}

export const DEFAULT_FORMAT: DocFormat = {
  font: "system",
  size: 16,
  align: "left",
};

// Fuentes de sistema ofrecidas en el mini toolbar. `css` se usa en el editor
// (vista previa en vivo) y `pdf` indica la familia que carga el backend (Arial
// por defecto; Helvetica es .ttc y no la soporta genpdf).
export interface FontOption {
  id: string;
  label: string;
  css: string;
  pdf: string;
}

export const FONT_OPTIONS: FontOption[] = [
  { id: "system", label: "Sistema", css: `-apple-system, BlinkMacSystemFont, "Helvetica Neue", Arial, sans-serif`, pdf: "Arial" },
  { id: "helvetica", label: "Helvetica", css: `"Helvetica Neue", Helvetica, Arial, sans-serif`, pdf: "Arial" },
  { id: "times", label: "Times New Roman", css: `"Times New Roman", Georgia, serif`, pdf: "Times New Roman" },
  { id: "georgia", label: "Georgia", css: `Georgia, "Times New Roman", serif`, pdf: "Georgia" },
  { id: "courier", label: "Courier New", css: `"Courier New", monospace`, pdf: "Courier New" },
  { id: "verdana", label: "Verdana", css: `Verdana, Geneva, sans-serif`, pdf: "Verdana" },
];

export const SIZE_OPTIONS = [10, 11, 12, 13, 14, 15, 16, 17, 18, 20, 22, 24];

// Coincide con genpdf::Alignment (Left / Right / Center).
export const ALIGN_OPTIONS: { id: Align; label: string }[] = [
  { id: "left", label: "Izquierda" },
  { id: "center", label: "Centro" },
  { id: "right", label: "Derecha" },
];

export interface Transcript {
  id: string;
  title: string;
  text: string;
  segments: Segment[];
  createdAt: Date;
  duration: number;
  language: string;
  model: string;
  audioSource: "recording" | "file";
  fileName?: string;
  /** Texto editado y guardado; se conserva el nombre por compatibilidad. */
  corrected?: string;
  /** Contenido ricotexto del editor (HTML). El PDF usa `corrected` (plano). */
  correctedHtml?: string;
  /** Formato de documento aplicado al texto corregido. */
  format?: DocFormat;
  /** Nombres personalizados por identificador de hablante. */
  speakerNames?: Record<string, string>;
}

export interface Segment {
  start: number;
  end: number;
  text: string;
  speaker?: number;
}

export interface WhisperConfig {
  model: SttModel;
  language: string;
  threads: number;
}

type WhisperModel = "tiny" | "base" | "small" | "medium" | "large-v3-turbo" | "large-v3" | "large-v3-q5" | "latam-turbo-q5";

// Orden = calidad (mismo orden que WHISPER_MODELS en Rust).
const WHISPER_BASE: { id: WhisperModel; label: string; size: string }[] = [
  { id: "tiny", label: "Tiny", size: "78 MB" },
  { id: "base", label: "Base", size: "148 MB" },
  { id: "small", label: "Small", size: "488 MB" },
  { id: "medium", label: "Medium", size: "1.5 GB" },
  { id: "large-v3-turbo", label: "Large v3 Turbo", size: "1.62 GB" },
  { id: "large-v3", label: "Large v3", size: "3.10 GB" },
  { id: "large-v3-q5", label: "Large v3 Q5", size: "1.08 GB" },
  { id: "latam-turbo-q5", label: "LatAm Turbo Q5", size: "574 MB" },
];

export const DEFAULT_STT: SttModel = "parakeet-tdt-0.6b-v3";

// Opciones "recomendado o superior" (se omiten tiny/base en toda la UI).
const _whisperRecIdx = WHISPER_BASE.findIndex((o) => o.id === "small");
const WHISPER_CHOICES = WHISPER_BASE.slice(_whisperRecIdx);

type ParakeetModel = "parakeet-tdt-0.6b-v3";

// NVIDIA Parakeet, servido como GGUF y ejecutado por transcribe.cpp. Es mucho
// más rápido que Whisper y entrega timestamps por palabra, de modo que el texto
// sale re-seccionado por frase. Admite diarización con el motor Nemotron.
const PARAKEET_BASE: { id: ParakeetModel; label: string; size: string }[] = [
  { id: "parakeet-tdt-0.6b-v3", label: "Parakeet TDT v3", size: "740 MB" },
];

/** Cualquier modelo de transcripción, sea Whisper o Parakeet. */
export type SttModel = WhisperModel | ParakeetModel;

export interface SttOption {
  id: SttModel;
  label: string;
  size: string;
  family: "parakeet" | "whisper";
}

/** Catálogo que consume la UI, Parakeet primero por ser el más rápido. */
export const STT_OPTIONS: SttOption[] = [
  ...PARAKEET_BASE.map((o) => ({ ...o, family: "parakeet" as const })),
  ...WHISPER_CHOICES.map((o) => ({ ...o, family: "whisper" as const })),
];

/** Familia de un modelo por id; `null` si no está en el catálogo. */
export function sttFamily(id: string): "parakeet" | "whisper" | null {
  return STT_OPTIONS.find((o) => o.id === id)?.family ?? null;
}

export type RecordingState = "idle" | "recording" | "transcribing" | "processing";
