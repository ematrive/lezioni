import { ChevronLeft, ChevronRight, Play } from "lucide-react";
import { useCallback } from "react";
import { ai, type SlideAlignment, type Source } from "../../lib/api";
import { PdfPage, usePdf } from "../../lib/PdfPage";
import { clock, useMomentTime } from "./moment";
import { t } from "../../i18n";

/** The page the teacher is on at `t` seconds: the last step before it. */
export function pageAt(al: SlideAlignment, t: number): number {
  let page = al.steps[0]?.page ?? 1;
  for (const s of al.steps) if (s.at <= t) page = s.page;
  return page;
}

/** The slide of the moment, next to the transcript of a recording (see `useMomentTime`). */
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
  useMomentTime(player, section, (t) => setPage(pageAt(al, t)), [al]);

  const first = al.steps.find((s) => s.page === page);
  return (
    <div className="space-y-2" data-slides-panel>
      <div className="flex items-center gap-1 text-xs text-zinc-500">
        <button className="rounded p-1 hover:bg-zinc-200 disabled:opacity-30 dark:hover:bg-zinc-800" disabled={page <= 1} onClick={() => setPage(page - 1)} title={t("Slide precedente")} aria-label={t("Slide precedente")}>
          <ChevronLeft className="size-4" />
        </button>
        <span className="tabular-nums">
          {t("Slide {page} di {pages}", { page, pages })}
        </span>
        <button className="rounded p-1 hover:bg-zinc-200 disabled:opacity-30 dark:hover:bg-zinc-800" disabled={page >= pages} onClick={() => setPage(page + 1)} title={t("Slide successiva")} aria-label={t("Slide successiva")}>
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
            title={t("Ascolta da quando il docente arriva a questa slide")}
          >
            <Play className="size-3" /> {clock(first.at)}
          </button>
        ) : (
          <span title={t("Il docente non sembra parlare di questa slide")}>{t("non commentata")}</span>
        )}
      </div>
      {error ? <p className="text-xs text-red-600">{t("Non riesco ad aprire il PDF: {error}", { error: String(error) })}</p> : doc ? <PdfPage doc={doc} page={page} /> : <div className="aspect-[4/3] animate-pulse rounded bg-zinc-200 dark:bg-zinc-800" />}
      <p className="truncate text-xs text-zinc-500" title={pdf.originalName}>
        {pdf.originalName}
      </p>
    </div>
  );
}
