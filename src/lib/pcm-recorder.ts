/** PCM capture through WKWebView's native Web Audio implementation. */
export function pcmWav(chunks: Int16Array[], sampleRate: number): Blob {
  const count = chunks.reduce((n, c) => n + c.length, 0);
  if (count === 0 || count > (0xffffffff - 36) / 2) throw new Error("No se pudo guardar el audio grabado.");
  const header = new ArrayBuffer(44);
  const view = new DataView(header);
  const tag = (offset: number, value: string) => [...value].forEach((c, i) => view.setUint8(offset + i, c.charCodeAt(0)));
  tag(0, "RIFF"); view.setUint32(4, 36 + count * 2, true);
  tag(8, "WAVE"); tag(12, "fmt "); view.setUint32(16, 16, true);
  view.setUint16(20, 1, true); view.setUint16(22, 1, true);
  view.setUint32(24, sampleRate, true); view.setUint32(28, sampleRate * 2, true);
  view.setUint16(32, 2, true); view.setUint16(34, 16, true);
  tag(36, "data"); view.setUint32(40, count * 2, true);
  // Apple Silicon is little endian, matching PCM's required byte order.
  return new Blob([header, ...chunks.map((c) => c.buffer as ArrayBuffer)], { type: "audio/wav" });
}

export class PcmRecorder {
  private chunks: Int16Array[] = [];
  private node: AudioWorkletNode;
  private source: MediaStreamAudioSourceNode;
  private finish: (() => void) | null = null;
  private failure: Error | null = null;

  private constructor(private context: AudioContext, stream: MediaStream) {
    this.node = new AudioWorkletNode(context, "local-legal-pcm");
    this.source = context.createMediaStreamSource(stream);
    this.node.port.onmessage = ({ data }) => {
      if (data.samples) this.chunks.push(data.samples);
      if (data.stopped) this.finish?.();
    };
    this.node.onprocessorerror = () => {
      this.failure = new Error("La grabación se interrumpió. Vuelve a intentarlo.");
      this.finish?.();
    };
    this.source.connect(this.node);
    // Worklet outputs silence; connecting keeps capture scheduled without feedback.
    this.node.connect(context.destination);
  }

  static async create(stream: MediaStream): Promise<PcmRecorder> {
    const context = new AudioContext({ sampleRate: 16000 });
    try {
      await context.audioWorklet.addModule("/pcm-worklet.js");
      const recorder = new PcmRecorder(context, stream);
      await context.resume();
      return recorder;
    } catch {
      await context.close();
      throw new Error("No se pudo iniciar la grabación. Si persiste, reinstala Local Legal.");
    }
  }

  async stop(): Promise<Blob> {
    try {
      if (this.failure) throw this.failure;
      await new Promise<void>((resolve, reject) => {
        const timeout = setTimeout(() => reject(new Error("La grabación no respondió. Vuelve a intentarlo.")), 5000);
        this.finish = () => { clearTimeout(timeout); resolve(); };
        this.node.port.postMessage("stop");
      });
      if (this.failure) throw this.failure;
      return pcmWav(this.chunks, this.context.sampleRate);
    } finally {
      this.dispose();
    }
  }

  dispose() {
    this.source.disconnect();
    this.node.disconnect();
    this.node.port.close();
    this.chunks = [];
    if (this.context.state !== "closed") void this.context.close();
  }
}
