const KB = 1024;

export function formatBytes(bytes: number | null, opts: { approximate?: boolean } = {}): string {
  if (bytes === null || bytes === undefined) return "unknown";
  const prefix = opts.approximate ? "≥ " : "";
  if (bytes < KB) return `${prefix}${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / KB;
  let unit = 0;
  while (value >= KB && unit < units.length - 1) {
    value /= KB;
    unit += 1;
  }
  const digits = value >= 100 ? 0 : value >= 10 ? 1 : 2;
  return `${prefix}${value.toFixed(digits)} ${units[unit]}`;
}

export function formatCount(n: number): string {
  return n.toLocaleString("en-US");
}

export function formatRelTime(iso: string | null): string {
  if (!iso) return "unknown";
  const diff = Date.now() - new Date(iso).getTime();
  const days = Math.floor(diff / 86400_000);
  if (days <= 0) {
    const hours = Math.floor(diff / 3600_000);
    return hours <= 0 ? "just now" : `${hours} hours ago`;
  }
  if (days === 1) return "1 day ago";
  if (days < 30) return `${days} days ago`;
  const months = Math.floor(days / 30);
  if (months < 12) return `${months} months ago`;
  return `${Math.floor(months / 12)} years ago`;
}

export function formatClock(seconds: number): string {
  return `${Math.floor(seconds)}s`;
}

export function formatTimestamp(iso: string | null): string {
  if (!iso) return "—";
  const d = new Date(iso);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

export function shortenPath(path: string, root: string): string {
  if (path === root) return ".";
  if (path.startsWith(`${root}/`)) return path.slice(root.length + 1);
  return path;
}

export function displayPath(path: string, root: string): string {
  if (path === root) return `${basename(root)} (main worktree)`;
  return shortenPath(path, root);
}

export function homePath(path: string): string {
  return path.replace(/^\/Users\/[^/]+/, "~");
}

export function basename(path: string): string {
  const parts = path.split("/");
  return parts[parts.length - 1] || path;
}

export function dirname(path: string): string {
  const idx = path.lastIndexOf("/");
  return idx <= 0 ? "/" : path.slice(0, idx);
}
