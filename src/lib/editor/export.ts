/** A safe title and local export time keep successive PDFs distinguishable. */
export function pdfFileName(title: string, now = new Date()): string {
  const base = (title || "transcripcion").replace(/[\\/:*?"<>|]/g, "-");
  const pad = (value: number) => String(value).padStart(2, "0");
  const date = `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
  const time = `${pad(now.getHours())}-${pad(now.getMinutes())}-${pad(now.getSeconds())}`;
  return `${base}_${date}_${time}.pdf`;
}
