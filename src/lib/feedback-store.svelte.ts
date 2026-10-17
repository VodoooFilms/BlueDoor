export type Notice = { id: symbol; message: string; kind: "error" | "status"; className: string; onDismiss?: () => void };
let notices = $state<Notice[]>([]);
export function getNotices() { return notices; }
export function putNotice(notice: Notice) { notices = [...notices.filter(item => item.id !== notice.id), notice]; }
export function removeNotice(id: symbol) { notices = notices.filter(item => item.id !== id); }
