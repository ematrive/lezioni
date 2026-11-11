import { ChevronLeft, ChevronRight, Play } from "lucide-react";
import { useCallback, useEffect, useRef } from "react";
import { ai, type SlideAlignment, type Source } from "../../lib/api";
import { PdfPage, usePdf } from "../../lib/PdfPage";

const clock = (secs: number) => {
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(Math.floor(secs / 3600))}:${p(Math.floor(secs / 60) % 60)}:${p(Math.floor(secs % 60))}`;
};

/** The page the teacher is on at `t` seconds: the last step before it. */
export function pageAt(al: SlideAlignment, t: number): number {
  let page = al.steps[0]?.page ?? 1;
  for (const s of al.steps) if (s.at <= t) page = s.page;
  return page;
}

/** The slide of the moment, next to the transcript of a recording. It follows the recording
 *  while it plays, and the text at the top of the view while scrolling. */
export function SlidesPanel({
  lessonId,
  pdf,
  al,
  page,
  setPage,
  player,
  section,
}: {
  lessonId: string;
  pdf: Source;
  al: SlideAlignment;
  page: number;
  setPage: (p: number) => void;
  player: () => HTMLMediaElement | null;
  section: () => HTMLElement | null;
}) {
  const load = useCallback(() => ai.sourceBytes(lessonId, pdf.id), [lessonId, pdf.id]);
  const { doc, error } = usePdf(`${lessonId}/${pdf.id}`, load);
  const pages = doc?.numPages ?? al.pages;
  const follow = useRef(setPage);
  follow.current = setPage;
  // The getters change at every render: read through refs, so the listeners are set once.
  const playerRef = useRef(player);
  playerRef.current = player;
  const sectionRef = useRef(section);
  sectionRef.current = section;

  // While the recording plays: its minute.
  useEffect(() => {
    const p = playerRef.current();
    if (!p) return;
    const on = () => !p.paused && follow.current(pageAt(al, p.currentTime));
    p.addEventListener("timeupdate", on);
    return () => p.removeEventListener("timeupdate", on);
  }, [al]);

  // While scrolling (recording paused): the paragraph at the top of the view.
  useEffect(() => {
    const sec = sectionRef.current();
    if (!sec) return;
    const visible = new Map<Element, number>();
    const obs = new IntersectionObserver(
      (entries) => {
        for (const e of entries) {
          if (e.isIntersecting) visible.set(e.target, Number((e.target as HTMLElement).dataset.t));
          else visible.delete(e.target);
        }
        const p = playerRef.current();
        if (p && !p.paused) return;
        const top = [...visible.entries()].sort((a, b) => a[0].getBoundingClientRect().top - b[0].getBoundingClientRect().top)[0];
        if (top) follow.current(pageAt(al, top[1]));
      },
      { rootMargin: "-25% 0px -55% 0px" },
    );
    sec.querySelectorAll("[data-t]").forEach((el) => obs.observe(el));
    return () => obs.disconnect();
  }, [al]);

  const first = al.steps.find((s) => s.page === page);
  return (
    <div className="space-y-2" data-slides-panel>
      <div className="flex items-center gap-1 text-xs text-zinc-500">
        <button className="rounded p-1 hover:bg-zinc-200 disabled:opacity-30 dark:hover:bg-zinc-800" disabled={page <= 1} onClick={() => setPage(page - 1)} title="Slide precedente" aria-label="Slide precedente">
          <ChevronLeft className="size-4" />
        </button>
        <span className="tabular-nums">
          Slide {page} di {pages}
        </span>
        <button className="rounded p-1 hover:bg-zinc-200 disabled:opacity-30 dark:hover:bg-zinc-800" disabled={page >= pages} onClick={() => setPage(page + 1)} title="Slide successiva" aria-label="Slide successiva">
          <ChevronRight className="size-4" />
        </button>
        <span className="flex-1" />
        {first ? (
          <button
            className="flex items-center gap-1 rounded bg-sky-100 px-1.5 font-mono text-sky-800 hover:bg-sky-200 dark:bg-sky-950 dark:text-sky-300"
            onClick={() => {
              const p = player();
              if (p) {
                p.currentTime = first.at;
                p.play().catch(() => {});
              }
            }}
            title="Ascolta da quando il docente arriva a questa slide"
          >
            <Play className="size-3" /> {clock(first.at)}
          </button>
        ) : (
          <span title="Il docente non sembra parlare di questa slide">non commentata</span>
        )}
      </div>
      {error ? <p className="text-xs text-red-600">Non riesco ad aprire il PDF: {error}</p> : doc ? <PdfPage doc={doc} page={page} /> : <div className="aspect-[4/3] animate-pulse rounded bg-zinc-200 dark:bg-zinc-800" />}
      <p className="truncate text-xs text-zinc-500" title={pdf.originalName}>
        {pdf.originalName}
      </p>
    </div>
  );
}
