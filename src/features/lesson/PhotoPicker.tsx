import { useEffect, useState } from "react";
import { fileUrl, type Lesson, type PhotoLink, sourceFile } from "../../lib/api";
import { clock } from "./moment";
import { photoTitle } from "./PhotosPanel";
import { t } from "../../i18n";

/** Where a photo is now, in a few words. */
export function photoState(link: PhotoLink | undefined, lesson: Lesson): string {
  if (!link) return t("non ancora agganciata");
  if (link.at === null) return link.by === "mano" ? t("sganciata") : t("non agganciata");
  const rec = lesson.sources.filter((s) => s.kind === "audio" || s.kind === "video");
  const name = rec.length > 1 ? ` · ${rec.find((r) => r.id === link.recording)?.originalName ?? ""}` : "";
  return `${clock(link.at)}${name}`;
}

/** The menu that links a photo to a paragraph of a transcript (its minute), or to the minute
 *  being listened to. Opens next to the button that was clicked. */
export function PhotoPicker({
  lesson,
  links,
  at,
  listening,
  anchor,
  onPick,
  onClose,
}: {
  lesson: Lesson;
  links: PhotoLink[];
  /** The minute of the paragraph. */
  at: number;
  /** The minute of the recording being listened to, when it was started. */
  listening: number | null;
  anchor: DOMRect;
  onPick: (imageId: string, at: number) => void;
  onClose: () => void;
}) {
  // Only when it says something more than the paragraph.
  if (listening !== null && Math.floor(listening) === at) listening = null;
  const [where, setWhere] = useState<"paragraph" | "listening">("paragraph");
  useEffect(() => {
    const k = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  }, [onClose]);
  const photos = lesson.sources.filter((s) => s.kind === "image" && !s.omitted);
  const target = where === "listening" && listening !== null ? Math.floor(listening) : at;
  // Below the button, or above when it is low on the screen.
  const below = anchor.bottom + 360 < window.innerHeight;
  const style: React.CSSProperties = { left: Math.max(8, Math.min(anchor.left, window.innerWidth - 328)), ...(below ? { top: anchor.bottom + 4 } : { bottom: window.innerHeight - anchor.top + 4 }) };
  return (
    <>
      <div className="fixed inset-0 z-30" onClick={onClose} />
      <div className="anim-pop fixed z-40 w-80 rounded-md border border-zinc-200 bg-white p-1 text-sm shadow-lg dark:border-zinc-700 dark:bg-zinc-900" style={style} role="menu" data-photo-picker>
        <p className="px-2 pt-1 pb-1.5 text-xs font-semibold text-zinc-500">{t("Aggancia una foto")}</p>
        {listening !== null && (
          <div className="mx-2 mb-1.5 flex rounded-md border border-zinc-300 p-0.5 text-xs dark:border-zinc-700" role="group" aria-label={t("Dove agganciarla")}>
            {(["paragraph", "listening"] as const).map((w) => (
              <button key={w} className={`flex-1 rounded px-2 py-0.5 ${where === w ? "bg-sky-600 text-white" : "hover:bg-zinc-100 dark:hover:bg-zinc-800"}`} onClick={() => setWhere(w)}>
                {w === "paragraph" ? t("Paragrafo {time}", { time: clock(at) }) : t("In ascolto {time}", { time: clock(listening) })}
              </button>
            ))}
          </div>
        )}
        {photos.length ? (
          <div className="max-h-72 overflow-y-auto">
            {photos.map((p) => (
              <button key={p.id} role="menuitem" className="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left hover:bg-zinc-100 dark:hover:bg-zinc-800" onClick={() => onPick(p.id, target)}>
                <img src={fileUrl(sourceFile(lesson, p))} alt="" className="size-10 shrink-0 rounded object-cover" loading="lazy" />
                <span className="min-w-0 flex-1">
                  <span className="block truncate">{photoTitle(p)}</span>
                  <span className="block truncate text-xs text-zinc-500">{photoState(links.find((l) => l.image === p.id), lesson)}</span>
                </span>
              </button>
            ))}
          </div>
        ) : (
          <p className="px-2 pb-2 text-zinc-500">{t("Nella lezione non ci sono foto: aggiungile nelle Fonti.")}</p>
        )}
      </div>
    </>
  );
}
