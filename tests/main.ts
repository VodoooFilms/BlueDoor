import { mount } from "svelte";
import TranscriptView from "../src/components/TranscriptView.svelte";
import type { Transcript } from "../src/lib/types";

const transcript: Transcript = (window as any).__transcriptFixture ?? {
  id: "harness-1",
  title: "Prueba de edicion",
  text: "Hola, esta es una prueba de transcripcion.",
  segments: [{ start: 0, end: 12, text: "Hola, esta es una prueba de transcripcion." }],
  createdAt: new Date(),
  duration: 12,
  language: "es",
  model: "ggml-large-v3-turbo.bin",
  audioSource: "recording",
};

const app = document.getElementById("app")!;
mount(TranscriptView, {
  target: app,
  props: {
    transcript,
    onDirtyChange: () => {},
    onSave: (id, text, format, html, speakerNames) => {
      (window as any).__lastSave = { id, text, format, html, speakerNames };
      console.log("onSave", JSON.stringify({ id, text, format, html }));
    },
  },
});