import { useEffect, useState } from "react";
import { fmtDate, share, toAppError, type Hit } from "../../lib/api";
import { hmsOf } from "../../lib/time";

const AREA: Record<Hit["area"], string> = { title: "Titolo", notes: "Note", summary: "Sintesi", study: "Studio", extract: "" };

/** Hits with the query words highlighted. */
function Snippet({ text, query }: { text: string; query: string }) {
  const words = query
    .split(/\s+/)
    .filter((w) => w.length >= 2)
    .map((w) => w.normalize("NFD").replace(/[̀-ͯ]/g, "").replace(/[.*+?^${}()|[\]\\]/g, "\\$&"));
  if (!words.length) return <>{text}</>;
  // Accent-insensitive: match on the folded text, cut the original at the same positions.
  const folded = text.normalize("NFD").replace(/[̀-ͯ]/g, "");
  if (folded.length !== text.length) return <>{text}</>;
  const re = new RegExp(words.join("|"), "gi");
  const out: React.ReactNode[] = [];
  let last = 0;
  for (const m of folded.matchAll(re)) {
    const at = m.index ?? 0;
    out.push(text.slice(last, at));
    out.push(
      <mark key={at} className="rounded bg-amber-200 px-0.5 text-inherit dark:bg-amber-700/60">
        {text.slice(at, at + m[0].length)}
      </mark>,
    );
    last = at + m[0].length;
  }
  out.push(text.slice(last));
  return <>{out}</>;
}

export function SearchResults({ query, onOpen }: { query: string; onOpen: (h: Hit) => void }) {
  const [hits, setHits] = useState<Hit[] | null>(null);
  useEffect(() => {
    let live = true;
    const t = setTimeout(() => {
      share
        .search(query)
        .then((h) => live && setHits(h))
        .catch((e) => {
          console.error(toAppError(e).message);
          if (live) setHits([]);
        });
    }, 200);
    return () => {
      live = false;
      clearTimeout(t);
    };
  }, [query]);
  return (
    <div className="flex h-full flex-col">
      <div className="border-b border-zinc-200 px-3 py-3 dark:border-zinc-800">
        <h2 className="text-base font-semibold">Risultati</h2>
        <p className="text-xs text-zinc-500">{hits === null ? "Ricerca…" : hits.length === 0 ? "Nessun risultato." : `${hits.length >= 200 ? "Primi 200" : hits.length} risultati in tutto l'archivio`}</p>
      </div>
      <ul className="min-h-0 flex-1 overflow-auto p-2">
        {hits?.map((h, i) => (
          <li key={i}>
            <button className="w-full rounded-md px-3 py-2 text-left hover:bg-zinc-100 dark:hover:bg-zinc-900" onClick={() => onOpen(h)}>
              <div className="flex items-baseline justify-between gap-2">
                <span className="truncate font-medium">{h.lesson}</span>
                <span className="shrink-0 text-xs text-zinc-400">{fmtDate(h.date)}</span>
              </div>
              <div className="truncate text-xs text-sky-700 dark:text-sky-400">
                {h.course} · {AREA[h.area] || h.label}
                {h.time !== null && ` · ${hmsOf(h.time)}`}
              </div>
              {h.area !== "title" && (
                <p className="mt-1 line-clamp-3 text-xs text-zinc-600 dark:text-zinc-400">
                  <Snippet text={h.snippet} query={query} />
                </p>
              )}
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
