// Single-file page of one or more lessons (HTML export, and PDF through the browser's print).
// Built with the same Markdown renderer as the app; KaTeX CSS and fonts are embedded, so the
// page works offline and prints formulas correctly.
import katexCss from "katex/dist/katex.min.css?raw";
import { renderToStaticMarkup } from "react-dom/server";
import type { Bookmark, Course, CourseSummary, Lesson, MapNode, Outputs, Source } from "../../lib/api";
import { ai, fmtDate } from "../../lib/api";
import { localeOf, tFor, type Lang } from "../../i18n";
import { Markdown, MARK_ICON, toSecs } from "../../lib/Markdown";
import { renderPdfPages } from "../../lib/PdfPage";
import { hmsOf } from "../../lib/time";
import { imageRefId } from "../../lib/summaryImages";
import proseCss from "../../lib/prose.css?raw";

const fonts = import.meta.glob("../../../node_modules/katex/dist/fonts/*.woff2", { query: "?inline", import: "default", eager: true }) as Record<string, string>;

function inlineKatexFonts(css: string): string {
  const byName = Object.fromEntries(Object.entries(fonts).map(([k, v]) => [k.split("/").pop()!, v]));
  // Each @font-face lists woff2, woff and ttf: keep only the embedded woff2.
  return css.replace(/url\(fonts\/([^)]+?\.woff2)\) format\("woff2"\)(?:,url\([^)]*\) format\("[^"]*"\))*/g, (m, file: string) => (byName[file] ? `url(${byName[file]}) format("woff2")` : m));
}

export interface ExportParts {
  summary: boolean;
  map: boolean;
  flashcards: boolean;
  questions: boolean;
  glossary: boolean;
  transcript: boolean;
  /** the photos the summary places in its text */
  summaryPhotos: boolean;
  /** the handwritten notes read from slides and photos, gathered in one section */
  handwritten: boolean;
  /** every photo of the lesson, in an appendix */
  photos: boolean;
  /** the pages of the teacher's slides, in an appendix */
  slides: boolean;
  /** the student's notes on the lesson */
  notes: boolean;
  /** the student's bookmarks in the recording */
  bookmarks: boolean;
  /** the `[F1 00:12:30]` references and the legend of the sources */
  refs: boolean;
}

export const DEFAULT_PARTS: ExportParts = {
  summary: true,
  map: true,
  flashcards: true,
  questions: true,
  glossary: true,
  transcript: false,
  summaryPhotos: true,
  handwritten: false,
  photos: false,
  slides: false,
  notes: false,
  bookmarks: false,
  refs: true,
};

export interface GalleryPhoto {
  /** its code in the summary (`F3`), when it has one */
  code?: string;
  title: string;
  description?: string;
  /** where it is linked in a recording: "F1 00:12:30" */
  at?: string;
  url: string;
}

export interface ExportLesson {
  lesson: Lesson;
  outputs: Outputs;
  /** The photos placed in the summary (`![titolo](F3)`): code → data URL. */
  photos?: Record<string, string>;
  /** Every photo of the lesson (`parts.photos`). */
  gallery?: GalleryPhoto[];
  /** The pages of each PDF of slides (`parts.slides`). */
  slides?: { name: string; pages: string[] }[];
}

const IMAGE_TYPES: Record<string, string> = { jpg: "image/jpeg", jpeg: "image/jpeg", png: "image/png", gif: "image/gif", webp: "image/webp", heic: "image/heic", heif: "image/heif", avif: "image/avif", bmp: "image/bmp", tif: "image/tiff", tiff: "image/tiff" };

/** The media type of an image from its file name ("" when unknown: the decoder sniffs it). */
function imageType(file: string): string {
  return IMAGE_TYPES[file.split(".").pop()?.toLowerCase() ?? ""] ?? "";
}

/** Decoded through an <img>, the same decoder that shows the image in the app (HEIC in WebKit,
 *  where createImageBitmap may refuse it). A blob: URL has our origin: the canvas stays clean. */
async function viaImgElement(blob: Blob): Promise<HTMLImageElement> {
  const url = URL.createObjectURL(blob);
  try {
    const img = new Image();
    img.src = url;
    await img.decode();
    return img;
  } finally {
    // Revoked once decoded: the decoded pixels stay with the element.
    setTimeout(() => URL.revokeObjectURL(url), 0);
  }
}

/** A photo made smaller (`max` px on the longest side, JPEG), as a data URL: any browser can show
 *  it, also one that cannot read the original (Chrome and HEIC). */
async function shrinkImage(bytes: ArrayBuffer, max: number, file: string): Promise<string> {
  const blob = new Blob([bytes], { type: imageType(file) });
  let pic: { width: number; height: number; draw: (c: CanvasRenderingContext2D, w: number, h: number) => void };
  try {
    const b = await createImageBitmap(blob);
    pic = { width: b.width, height: b.height, draw: (c, w, h) => c.drawImage(b, 0, 0, w, h) };
  } catch {
    const img = await viaImgElement(blob);
    pic = { width: img.naturalWidth, height: img.naturalHeight, draw: (c, w, h) => c.drawImage(img, 0, 0, w, h) };
  }
  if (!pic.width || !pic.height) throw new Error("empty image");
  const k = Math.min(1, max / Math.max(pic.width, pic.height));
  const canvas = document.createElement("canvas");
  canvas.width = Math.max(1, Math.round(pic.width * k));
  canvas.height = Math.max(1, Math.round(pic.height * k));
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("no canvas");
  // JPEG has no transparency: white under a transparent PNG instead of black.
  ctx.fillStyle = "#fff";
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  pic.draw(ctx, canvas.width, canvas.height);
  return canvas.toDataURL("image/jpeg", 0.85);
}

/** The name of a source in the warnings: its title, or the file it came from. */
const sourceLabel = (s: Source) => s.title || s.originalName;

/** The photos placed in the summary, made smaller (1600 px, JPEG) and embedded in the page; the
 *  names of those that could not be read go to `failed`. */
export async function summaryPhotos(lesson: Lesson, outputs: Outputs, failed: string[] = []): Promise<Record<string, string>> {
  const out: Record<string, string> = {};
  // `F3` (placed by the model) or `img:<source id>` (placed by hand).
  const codes = new Set([...(outputs.summary ?? "").matchAll(/!\[[^\]]*\]\((F\d+|img:[\w-]+)\)/g)].map((m) => m[1]));
  for (const code of codes) {
    const id = imageRefId(code) ?? outputs.info?.sources[code];
    const src = lesson.sources.find((s) => s.id === id && s.kind === "image" && !s.omitted);
    if (!src) continue;
    try {
      out[code] = await shrinkImage(await ai.sourceBytes(lesson.id, src.id), 1600, src.file);
    } catch (e) {
      // Not readable here (HEIC on Windows, a damaged file): the caption stays, and the person is told.
      console.warn("image", src.file, e);
      failed.push(sourceLabel(src));
    }
  }
  return out;
}

/** Every photo of the lesson, with its title and the minute it is linked to. */
export async function lessonPhotos(lesson: Lesson, outputs: Outputs, failed: string[] = []): Promise<GalleryPhoto[]> {
  const codeOf = Object.fromEntries(Object.entries(outputs.info?.sources ?? {}).map(([c, id]) => [id, c]));
  const out: GalleryPhoto[] = [];
  for (const s of lesson.sources.filter((s) => s.kind === "image" && !s.omitted)) {
    try {
      const url = await shrinkImage(await ai.sourceBytes(lesson.id, s.id), 1600, s.file);
      const link = outputs.photos.find((p) => p.image === s.id && p.at !== null);
      const rec = link ? (codeOf[link.recording] ?? lesson.sources.find((x) => x.id === link.recording)?.originalName) : undefined;
      out.push({ code: codeOf[s.id], title: s.title || s.originalName, description: s.description || undefined, at: link && rec ? `${rec} ${hmsOf(link.at!)}` : undefined, url });
    } catch (e) {
      // Not readable here (HEIC on Windows, a damaged file): left out, and the person is told.
      console.warn("image", s.file, e);
      if (!failed.includes(sourceLabel(s))) failed.push(sourceLabel(s));
    }
  }
  return out;
}

/** The pages of the teacher's slides (PDFs marked as slides), drawn as images. */
export async function slidePages(lesson: Lesson, failed: string[] = []): Promise<{ name: string; pages: string[] }[]> {
  const out: { name: string; pages: string[] }[] = [];
  for (const s of lesson.sources.filter((s) => s.kind === "pdf" && s.role === "slide" && !s.omitted)) {
    try {
      out.push({ name: s.originalName, pages: await renderPdfPages(await ai.sourceBytes(lesson.id, s.id), 1400) });
    } catch (e) {
      console.error("pdf.js", e);
      failed.push(sourceLabel(s));
    }
  }
  return out;
}

/** The handwritten notes of a text (`*(nota a mano: …)*`), with the page they are on (`## Pagina N`). */
export function handwrittenNotes(text: string): { page: number | null; notes: string[] }[] {
  const out: { page: number | null; notes: string[] }[] = [];
  let page: number | null = null;
  for (const line of text.split("\n")) {
    const h = line.match(/^#{1,6}\s*(?:Pagina|Page)\s+(\d+)\s*$/i);
    if (h) {
      page = Number(h[1]);
      continue;
    }
    for (const m of line.matchAll(/([*_])\(\s*(?:nota a mano|handwritten)\b.*?\)\1/gi)) {
      const last = out[out.length - 1];
      if (last && last.page === page) last.notes.push(m[0]);
      else out.push({ page, notes: [m[0]] });
    }
  }
  return out;
}

/** The paragraph of a transcript a minute falls in: the last one starting at or before it. */
function paragraphAt(text: string, at: number): string | null {
  let best: { start: number; text: string } | null = null;
  for (const p of text.split(/\n\s*\n/)) {
    const m = p.match(/^\s*\[(\d{1,2}:\d{2}(?::\d{2})?)\]\s*/);
    if (!m) continue;
    const start = toSecs(m[1]);
    if (start <= at && (!best || start >= best.start)) best = { start, text: p.slice(m[0].length).trim() };
  }
  return best?.text ?? null;
}

const MARK_CLASS: Record<Bookmark["kind"], string> = { importante: "mk-imp", dubbio: "mk-dub", nota: "mk-not" };
const markWord = (l: Lang, k: Bookmark["kind"]) => (k === "importante" ? tFor(l, "Importante") : k === "dubbio" ? tFor(l, "Non ho capito") : tFor(l, "Nota|segnaposto"));

/** The concept map drawn as in the app: boxes joined by lines, colours by level (static CSS). */
export function MapTree({ node, depth = 0 }: { node: MapNode; depth?: number }) {
  const kids = node.figli ?? [];
  return (
    <div className="mt-node">
      <span className={`mt-box mt-d${Math.min(depth, 3)}`}>{node.etichetta}</span>
      {kids.length > 0 && (
        <div className="mt-kids">
          {kids.map((k, i) => (
            <div key={i} className="mt-kid">
              <MapTree node={k} depth={depth + 1} />
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

const MAP_CSS = `
.mt-node { display: flex; align-items: center; }
.mt-box { flex-shrink: 0; max-width: 220px; padding: 5px 11px; border-radius: 8px; font-size: 13px; line-height: 1.3; break-inside: avoid; }
.mt-d0 { background: #0284c7; color: #fff; font-weight: 700; font-size: 15px; }
.mt-d1 { background: #e0f2fe; color: #0c4a6e; font-weight: 600; }
.mt-d2 { background: #fff; border: 1px solid #d4d4d8; }
.mt-d3 { color: #52525b; }
.mt-kids { display: flex; flex-direction: column; gap: 6px; margin-left: 22px; padding: 3px 0 3px 14px; border-left: 2px solid #7dd3fc; }
.mt-kid { position: relative; break-inside: avoid; }
.mt-kid::before { content: ""; position: absolute; top: 50%; left: -14px; width: 14px; border-top: 2px solid #7dd3fc; }
* { -webkit-print-color-adjust: exact; print-color-adjust: exact; }
`;

function LessonDoc({ lesson, outputs, photos, gallery, slides, parts, course }: ExportLesson & { parts: ExportParts; course: Course }) {
  const codes = Object.entries(outputs.info?.sources ?? {});
  const codeOf = Object.fromEntries(codes.map(([c, id]) => [id, c]));
  const name = (id: string) => lesson.sources.find((s) => s.id === id)?.originalName ?? "";
  const transcripts = lesson.sources.filter((s) => outputs.extracts[s.id] !== undefined);
  const handwritten = parts.handwritten ? transcripts.map((s) => ({ source: s, groups: handwrittenNotes(outputs.extracts[s.id]) })).filter((h) => h.groups.length) : [];
  const bookmarks = parts.bookmarks ? [...outputs.bookmarks].sort((a, b) => a.sourceId.localeCompare(b.sourceId) || a.at - b.at) : [];
  const study = outputs.study;
  const hasStudy = study && ((parts.flashcards && study.flashcard.length) || (parts.questions && study.domandeEsame.length) || (parts.glossary && study.glossario.length));
  // In the language of the materials, whatever the interface's.
  const l = course.materialsLanguage ?? "it";
  const md = (text: string) => <Markdown text={text} exported lang={l} hideRefs={!parts.refs} />;
  // Short texts written inline in the app too (question titles, glossary): formulas and bold kept.
  const mdInline = (text: string) => <Markdown text={text} exported inline lang={l} hideRefs={!parts.refs} />;
  return (
    <article className="lesson">
      <header>
        <p className="course">
          {course.name}
          {course.teacher ? ` · ${course.teacher}` : ""}
        </p>
        <h1>{lesson.title}</h1>
        <p className="date">{fmtDate(lesson.date, l)}</p>
      </header>
      {parts.notes && lesson.notes.trim() && (
        <section className="notes">
          <h2 className="part">{tFor(l, "Le mie note")}</h2>
          <Markdown text={lesson.notes} exported lang={l} />
        </section>
      )}
      {parts.summary && outputs.summary && (
        <section>
          <h2 className="part">{tFor(l, "Sintesi")}</h2>
          <Markdown text={outputs.summary} exported lang={l} hideRefs={!parts.refs} imgSrc={(src) => (parts.summaryPhotos ? photos?.[src] : undefined)} />
          {parts.refs && codes.length > 0 && (
            <p className="legend">
              {tFor(l, "Fonti")}:{" "}
              {codes.map(([c, id], i) => (
                <span key={c}>
                  {i ? " · " : ""}
                  <b>{c}</b> {name(id)}
                </span>
              ))}
            </p>
          )}
        </section>
      )}
      {parts.map && outputs.map && (
        <section>
          <h2 className="part">{tFor(l, "Mappa concettuale")}</h2>
          {/* Drawn as in the app, on the web page too (it scrolls sideways on a phone). */}
          <div className="map-drawn">
            <MapTree node={outputs.map} />
          </div>
        </section>
      )}
      {hasStudy && study && (
        <section>
          <h2 className="part">{tFor(l, "Studio")}</h2>
          {parts.flashcards && study.flashcard.length > 0 && (
            <>
              <h3>{tFor(l, "Flashcard")}</h3>
              {study.flashcard.map((c, i) => (
                <details key={i} className="card">
                  <summary>{mdInline(c.domanda)}</summary>
                  {md(c.risposta)}
                </details>
              ))}
            </>
          )}
          {parts.questions && study.domandeEsame.length > 0 && (
            <>
              <h3>{tFor(l, "Domande d'esame")}</h3>
              <ol>
                {study.domandeEsame.map((q, i) => (
                  <li key={i}>
                    <details className="card">
                      <summary>{mdInline(q.domanda)}</summary>
                      {md(q.traccia)}
                    </details>
                  </li>
                ))}
              </ol>
            </>
          )}
          {parts.glossary && study.glossario.length > 0 && (
            <>
              <h3>{tFor(l, "Glossario")}</h3>
              <dl className="glossary">
                {[...study.glossario]
                  .sort((a, b) => a.termine.localeCompare(b.termine, l))
                  .map((g, i) => (
                    <div key={i}>
                      <dt>{mdInline(g.termine)}</dt>
                      <dd>{mdInline(g.definizione)}</dd>
                    </div>
                  ))}
              </dl>
            </>
          )}
        </section>
      )}
      {handwritten.length > 0 && (
        <section>
          <h2 className="part">{tFor(l, "Appunti a mano")}</h2>
          {handwritten.map(({ source, groups }) => (
            <div key={source.id} className="hand-source">
              <h3>
                {parts.refs && codeOf[source.id] ? <span className="ref">{codeOf[source.id]}</span> : null} {source.title || source.originalName}
              </h3>
              {groups.map((g, i) => (
                <div key={i} className="hand-group">
                  {g.page !== null && <p className="hand-page">{tFor(l, "Pagina {page}", { page: g.page })}</p>}
                  <Markdown text={g.notes.join("\n\n")} exported lang={l} />
                </div>
              ))}
            </div>
          ))}
        </section>
      )}
      {bookmarks.length > 0 && (
        <section>
          <h2 className="part">{tFor(l, "I miei segnaposti")}</h2>
          {bookmarks.map((b) => {
            const text = outputs.extracts[b.sourceId];
            const para = text ? paragraphAt(text, b.at) : null;
            return (
              <div key={b.id} className={`mark ${MARK_CLASS[b.kind]}`}>
                <p className="mark-head">
                  <b>
                    {MARK_ICON[b.kind]} {markWord(l, b.kind)}
                  </b>{" "}
                  · {(parts.refs && codeOf[b.sourceId]) || name(b.sourceId)} {hmsOf(b.at)}
                  {b.note && <span className="mark-note"> — {b.note}</span>}
                </p>
                {para && <div className="mark-text">{md(para)}</div>}
              </div>
            );
          })}
        </section>
      )}
      {parts.transcript && transcripts.length > 0 && (
        <section className="appendix">
          <h2 className="part">{tFor(l, "Trascrizioni e testi delle fonti")}</h2>
          {transcripts.map((s) => (
            <div key={s.id}>
              <h3>{s.originalName}</h3>
              {/* With the bookmarks chosen too: the paragraphs they are on stand out, as in the app. */}
              {parts.bookmarks && outputs.bookmarks.some((b) => b.sourceId === s.id) ? (
                // The minutes become links to find the paragraphs: kept visible, they are the transcript's own.
                <Markdown text={outputs.extracts[s.id]} exported lang={l} source={s.id} marks={outputs.bookmarks.filter((b) => b.sourceId === s.id)} />
              ) : (
                md(outputs.extracts[s.id])
              )}
            </div>
          ))}
        </section>
      )}
      {parts.photos && gallery && gallery.length > 0 && (
        <section className="appendix">
          <h2 className="part">{tFor(l, "Foto")}</h2>
          <div className="gallery">
            {gallery.map((g, i) => (
              <figure key={i}>
                <img src={g.url} alt={g.title} />
                <figcaption>
                  {parts.refs && g.code ? <span className="ref">{g.code}</span> : null} <b>{g.title}</b>
                  {parts.refs && g.at ? <span className="photo-at"> · {g.at}</span> : null}
                  {g.description && <span className="photo-desc">{g.description}</span>}
                </figcaption>
              </figure>
            ))}
          </div>
        </section>
      )}
      {parts.slides && slides && slides.length > 0 && (
        <section className="appendix">
          <h2 className="part">{tFor(l, "Slide")}</h2>
          {slides.map((d, i) => (
            <div key={i}>
              <h3>{d.name}</h3>
              <div className="slides">
                {d.pages.map((url, n) => (
                  <figure key={n}>
                    <img src={url} alt={tFor(l, "Pagina {page}", { page: n + 1 })} />
                    <figcaption>{n + 1}</figcaption>
                  </figure>
                ))}
              </div>
            </div>
          ))}
        </section>
      )}
    </article>
  );
}

/** The course summary (built from the lessons' summaries) with its exam questions, before the lessons. */
function CourseDoc({ course, summary, lessons, parts }: { course: Course; summary: CourseSummary; lessons: Lesson[]; parts: ExportParts }) {
  const l = course.materialsLanguage ?? "it";
  const refs = summary.info?.refs ?? {};
  const title = (code: string) => lessons.find((x) => x.id === refs[code])?.title;
  return (
    <article className="lesson course-summary">
      <header>
        <p className="course">
          {course.name}
          {course.teacher ? ` · ${course.teacher}` : ""}
        </p>
        <h1>{tFor(l, "Sintesi del corso")}</h1>
      </header>
      {summary.summary && <Markdown text={summary.summary} exported lang={l} hideRefs={!parts.refs} />}
      {summary.questions.length > 0 && (
        <section>
          <h2 className="part">{tFor(l, "Domande d'esame del corso")}</h2>
          <ol>
            {summary.questions.map((q, i) => (
              <li key={i}>
                <details className="card">
                  <summary>
                    <Markdown text={q.domanda} exported inline lang={l} />
                  </summary>
                  <Markdown text={q.traccia} exported lang={l} hideRefs={!parts.refs} />
                  {q.lezioni.length > 0 && (
                    <p className="review-lessons">
                      {tFor(l, "Da ripassare:")}{" "}
                      {q.lezioni.map((c, k) => (
                        <span key={c}>
                          {k ? " · " : ""}
                          {parts.refs && <b>{c}</b>} {title(c) ?? (parts.refs ? "" : c)}
                        </span>
                      ))}
                    </p>
                  )}
                </details>
              </li>
            ))}
          </ol>
        </section>
      )}
    </article>
  );
}

const PAGE_CSS = `
:root { color-scheme: light; }
body { margin: 0; background: #fafafa; color: #18181b; font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Inter, sans-serif; }
main { max-width: 820px; margin: 0 auto; padding: 40px 24px 80px; background: #fff; }
.lesson + .lesson { margin-top: 56px; padding-top: 32px; border-top: 3px solid #0284c7; }
header .course { margin: 0; color: #0369a1; font-weight: 600; text-transform: uppercase; letter-spacing: .04em; font-size: 12px; }
header h1 { margin: 4px 0; font-size: 30px; }
header .date { margin: 0 0 24px; color: #71717a; }
h2.part { font-size: 13px; text-transform: uppercase; letter-spacing: .06em; color: #0369a1; border: 0; margin: 36px 0 8px; }
.ref { font-family: ui-monospace, Menlo, monospace; font-size: 11px; background: #e0f2fe; color: #075985; border-radius: 4px; padding: 0 4px; }
.legend { font-size: 13px; color: #52525b; margin-top: 16px; }
.review-lessons { font-size: 13px; color: #52525b; margin: 6px 0 0; }
.map-drawn { overflow-x: auto; padding-bottom: 4px; }
p[data-mark] { border-left: 4px solid; border-radius: 0 6px 6px 0; padding: 4px 10px; margin-left: -14px; }
p[data-mark="importante"] { border-color: #fbbf24; background: #fffbeb; }
p[data-mark="dubbio"] { border-color: #fb7185; background: #fff1f2; }
p[data-mark="nota"] { border-color: #38bdf8; background: #f0f9ff; }
.card { border: 1px solid #e4e4e7; border-radius: 8px; padding: 8px 12px; margin: 6px 0; }
.card summary { cursor: pointer; font-weight: 600; }
.card > div, .card > .prose-lezioni { margin-top: 6px; color: #3f3f46; }
.glossary div { display: grid; grid-template-columns: 180px 1fr; gap: 12px; padding: 6px 0; border-bottom: 1px solid #f4f4f5; }
.glossary dt { font-weight: 600; } .glossary dd { margin: 0; }
.notes { background: #f4f4f5; border-radius: 8px; padding: 2px 14px 6px; }
.hand-source + .hand-source { margin-top: 12px; }
.hand-page { margin: 10px 0 0; font-size: 12px; color: #71717a; text-transform: uppercase; letter-spacing: .04em; }
.mark { border-left: 4px solid; border-radius: 0 6px 6px 0; padding: 4px 10px; margin: 8px 0; break-inside: avoid; }
.mark-head { margin: 2px 0; font-size: 14px; }
.mark-note { color: #3f3f46; }
.mark-text { font-size: 14px; color: #52525b; }
.mark-text p { margin: 4px 0; }
.mk-imp { border-color: #fbbf24; background: #fffbeb; }
.mk-dub { border-color: #fb7185; background: #fff1f2; }
.mk-not { border-color: #38bdf8; background: #f0f9ff; }
.prose-lezioni img { max-width: 100%; height: auto; }
[data-summary-photo] { display: block; margin: 16px 0; text-align: center; break-inside: avoid; }
[data-summary-photo] img { display: block; margin: 0 auto; border: 1px solid #e4e4e7; border-radius: 6px; }
[data-summary-photo] > span { display: block; margin-top: 4px; font-size: 13px; color: #71717a; }
.gallery { display: grid; grid-template-columns: repeat(auto-fill, minmax(220px, 1fr)); gap: 16px; }
.gallery figure, .slides figure { margin: 0; break-inside: avoid; }
.gallery img { width: 100%; border-radius: 6px; border: 1px solid #e4e4e7; }
.gallery figcaption { font-size: 13px; margin-top: 4px; }
.photo-at { color: #71717a; }
.photo-desc { display: block; color: #52525b; }
.slides { display: grid; grid-template-columns: 1fr; gap: 14px; }
.slides img { width: 100%; border: 1px solid #e4e4e7; }
.slides figcaption { text-align: center; font-size: 11px; color: #a1a1aa; }
footer { margin-top: 48px; font-size: 11px; color: #a1a1aa; text-align: center; }
@media (max-width: 600px) { main { padding: 24px 16px; } .glossary div { grid-template-columns: 1fr; gap: 2px; } }
@media print {
  body { background: #fff; } main { max-width: none; padding: 0; }
  .lesson + .lesson { break-before: page; border: 0; margin: 0; padding: 0; }
  .appendix { break-before: page; }
  details > summary { list-style: none; } details > summary::-webkit-details-marker { display: none; }
  .card { break-inside: avoid; }
  .slides img { max-height: 115mm; width: auto; max-width: 100%; display: block; margin: 0 auto; }
  /* A tall photo stays on one page, its caption with it (A4 less the margins). */
  [data-summary-photo] img { width: auto; max-height: 225mm; }
  @page { margin: 18mm 16mm; }
}
`;

/** Full HTML document. `forPrint` opens every flashcard (the answers are inside <details>).
 *  `courseSummary`: a whole course exported, its summary goes before the lessons. */
export function buildHtml(course: Course, lessons: ExportLesson[], parts: ExportParts, forPrint: boolean, courseSummary?: CourseSummary | null): string {
  const title = lessons.length === 1 ? `${lessons[0].lesson.title} · ${course.name}` : course.name;
  const l = course.materialsLanguage ?? "it";
  const body = renderToStaticMarkup(
    <main>
      {parts.summary && (courseSummary?.summary || courseSummary?.questions.length) ? <CourseDoc course={course} summary={courseSummary} lessons={lessons.map((x) => x.lesson)} parts={parts} /> : null}
      {lessons.map((l) => (
        <LessonDoc key={l.lesson.id} {...l} parts={parts} course={course} />
      ))}
      <footer>{tFor(l, "Creato con Lezioni")} · {new Date().toLocaleDateString(localeOf(l))}</footer>
    </main>,
  );
  const html = forPrint ? body.replace(/<details /g, "<details open ") : body;
  const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;");
  return `<!doctype html>
<html lang="${l}"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>${esc(title)}</title>
<style>${inlineKatexFonts(katexCss)}</style>
<style>${proseCss}${PAGE_CSS}${MAP_CSS}</style>
</head><body>${html}</body></html>`;
}

/** Just the concept map of a lesson, on a landscape page, for printing to PDF. */
export function buildMapHtml(course: Course, lesson: Lesson, map: MapNode): string {
  const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;");
  const l = course.materialsLanguage ?? "it";
  const body = renderToStaticMarkup(
    <main className="map-page">
      <header>
        <p className="course">
          {course.name}
          {course.teacher ? ` · ${course.teacher}` : ""}
        </p>
        <h1>{lesson.title}</h1>
        <p className="date">
          {tFor(l, "Mappa concettuale")} · {fmtDate(lesson.date, l)}
        </p>
      </header>
      <MapTree node={map} />
      <footer>{tFor(l, "Creato con Lezioni")} · {new Date().toLocaleDateString(localeOf(l))}</footer>
    </main>,
  );
  return `<!doctype html>
<html lang="${l}"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>${esc(`${tFor(l, "Mappa")} · ${lesson.title}`)}</title>
<style>${PAGE_CSS}${MAP_CSS}
main.map-page { max-width: none; padding: 24px 32px; }
main.map-page header h1 { font-size: 22px; }
@media print { @page { size: A4 landscape; margin: 12mm; } main.map-page { padding: 0; } }
</style>
</head><body>${body}</body></html>`;
}
