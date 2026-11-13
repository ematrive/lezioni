import { ChevronDown, ChevronUp, Search, X } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { t } from "../i18n";

/** Lower case without accents, one character per character so that offsets stay valid. */
function foldKeep(s: string): string {
  // Fold each character on its own so the folded text is as long as the original.
  let out = "";
  for (const ch of s) {
    const f = ch.toLowerCase().normalize("NFD").replace(/\p{M}/gu, "");
    out += f.length === ch.length ? f : ch.toLowerCase();
  }
  return out;
}

type HighlightRegistry = { set: (k: string, h: unknown) => void; delete: (k: string) => void };
const registry = (): HighlightRegistry | null => (typeof CSS !== "undefined" && "highlights" in CSS ? (CSS as unknown as { highlights: HighlightRegistry }).highlights : null);
const HighlightCtor = (globalThis as unknown as { Highlight?: new (...r: Range[]) => unknown }).Highlight;

/** The occurrences of `query` in the text of `root` (case and accents ignored). */
function findRanges(root: HTMLElement, query: string): Range[] {
  const q = foldKeep(query.trim());
  if (q.length < 2) return [];
  const out: Range[] = [];
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
    acceptNode: (n) => {
      const el = n.parentElement;
      // Not the formulas' hidden MathML, not the find bar itself, not form fields.
      if (!el || el.closest(".katex-mathml, [data-find-bar], textarea, input, script, style")) return NodeFilter.FILTER_REJECT;
      return NodeFilter.FILTER_ACCEPT;
    },
  });
  for (let n = walker.nextNode(); n; n = walker.nextNode()) {
    const text = foldKeep(n.textContent ?? "");
    for (let i = text.indexOf(q); i >= 0; i = text.indexOf(q, i + q.length)) {
      const r = document.createRange();
      r.setStart(n, i);
      r.setEnd(n, i + q.length);
      out.push(r);
    }
  }
  return out;
}

/** Find in the lesson: all occurrences highlighted (CSS Custom Highlight API when there is one),
 *  the current one brought to the middle of the view. */
export function FindBar({ root, onClose, resetKey }: { root: () => HTMLElement | null; onClose: () => void; resetKey: unknown }) {
  const [query, setQuery] = useState("");
  const [ranges, setRanges] = useState<Range[]>([]);
  const [current, setCurrent] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => input.current?.focus(), []);

  const refresh = useCallback(() => {
    const el = root();
    setRanges(el ? findRanges(el, query) : []);
  }, [root, query]);

  // Again when the content changes (other tab, outputs loaded, text edited).
  useEffect(() => {
    refresh();
    const el = root();
    if (!el) return;
    let t = 0;
    const obs = new MutationObserver(() => {
      clearTimeout(t);
      t = window.setTimeout(refresh, 250);
    });
    obs.observe(el, { childList: true, subtree: true, characterData: true });
    return () => {
      obs.disconnect();
      clearTimeout(t);
    };
  }, [refresh, root, resetKey]);
  useEffect(() => setCurrent(0), [query, resetKey]);

  useEffect(() => {
    const reg = registry();
    const cur = ranges[current];
    if (reg && HighlightCtor) {
      reg.set("find", new HighlightCtor(...ranges));
      if (cur) reg.set("find-current", new HighlightCtor(cur));
      else reg.delete("find-current");
    } else if (cur) {
      const sel = window.getSelection();
      sel?.removeAllRanges();
      sel?.addRange(cur);
      input.current?.focus();
    }
    cur?.startContainer.parentElement?.scrollIntoView({ behavior: "smooth", block: "center" });
  }, [ranges, current]);
  useEffect(
    () => () => {
      registry()?.delete("find");
      registry()?.delete("find-current");
    },
    [],
  );

  const step = (d: number) => ranges.length && setCurrent((c) => (c + d + ranges.length) % ranges.length);
  return (
    <div className="flex items-center gap-1" data-find-bar>
      <div className="relative">
        <Search className="pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-zinc-400" />
        <input
          ref={input}
          className="field h-8 w-56 pl-7 text-sm"
          placeholder={t("Cerca nella lezione")}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              step(e.shiftKey ? -1 : 1);
            } else if (e.key === "Escape") {
              e.preventDefault();
              e.stopPropagation();
              onClose();
            }
          }}
          aria-label={t("Cerca nella lezione")}
        />
      </div>
      {query.trim().length >= 2 && (
        <span className="px-1 text-xs whitespace-nowrap text-zinc-500 tabular-nums" aria-live="polite">
          {ranges.length ? t("{current} di {total}", { current: current + 1, total: ranges.length }) : t("nessuno")}
        </span>
      )}
      <button className="rounded p-1 text-zinc-500 hover:bg-zinc-200 disabled:opacity-40 dark:hover:bg-zinc-800" disabled={!ranges.length} onClick={() => step(-1)} title={t("Precedente (Maiusc+Invio)")} aria-label={t("Precedente")}>
        <ChevronUp className="size-4" />
      </button>
      <button className="rounded p-1 text-zinc-500 hover:bg-zinc-200 disabled:opacity-40 dark:hover:bg-zinc-800" disabled={!ranges.length} onClick={() => step(1)} title={t("Successivo (Invio)")} aria-label={t("Successivo")}>
        <ChevronDown className="size-4" />
      </button>
      <button className="rounded p-1 text-zinc-500 hover:bg-zinc-200 dark:hover:bg-zinc-800" onClick={onClose} title={t("Chiudi (Esc)")} aria-label={t("Chiudi la ricerca")}>
        <X className="size-4" />
      </button>
    </div>
  );
}

