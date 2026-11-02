const UNITS = ["B", "KB", "MB", "GB", "TB"];

export function bytes(value: number, digits = 1): string {
  let n = Math.max(0, value);
  let unit = 0;
  while (n >= 1024 && unit < UNITS.length - 1) {
    n /= 1024;
    unit++;
  }
  return `${unit === 0 ? Math.round(n) : n.toFixed(digits)} ${UNITS[unit]}`;
}

export function speed(value: number): string {
  return `${bytes(value)}/s`;
}

export function duration(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${pad(h)}:${pad(m)}:${pad(s)}`;
}

export function date(unixSeconds: number): string {
  const d = new Date(unixSeconds * 1000);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

export function daysLeft(unixSeconds: number): number {
  return Math.ceil((unixSeconds * 1000 - Date.now()) / 86_400_000);
}

export function relative(unixSeconds: number): string {
  const diff = Math.floor(Date.now() / 1000 - unixSeconds);
  if (diff < 60) return "刚刚";
  if (diff < 3600) return `${Math.floor(diff / 60)} 分钟前`;
  if (diff < 86400) return `${Math.floor(diff / 3600)} 小时前`;
  return `${Math.floor(diff / 86400)} 天前`;
}

export type DelayTone = "good" | "ok" | "bad" | "none";

export function delayTone(delay: number | null | undefined): DelayTone {
  if (!delay) return "none";
  if (delay < 200) return "good";
  if (delay < 500) return "ok";
  return "bad";
}

/** Split a leading flag emoji off a node name ("🇭🇰 香港 01"). */
export function splitFlag(name: string): { flag: string | null; label: string } {
  const match = name.match(/^\s*((?:\p{Regional_Indicator}{2})|🏳️‍🌈|🌐)\s*/u);
  if (!match) return { flag: null, label: name };
  return { flag: match[1], label: name.slice(match[0].length) || name };
}
