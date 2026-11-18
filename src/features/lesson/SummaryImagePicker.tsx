import { useEffect, useState } from "react";
import { fileUrl, type Lesson, type Source, sourceFile } from "../../lib/api";
import { photoTitle } from "./PhotosPanel";
import { t } from "../../i18n";

/** A caption from the photo: its title, or the file name without the extension. */
export function imageCaption(s: Source): string {
  return s.title || s.originalName.replace(/\.[^.]+$/, "");
}

/** The menu that puts an image of the lesson in the summary, at the point chosen. Images not yet
 *  in the summary come first. Opens next to the button that was clicked. */
export function SummaryImagePicker({ lesson, placed, anchor, onPick, onClose }: { lesson: Lesson; placed: Set<string>; anchor: DOMRect; onPick: (imageId: string, caption: string) => void; onClose: () => void }) {
  const [chosen, setChosen] = useState<Source | null>(null);
  const [caption, setCaption] = useState("");
  useEffect(() => {
    const k = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  }, [onClose]);
  const images = lesson.sources.filter((s) => s.kind === "image" && !s.omitted).sort((a, b) => Number(placed.has(a.id)) - Number(placed.has(b.id)));
  const below = anchor.bottom + 380 < window.innerHeight;
  const style: React.CSSProperties = { left: Math.max(8, Math.min(anchor.right - 320, window.innerWidth - 328)), ...(below ? { top: anchor.bottom + 4 } : { bottom: window.innerHeight - anchor.top + 4 }) };
  return (
    <>
      <div className="fixed inset-0 z-30" onClick={onClose} />
      <div className="anim-pop fixed z-40 w-80 rounded-md border border-zinc-200 bg-white p-1 text-sm shadow-lg dark:border-zinc-700 dark:bg-zinc-900" style={style} role="menu" data-image-picker>
        <p className="px-2 pt-1 pb-1.5 text-xs font-semibold text-zinc-500">{chosen ? t("Didascalia") : t("Metti un'immagine nella sintesi")}</p>
        {chosen ? (
          <form
            className="space-y-2 px-2 pb-2"
            onSubmit={(e) => {
              e.preventDefault();
              onPick(chosen.id, caption.trim() || imageCaption(chosen));
            }}
          >
            <img src={fileUrl(sourceFile(lesson, chosen))} alt="" className="max-h-40 w-full rounded object-contain" />
            <input className="field w-full" value={caption} onChange={(e) => setCaption(e.target.value)} autoFocus data-image-caption />
            <div className="flex justify-end gap-2">
              <button type="button" className="btn" onClick={() => setChosen(null)}>
                {t("Indietro")}
              </button>
              <button type="submit" className="btn btn-primary">
                {t("Inserisci")}
              </button>
            </div>
          </form>
        ) : images.length ? (
          <div className="max-h-72 overflow-y-auto">
            {images.map((p) => (
              <button
                key={p.id}
                role="menuitem"
                className="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left hover:bg-zinc-100 dark:hover:bg-zinc-800"
                onClick={() => {
                  setChosen(p);
                  setCaption(imageCaption(p));
                }}
              >
                <img src={fileUrl(sourceFile(lesson, p))} alt="" className="size-10 shrink-0 rounded object-cover" loading="lazy" />
                <span className="min-w-0 flex-1">
                  <span className="block truncate">{photoTitle(p)}</span>
                  <span className="block truncate text-xs text-zinc-500">{placed.has(p.id) ? t("già nella sintesi") : t("non ancora nella sintesi")}</span>
                </span>
              </button>
            ))}
          </div>
        ) : (
          <p className="px-2 pb-2 text-zinc-500">{t("Nella lezione non ci sono immagini: aggiungile nelle Fonti.")}</p>
        )}
      </div>
    </>
  );
}
