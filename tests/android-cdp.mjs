/** Debug WebView bridge for synthetic native Android QA; requires adb forwarding. */
export async function connectAndroid(port = 9223) {
  let tabs;
  for (let attempt = 0; attempt < 10; attempt++) {
    try { tabs = await fetch(`http://127.0.0.1:${port}/json/list`).then(r => r.json()); if (tabs.some(t => t.url === "http://tauri.localhost/" || t.url === "https://tauri.localhost/")) break; } catch (error) { if (attempt === 9) throw error; }
    await new Promise(resolve => setTimeout(resolve, 500));
  }
  const tab = tabs.find(t => t.url === 'http://tauri.localhost/' || t.url === 'https://tauri.localhost/');
  if (!tab) throw new Error('No BlueDoor debug WebView found');
  const socket = new WebSocket(tab.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => { socket.addEventListener('open', resolve, { once: true }); socket.addEventListener('error', reject, { once: true }); });
  let sequence = 0;
  const pending = new Map();
  socket.addEventListener('close', () => { for (const task of pending.values()) task.reject(new Error('Android WebView disconnected')); pending.clear(); });
  socket.addEventListener('message', e => { const message = JSON.parse(e.data); const task = pending.get(message.id); if (task) { pending.delete(message.id); message.error ? task.reject(new Error(JSON.stringify(message.error))) : task.resolve(message.result); } });
  async function evaluate(expression) {
    const id = ++sequence;
    const result = await new Promise((resolve, reject) => { pending.set(id, { resolve, reject }); socket.send(JSON.stringify({ id, method: 'Runtime.evaluate', params: { expression, returnByValue: true, awaitPromise: true } })); });
    if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description || result.exceptionDetails.exception?.value || result.exceptionDetails.text);
    return result.result?.value;
  }
  return { evaluate, invoke: (command, args = {}) => evaluate(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(command)},${JSON.stringify(args)})`), close: () => socket.close() };
}
