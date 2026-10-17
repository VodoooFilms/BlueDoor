// Estado compartido y persistente del modelo de transcripción activo.
// - Lo usa Recorder (transcripción) y ModelsView.
// - Cubre las dos familias del mismo motor: Whisper y Parakeet.
import { DEFAULT_STT, STT_OPTIONS } from "./types";
import type { SttModel } from "./types";

const KEY = "bluedoor-model-stt";
// Clave de la era en la que solo había Whisper. Se lee una vez y se migra.
const LEGACY_KEY = "bluedoor-model-whisper";

function read(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function write(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    /* sin localStorage: no persistir */
  }
}

function normalize(v: string | null): SttModel {
  const found = STT_OPTIONS.find((o) => o.id === v);
  return found ? found.id : DEFAULT_STT;
}

function initial(): SttModel {
  const current = read(KEY);
  if (current) return normalize(current);
  const legacy = read(LEGACY_KEY);
  if (legacy) {
    const migrated = normalize(legacy);
    write(KEY, migrated);
    return migrated;
  }
  return DEFAULT_STT;
}

let stt: SttModel = $state(initial());

export function getSttModel(): SttModel {
  return stt;
}

export function setSttModel(m: SttModel) {
  stt = m;
  write(KEY, m);
}
