// User guide inside the app: the chapters of docs/manuale (Italian) and docs/manual (English,
// same file names), bundled at build time with their screenshots, so the guide and the manual on
// GitHub are the same text. The interface language picks the folder.
import { openUrl } from "@tauri-apps/plugin-opener";
import { BookOpen, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { t, uiLang } from "../../i18n";
import { Markdown } from "../../lib/Markdown";
import { usePaneSize } from "../../lib/colwidths";

const pages = import.meta.glob(["../../../docs/manuale/*.md", "../../../docs/manual/*.md"], { query: "?raw", import: "default", eager: true }) as Record<string, string>;
const images = import.meta.glob(["../../../docs/manuale/img/*.png", "../../../docs/manual/img/*.png"], { query: "?url", import: "default", eager: true }) as Record<string, string>;

export interface Chapter {
  file: string;
  title: string;
  text: string;
}

const folder = () => (uiLang() === "en" ? "manual" : "manuale");

/** The chapters in the interface language (read again when the app mounts with a new one). */
function chapters(): Chapter[] {
  const dir = `../../../docs/${folder()}/`;
  return Object.entries(pages)
    .filter(([path]) => path.startsWith(dir))
    .map(([path, text]) => ({ file: path.split("/").pop()!, title: text.match(/^#\s+(.+)$/m)?.[1] ?? path, text }))
    .sort((a, b) => a.file.localeCompare(b.file));
}

/** An English screenshot missing falls back to the Italian one. */
const imageUrl = (src: string) => {
  const f = src.replace(/^\.\//, "");
  return images[`../../../docs/${folder()}/${f}`] ?? images[`../../../docs/manuale/${f}`];
};

const LAST = "lezioni:guide";

export function Guide({ onClose }: { onClose: () => void }) {
  const CHAPTERS = useMemo(chapters, []);
  const [file, setFile] = useState(() => {
    try {
      return localStorage.getItem(LAST) ?? CHAPTERS[0]?.file;
    } catch {
      return CHAPTERS[0]?.file;
    }
  });
  const [q, setQ] = useState("");
  const pane = usePaneSize("guide", { def: 240, min: 180, max: 400 });
  const body = useRef<HTMLDivElement>(null);
  const chapter = CHAPTERS.find((c) => c.file === file) ?? CHAPTERS[0];

  useEffect(() => {
    try {
      if (file) localStorage.setItem(LAST, file);
    } catch {
      /* not persisted */
    }
    body.current?.scrollTo({ top: 0 });
  }, [file]);
  useEffect(() => {
    const k = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  }, [onClose]);

  const found = useMemo(() => {
    const t = q.trim().toLowerCase();
    return t.length < 2 ? null : CHAPTERS.filter((c) => c.text.toLowerCase().includes(t));
  }, [q]);

  if (!chapter) return null;
  const idx = CHAPTERS.indexOf(chapter);
  const go = (href: string) => {
    const f = href.split("#")[0].replace(/^\.\//, "");
    if (CHAPTERS.some((c) => c.file === f)) setFile(f);
  };

  return (
    <div className="anim-fade fixed inset-0 z-40 flex bg-black/30 p-6" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="anim-pop mx-auto flex h-full w-full max-w-6xl overflow-hidden rounded-xl bg-white shadow-2xl dark:bg-zinc-950">
        <nav className="relative flex shrink-0 flex-col border-r border-zinc-200 bg-zinc-50 dark:border-zinc-800 dark:bg-zinc-900" style={{ width: pane.size }}>
          <div className="flex items-center gap-2 px-4 pt-4 pb-2 font-semibold">
            <BookOpen className="size-5 text-sky-600" /> {t("Guida")}
          </div>
          <div className="px-3 pb-2">
            <input className="field" placeholder={t("Cerca nella guida")} value={q} onChange={(e) => setQ(e.target.value)} />
          </div>
          <ul className="min-h-0 flex-1 overflow-auto px-2 pb-3">
            {(found ?? CHAPTERS).map((c) => (
              <li key={c.file}>
                <button className={`w-full rounded-md px-3 py-1.5 text-left ${c.file === chapter.file ? "bg-sky-600 text-white" : "hover:bg-zinc-200 dark:hover:bg-zinc-800"}`} onClick={() => setFile(c.file)}>
                  {c.title}
                </button>
              </li>
            ))}
            {found?.length === 0 && <li className="px-3 py-2 text-sm text-zinc-500">{t("Niente trovato.")}</li>}
          </ul>
          <div className="border-t border-zinc-200 px-4 py-2 text-xs text-zinc-500 dark:border-zinc-800">Lezioni {__APP_VERSION__}</div>
          {pane.handle}
        </nav>
        <div className="flex min-w-0 flex-1 flex-col">
          <div className="flex justify-end px-3 pt-3">
            <button className="rounded p-1.5 text-zinc-500 hover:bg-zinc-100 dark:hover:bg-zinc-800" title={t("Chiudi (Esc)")} onClick={onClose}>
              <X className="size-5" />
            </button>
          </div>
          <div ref={body} className="min-h-0 flex-1 overflow-auto px-10 pb-10">
            <div className="mx-auto max-w-3xl">
              <Markdown text={chapter.text} imgSrc={imageUrl} onLink={(href) => (/^https?:/.test(href) ? openUrl(href) : go(href))} />
              <div className="mt-10 flex justify-between border-t border-zinc-200 pt-4 dark:border-zinc-800">
                {idx > 0 ? (
                  <button className="btn" onClick={() => setFile(CHAPTERS[idx - 1].file)}>
                    ← {CHAPTERS[idx - 1].title}
                  </button>
                ) : (
                  <span />
                )}
                {idx < CHAPTERS.length - 1 && (
                  <button className="btn btn-primary" onClick={() => setFile(CHAPTERS[idx + 1].file)}>
                    {CHAPTERS[idx + 1].title} →
                  </button>
                )}
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
