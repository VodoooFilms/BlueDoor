export type Lang = "es" | "en";

export interface Copy {
  langLabel: string;
  switchLanguage: string;
  lightTheme: string;
  darkTheme: string;
  home: string;
  startupError: string;
  historyPreserved: string;
  retry: string;
  retrying: string;
  navigationBlocked: string;
  unsavedChangesTitle: string;
  unsavedChangesBody: string;
  discardChanges: string;
  saveAndLeave: string;

  // Models view
  modelsTitle: string;
  modelsSubtitle: string;
  setupWhisperTitle: string;
  setupWhisperLink: string;
  setupNemotronTitle: string;
  setupNemotronLink: string;
  downloaded: string;
  download: string;
  changeModel: string;
  continue: string;
  continueDisabledHint: string;
  continueReadyHint: string;
  delete: string;
  deleteConfirm: string;
  deleteTitle: string;
  deleteWhisperBody: string;
  deleted: string;
  back: string;
  cancel: string;

  // Recorder
  recordTitle: string;
  recordHint: string;
  importAudio: string;
  transcribing: string;
  transcribingHint: string;
  transcriptionTime: string;
  recordingLabel: string;
  model: string;
  history: string;
  noHistory: string;
  collapseHistory: string;
  expandHistory: string;
  usingModelsTitle: string;
  recordNav: string;
  editNav: string;
  localModels: string;
  speakerDetection: string;
  speakerDetectionHint: string;
  speakers: string;
  speakerNamePlaceholder: string;
  or: string;
  needsModel: string;
  micDenied: string;
  micUnavailable: string;
  deleteTranscript: string;

  // Transcript view
  newTranscript: string;
  undo: string;
  reset: string;
  copy: string;
  copied: string;
  corrected: string;
  save: string;
  saved: string;
  exportPdf: string;
  exportingPdf: string;
  pdfSaved: (path: string) => string;
  meta: (dur: string, lang: string, model: string) => string;
  fontLabel: string;
  sizeLabel: string;
  alignLabel: string;
  alignLeft: string;
  alignCenter: string;
  alignRight: string;
  boldLabel: string;
  italicLabel: string;
  underlineLabel: string;
  textColor: string;
  colorAuto: string;
  leftIndent: string;
  firstLineIndent: string;
  rightIndent: string;
}

export const COPY: Record<Lang, Copy> = {
  es: {
    langLabel: "ES",
    switchLanguage: "Cambiar a inglés",
    lightTheme: "Cambiar a modo claro",
    darkTheme: "Cambiar a modo oscuro",
    home: "Inicio (micrófono)",
    startupError: "No se pudo iniciar BlueDoor.",
    historyPreserved: "El historial no se ha modificado.",
    retry: "Reintentar",
    retrying: "Reintentando…",
    navigationBlocked: "Termina la grabación o la transcripción antes de cambiar de pantalla.",
    unsavedChangesTitle: "Cambios sin guardar",
    unsavedChangesBody: "Guarda los cambios de texto y nombres antes de salir de esta transcripción.",
    discardChanges: "Descartar",
    saveAndLeave: "Guardar y salir",

    modelsTitle: "Modelos",
    modelsSubtitle:
      "Estos modelos se almacenan localmente en tu dispositivo. Puedes elegir, descargar y eliminar cada uno cuando quieras.",
    setupWhisperTitle: "Transcripción de voz",
    setupWhisperLink:
      "Convierte tu voz en texto en el idioma que hables. Parakeet es el más rápido; Whisper es más preciso.",
    setupNemotronTitle: "NVIDIA Nemotron 3",
    setupNemotronLink:
      "Identifica varias voces y separa la transcripción por hablante.",
    downloaded: "✓ Descargado",
    download: "Descargar",
    changeModel: "Cambiar",
    continue: "Continuar",
    continueDisabledHint: "Descarga Whisper o Parakeet para transcribir.",
    continueReadyHint: "Modelo de voz listo para usar.",
    delete: "Eliminar",
    deleteConfirm: "Eliminar",
    deleteTitle: "Eliminar modelo",
    deleteWhisperBody: "Borrarás el modelo Whisper de tu dispositivo. El audio grabado no se ve afectado.",
    deleted: "Modelo eliminado",
    back: "Volver",
    cancel: "Cancelar",

    recordTitle: "Grabar tu voz",
    recordHint: "haz clic o pulsa Espacio para grabar",
    importAudio: "Importar audio",
    transcribing: "Transcribiendo…",
    transcribingHint: "Esto puede tardar según la duración del audio",
    transcriptionTime: "Tiempo de transcripción",
    recordingLabel: "Grabando… haz clic, Espacio o Cmd+Enter para detener",
    model: "Modelo",
    history: "Historial",
    noHistory: "Aún no hay transcripciones",
    collapseHistory: "Ocultar historial",
    expandHistory: "Mostrar historial",
    usingModelsTitle: "Modelos activos para esta sesión (se cambian en la pestaña Modelos)",
    recordNav: "Grabar",
    editNav: "Editar",
    localModels: "Modelos locales:",
    speakerDetection: "Identificar voces",
    speakerDetectionHint: "Opcional y más lento; desactivado para máxima velocidad",
    speakers: "Hablantes:",
    speakerNamePlaceholder: "Escribe un nombre",
    or: "o",
    needsModel: "Primero descarga un modelo de voz (Whisper o Parakeet) en Ajustes → Modelos para poder transcribir.",
    micDenied:
      "Permiso de micrófono denegado. Concede acceso en los ajustes de privacidad del micrófono del sistema.",
    micUnavailable: "La grabación no está disponible en este dispositivo.",
    deleteTranscript: "Eliminar",

    newTranscript: "Nuevo",
    copy: "Copiar",
    undo: "Deshacer",
    reset: "Restablecer",
    copied: "Copiado",
    corrected: "Texto editable",
    save: "Grabar",
    saved: "Grabado",
    exportPdf: "Exportar PDF",
    exportingPdf: "Generando…",
    pdfSaved: (path) => `PDF guardado en ${path}`,
    meta: (dur, lang, model) => `${dur} · ${lang} · ${model}`,
    fontLabel: "Fuente",
    sizeLabel: "Tamaño",
    alignLabel: "Alineación",
    alignLeft: "Alinear a la izquierda",
    alignCenter: "Centrar",
    alignRight: "Alinear a la derecha",
    boldLabel: "Negrita",
    italicLabel: "Cursiva",
    underlineLabel: "Subrayado",
    textColor: "Color del texto",
    colorAuto: "Color automático",
    leftIndent: "Sangría izquierda",
    firstLineIndent: "Sangría de primera línea",
    rightIndent: "Margen derecho",
  },
  en: {
    langLabel: "EN",
    switchLanguage: "Switch to Spanish",
    lightTheme: "Switch to light mode",
    darkTheme: "Switch to dark mode",
    home: "Home (microphone)",
    startupError: "BlueDoor could not start.",
    historyPreserved: "Your history has not been modified.",
    retry: "Retry",
    retrying: "Retrying…",
    navigationBlocked: "Finish recording or transcription before changing screens.",
    unsavedChangesTitle: "Unsaved changes",
    unsavedChangesBody: "Save text and speaker name changes before leaving this transcript.",
    discardChanges: "Discard",
    saveAndLeave: "Save and leave",

    modelsTitle: "Models",
    modelsSubtitle:
      "These models are stored locally on your device. You can choose, download and delete each one anytime.",
    setupWhisperTitle: "Speech to text",
    setupWhisperLink: "Turns your voice into text in the language you speak. Parakeet is the fastest; Whisper is more accurate.",
    setupNemotronTitle: "NVIDIA Nemotron 3",
    setupNemotronLink: "Identifies multiple voices and splits the transcript by speaker.",
    downloaded: "✓ Downloaded",
    download: "Download",
    changeModel: "Change",
    continue: "Continue",
    continueDisabledHint: "Download Whisper or Parakeet to transcribe.",
    continueReadyHint: "Voice model ready to use.",
    delete: "Delete",
    deleteConfirm: "Delete",
    deleteTitle: "Delete model",
    deleteWhisperBody: "This removes the Whisper model from your device. Recorded audio is not affected.",
    deleted: "Model deleted",
    back: "Back",
    cancel: "Cancel",

    recordTitle: "Record your voice",
    recordHint: "click or press Space to record",
    importAudio: "Import audio",
    transcribing: "Transcribing…",
    transcribingHint: "This may take a while depending on the audio length",
    transcriptionTime: "Transcription time",
    recordingLabel: "Recording… click, Space or Cmd+Enter to stop",
    model: "Model",
    history: "History",
    noHistory: "No transcriptions yet",
    collapseHistory: "Hide history",
    expandHistory: "Show history",
    usingModelsTitle: "Active models for this session (change them in the Models tab)",
    recordNav: "Record",
    editNav: "Edit",
    localModels: "Local Models:",
    speakerDetection: "Identify speakers",
    speakerDetectionHint: "Optional and slower; off for maximum speed",
    speakers: "Speakers:",
    speakerNamePlaceholder: "Enter a name",
    or: "or",
    needsModel: "Download a speech model (Whisper or Parakeet) in Settings → Models before transcribing.",
    micDenied:
      "Microphone permission denied. Grant access in your system microphone privacy settings.",
    micUnavailable: "Recording is not available on this device.",
    deleteTranscript: "Delete",

    newTranscript: "New",
    copy: "Copy",
    undo: "Undo",
    reset: "Reset",
    copied: "Copied",
    corrected: "Editable text",
    save: "Save",
    saved: "Saved",
    exportPdf: "Export PDF",
    exportingPdf: "Generating…",
    pdfSaved: (path) => `PDF saved at ${path}`,
    meta: (dur, lang, model) => `${dur} · ${lang} · ${model}`,
    fontLabel: "Font",
    sizeLabel: "Size",
    alignLabel: "Alignment",
    alignLeft: "Align left",
    alignCenter: "Center",
    alignRight: "Align right",
    boldLabel: "Bold",
    italicLabel: "Italic",
    underlineLabel: "Underline",
    textColor: "Text color",
    colorAuto: "Automatic color",
    leftIndent: "Left indent",
    firstLineIndent: "First-line indent",
    rightIndent: "Right margin",
  },
};

const STORAGE_KEY = "bluedoor-language";

export function getInitialLang(): Lang {
  const saved = localStorage.getItem(STORAGE_KEY);
  return saved === "en" ? "en" : "es";
}

export function saveLang(lang: Lang) {
  localStorage.setItem(STORAGE_KEY, lang);
}
