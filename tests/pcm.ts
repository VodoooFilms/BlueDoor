import { PcmRecorder } from "../src/lib/pcm-recorder";
document.getElementById("record")!.onclick = async () => {
  const result = document.createElement("pre");
  result.id = "result";
  const source = new AudioContext({ sampleRate: 16000 });
  try {
    await source.resume();
    const oscillator = source.createOscillator();
    const destination = source.createMediaStreamDestination();
    oscillator.connect(destination);
    oscillator.start();
    const recorder = await PcmRecorder.create(destination.stream);
    // 1,5 s: una primera pasada en frío de WebKit renderiza audio más lento
    // que en tiempo real y 700 ms dejaban menos de las 4 000 muestras exigidas.
    await new Promise((resolve) => setTimeout(resolve, 1500));
    const blob = await recorder.stop();
    oscillator.stop();
    destination.stream.getTracks().forEach((t) => t.stop());
    const bytes = await blob.arrayBuffer();
    const wav = new DataView(bytes);
    const samples = new Int16Array(bytes.slice(44));
    if (wav.getUint32(24, true) !== 16000 || samples.length < 4000 || !samples.some((v) => v !== 0)) throw new Error("Incorrect WAV samples/rate");
    result.textContent = JSON.stringify({ ok: true, samples: samples.length, bytes: bytes.byteLength });
  } catch (e) { result.textContent = JSON.stringify({ ok: false, error: String(e) }); }
  finally { await source.close(); document.body.append(result); }
};
