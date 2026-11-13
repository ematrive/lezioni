import { ChevronLeft, ChevronRight, Play } from "lucide-react";
import { api, fileUrl, type Lesson, sourceFile, type Source } from "../../lib/api";
import { clock, useMomentTime } from "./moment";
import { t } from "../../i18n";

/** A photo linked to a minute of this recording. */
export interface PhotoAt {
  src: Source;
  at: number;
}

/** The title shown for a photo: the user's, or the file name. */
export const photoTitle = (s: Source) => s.title || s.originalName;

/** The photo of the moment: the last one linked at or before `t` (the first before any). */
export function photoAt(photos: PhotoAt[], t: number): string | null {
  let id = photos[0]?.src.id ?? null;
  for (const p of photos) if (p.at <= t) id = p.src.id;
  return id;
}

/** The photo of the moment, next to the transcript of a recording (see `useMomentTime`).
 *  `photos` sorted by minute. */
export function PhotosPanel({
  lesson,
  photos,
  current,
  setCurrent,
  player,
  section,
}: {
  lesson: Lesson;
  photos: PhotoAt[];
  current: string | null;
  setCurrent: (id: string) => void;
  player: () => HTMLMediaElement | null;
  section: () => HTMLElement | null;
}) {
  useMomentTime(
    player,
    section,
    (t) => {
      const id = photoAt(photos, t);
      if (id) setCurrent(id);
    },
    [photos],
  );
  const i = Math.max(0, photos.findIndex((p) => p.src.id === current));
  const p = photos[i];
  if (!p) return null;
  return (
    <div className="space-y-2" data-photos-panel>
      <div className="flex items-center gap-1 text-xs text-zinc-500">
        <button className="rounded p-1 hover:bg-zinc-200 disabled:opacity-30 dark:hover:bg-zinc-800" disabled={i <= 0} onClick={() => setCurrent(photos[i - 1].src.id)} title={t("Foto precedente")} aria-label={t("Foto precedente")}>
          <ChevronLeft className="size-4" />
        </button>
        <span className="tabular-nums">
          {t("Foto {i} di {n}", { i: i + 1, n: photos.length })}
        </span>
        <button className="rounded p-1 hover:bg-zinc-200 disabled:opacity-30 dark:hover:bg-zinc-800" disabled={i >= photos.length - 1} onClick={() => setCurrent(photos[i + 1].src.id)} title={t("Foto successiva")} aria-label={t("Foto successiva")}>
          <ChevronRight className="size-4" />
        </button>
        <span className="flex-1" />
        <button
          className="flex items-center gap-1 rounded bg-sky-100 px-1.5 font-mono text-sky-800 hover:bg-sky-200 dark:bg-sky-950 dark:text-sky-300"
          onClick={() => {
            const pl = player();
            if (pl) {
              pl.currentTime = p.at;
              pl.play().catch(() => {});
            }
          }}
          title={t("Ascolta da questo punto")}
        >
          <Play className="size-3" /> {clock(p.at)}
        </button>
      </div>
      <button className="block w-full" onClick={() => api.openSource(lesson.id, p.src.id)} title={t("Apri la foto")}>
        <img src={fileUrl(sourceFile(lesson, p.src))} alt={photoTitle(p.src)} className="max-h-[50vh] w-full rounded border border-zinc-200 bg-zinc-100 object-contain dark:border-zinc-700 dark:bg-zinc-900" />
      </button>
      <p className="text-sm font-medium">{photoTitle(p.src)}</p>
      {p.src.description && <p className="text-xs text-zinc-500">{p.src.description}</p>}
    </div>
  );
}
