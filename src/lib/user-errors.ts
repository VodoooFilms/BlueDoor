import type { Lang } from "./i18n";
const text = (error: unknown) => error instanceof Error ? error.message : String(error);
export function microphoneError(error: unknown, lang: Lang): string {
  const name = (error as { name?: string } | null)?.name;
  const es = lang === "es";
  if (name === "NotAllowedError" || name === "SecurityError") return es
    ? "Permiso de micrófono denegado. Activa BlueDoor en Ajustes del Sistema → Privacidad y seguridad → Micrófono y vuelve a intentarlo."
    : "Microphone permission denied. Enable BlueDoor in System Settings → Privacy & Security → Microphone, then try again.";
  if (name === "NotFoundError" || name === "DevicesNotFoundError" || name === "OverconstrainedError") return es
    ? "No se encontró ningún micrófono. Conecta uno o selecciona una entrada de audio en Ajustes del Sistema → Sonido y vuelve a intentarlo."
    : "No microphone found. Connect one or select an audio input in System Settings → Sound, then try again.";
  if (name === "NotReadableError" || name === "TrackStartError") return es
    ? "No se pudo iniciar el micrófono. Puede estar ocupado por otra app o desconectado. Cierra las apps que lo usan, comprueba la conexión y vuelve a intentarlo."
    : "Could not start the microphone. It may be in use by another app or disconnected. Close apps using it, check its connection, then try again.";
  return es ? "No se pudo iniciar la grabación. Comprueba el micrófono y su permiso en Ajustes del Sistema y vuelve a intentarlo."
    : "Could not start recording. Check the microphone and its permission in System Settings, then try again.";
}
export function importErrorMessage(error: unknown, kind: "audio" | "document", lang: Lang): string {
  const message = text(error).replace(/^Error:\s*/, "");
  const es = lang === "es";
  if (kind === "document") {
    if (/Formatos admitidos|unsupported.*(document|format)/i.test(message)) return es
      ? "Este formato de documento no es compatible. Selecciona un PDF, DOCX, TXT, MD o una imagen PNG, JPG, TIFF o HEIC."
      : "This document format is not supported. Select a PDF, DOCX, TXT, MD or a PNG, JPG, TIFF or HEIC image.";
    if (/No se pudo abrir el documento/i.test(message)) return es
      ? "No se pudo abrir el documento. Comprueba que sigue disponible y tienes permiso para leerlo; después selecciónalo de nuevo."
      : "Could not open the document. Check that it is still available and you have permission to read it, then select it again.";
    if (/No se pudo (extraer|leer|procesar).*(PDF|documento|DOCX)|invalid.*pdf/i.test(message)) return es
      ? "No se pudo leer el documento. Comprueba que puedes abrirlo y que no esté dañado ni protegido con contraseña; después vuelve a importarlo."
      : "Could not read the document. Check that it opens and is not damaged or password protected, then import it again.";
    return message;
  }
  if (/Selecciona un archivo de audio o vídeo compatible|extensión válida|formato.*no.*válido|unsupported.*format/i.test(message)) return es
    ? "Este archivo no tiene un formato de audio o vídeo compatible. Prueba con WAV, MP3, M4A, FLAC, OGG, MP4 o MOV."
    : "This file is not a supported audio or video format. Try WAV, MP3, M4A, FLAC, OGG, MP4 or MOV.";
  if (/No se pudo leer|dañado|no contener audio|invalid.*(data|audio)|decode.*fail/i.test(message)) return es
    ? "No se pudo leer el audio. El archivo puede estar dañado o no contener una pista de audio. Comprueba que se reproduce o expórtalo como WAV o MP3 y vuelve a intentarlo."
    : "Could not read the audio. The file may be damaged or have no audio track. Check that it plays, or export it as WAV or MP3 and try again.";
  if (/No se pudo abrir|not found|no such file/i.test(message)) return es
    ? "No se pudo abrir el archivo. Comprueba que sigue disponible y tienes permiso para leerlo; después selecciónalo de nuevo."
    : "Could not open the file. Check that it is still available and you have permission to read it, then select it again.";
  return es ? "No se pudo procesar el audio. Comprueba el archivo y que el modelo esté descargado, y vuelve a intentarlo."
    : "Could not process the audio. Check the file and that the model is downloaded, then try again.";
}
