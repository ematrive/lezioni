// One page of a PDF drawn on a canvas with pdf.js (the slides next to the transcript).
import { GlobalWorkerOptions, getDocument, type PDFDocumentProxy, type RenderTask } from "pdfjs-dist";
import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";
import { useEffect, useRef, useState } from "react";

GlobalWorkerOptions.workerSrc = workerUrl;

/** Documents already opened, by key (lesson + source): opening a 30 MB PDF takes a while. */
const docs = new Map<string, Promise<PDFDocumentProxy>>();

export function usePdf(key: string, load: () => Promise<ArrayBuffer>): { doc: PDFDocumentProxy | null; error: string | null } {
  const [doc, setDoc] = useState<PDFDocumentProxy | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    let p = docs.get(key);
    if (!p) {
      p = load().then((data) => getDocument({ data: new Uint8Array(data) }).promise);
      docs.set(key, p);
      p.catch(() => docs.delete(key));
    }
    p.then(
      (d) => live && setDoc(d),
      (e) => live && setError(String(e?.message ?? e)),
    );
    return () => {
      live = false;
    };
  }, [key, load]);
  return { doc, error };
}

/** Draws page `page` (1-based) at the width of its box. */
export function PdfPage({ doc, page }: { doc: PDFDocumentProxy; page: number }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const box = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);
  const [failed, setFailed] = useState<string | null>(null);
  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const obs = new ResizeObserver(() => setWidth(Math.round(el.clientWidth)));
    obs.observe(el);
    return () => obs.disconnect();
  }, []);
  useEffect(() => {
    if (!width || !canvas.current) return;
    let task: RenderTask | null = null;
    let live = true;
    // A render cancelled by the next page is normal; anything else must show, not leave a blank page.
    const fail = (e: unknown) => {
      if (!live || (e as Error)?.name === "RenderingCancelledException") return;
      console.error("pdf.js", e);
      setFailed(String((e as Error)?.message ?? e));
    };
    setFailed(null);
    doc.getPage(Math.min(Math.max(page, 1), doc.numPages)).then((p) => {
      if (!live || !canvas.current) return;
      const base = p.getViewport({ scale: 1 });
      const ratio = window.devicePixelRatio || 1;
      const vp = p.getViewport({ scale: (width / base.width) * ratio });
      const c = canvas.current;
      c.width = Math.floor(vp.width);
      c.height = Math.floor(vp.height);
      c.style.width = `${width}px`;
      c.style.height = `${Math.floor(vp.height / ratio)}px`;
      task = p.render({ canvas: c, viewport: vp });
      task.promise.catch(fail);
    }).catch(fail);
    return () => {
      live = false;
      task?.cancel();
    };
  }, [doc, page, width]);
  return (
    <div ref={box} className="w-full">
      <canvas ref={canvas} className="rounded border border-zinc-200 bg-white shadow-sm dark:border-zinc-700" data-pdf-page={page} />
      {failed && <p className="mt-1 text-xs text-red-600">Non riesco a disegnare la pagina {page}: {failed}</p>}
    </div>
  );
}
