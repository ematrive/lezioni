/** 750 → "00:12:30" */
export function hmsOf(secs: number): string {
  const p = (n: number) => String(Math.floor(n)).padStart(2, "0");
  return `${p(secs / 3600)}:${p((secs / 60) % 60)}:${p(secs % 60)}`;
}
