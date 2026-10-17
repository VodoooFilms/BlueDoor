// App-owned PCM capture: no codec, network, dependency or executable required.
class PcmCapture extends AudioWorkletProcessor {
  constructor() {
    super();
    this.buffer = new Int16Array(4096);
    this.used = 0;
    this.recording = true;
    this.port.onmessage = ({ data }) => {
      if (data === "stop") {
        this.recording = false;
        this.flush();
        this.port.postMessage({ stopped: true });
      }
    };
  }
  flush() {
    if (!this.used) return;
    const samples = this.buffer.slice(0, this.used);
    this.port.postMessage({ samples }, [samples.buffer]);
    this.used = 0;
  }
  process(inputs) {
    if (!this.recording) return false;
    const channels = inputs[0];
    if (!channels?.length) return true;
    for (let i = 0; i < channels[0].length; i++) {
      let sample = 0;
      for (const channel of channels) sample += channel[i];
      sample = Math.max(-1, Math.min(1, sample / channels.length));
      this.buffer[this.used++] = Math.round(sample * (sample < 0 ? 32768 : 32767));
      if (this.used === this.buffer.length) this.flush();
    }
    return true;
  }
}
registerProcessor("bluedoor-pcm", PcmCapture);
