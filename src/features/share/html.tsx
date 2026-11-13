// Single-file page of one or more lessons (HTML export, and PDF through the browser's print).
// Built with the same Markdown renderer as the app; KaTeX CSS and fonts are embedded, so the
// page works offline and prints formulas correctly.
import katexCss from "katex/dist/katex.min.css?raw";
import { renderToStaticMarkup } from "react-dom/server";
import type { Course, Lesson, MapNode, Outputs } from "../../lib/api";
import { ai, fmtDate } from "../../lib/api";
import { localeOf, tFor } from "../../i18n";
import { Markdown } from "../../lib/Markdown";
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
  study: boolean;
  transcript: boolean;
}

export interface ExportLesson {
  lesson: Lesson;
  outputs: Outputs;
  /** The photos placed in the summary (`![titolo](F3)`): code → data URL. */
  photos?: Record<string, string>;
}

/** The photos placed in the summary, made smaller (1600 px, JPEG) and embedded in the page. */
export async function summaryPhotos(lesson: Lesson, outputs: Outputs): Promise<Record<string, string>> {
  const out: Record<string, string> = {};
  const codes = new Set([...(outputs.summary ?? "").matchAll(/!\[[^\]]*\]\((F\d+)\)/g)].map((m) => m[1]));
  for (const code of codes) {
    const src = lesson.sources.find((s) => s.id === outputs.info?.sources[code] && s.kind === "image" && !s.omitted);
    if (!src) continue;
    try {
      const bytes = await ai.sourceBytes(lesson.id, src.id);
      const bitmap = await createImageBitmap(new Blob([bytes]));
      const k = Math.min(1, 1600 / Math.max(bitmap.width, bitmap.height));
      const canvas = document.createElement("canvas");
      canvas.width = Math.round(bitmap.width * k);
      canvas.height = Math.round(bitmap.height * k);
      canvas.getContext("2d")?.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
      out[code] = canvas.toDataURL("image/jpeg", 0.85);
    } catch {
      // Not readable here (HEIC on Windows, a damaged file): the caption stays.
    }
  }
  return out;
}

function Outline({ node }: { node: MapNode }) {
  return (
    <li>
      {node.etichetta}
      {node.figli?.length ? (
        <ul>
          {node.figli.map((c, i) => (
            <Outline key={i} node={c} />
          ))}
        </ul>
      ) : null}
    </li>
  );
}

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

function LessonDoc({ lesson, outputs, photos, parts, course, drawMap }: ExportLesson & { parts: ExportParts; course: Course; drawMap?: boolean }) {
  const codes = Object.entries(outputs.info?.sources ?? {});
  const name = (id: string) => lesson.sources.find((s) => s.id === id)?.originalName ?? "";
  const transcripts = lesson.sources.filter((s) => outputs.extracts[s.id] !== undefined);
  // In the language of the materials, whatever the interface's.
  const l = course.materialsLanguage ?? "it";
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
      {parts.summary && outputs.summary && (
        <section>
          <h2 className="part">{tFor(l, "Sintesi")}</h2>
          <Markdown text={outputs.summary} exported lang={l} imgSrc={(src) => photos?.[src]} />
          {codes.length > 0 && (
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
          {drawMap ? (
            <div className="map-drawn">
              <MapTree node={outputs.map} />
            </div>
          ) : (
            <ul className="outline">
              <Outline node={outputs.map} />
            </ul>
          )}
        </section>
      )}
      {parts.study && outputs.study && (
        <section>
          <h2 className="part">{tFor(l, "Studio")}</h2>
          {outputs.study.flashcard.length > 0 && <h3>{tFor(l, "Flashcard")}</h3>}
          {outputs.study.flashcard.map((c, i) => (
            <details key={i} className="card">
              <summary>{c.domanda}</summary>
              <div>{c.risposta}</div>
            </details>
          ))}
          {outputs.study.domandeEsame.length > 0 && <h3>{tFor(l, "Domande d'esame")}</h3>}
          <ol>
            {outputs.study.domandeEsame.map((q, i) => (
              <li key={i}>
                <details className="card">
                  <summary>{q.domanda}</summary>
                  <Markdown text={q.traccia} exported lang={l} />
                </details>
              </li>
            ))}
          </ol>
          {outputs.study.glossario.length > 0 && <h3>{tFor(l, "Glossario")}</h3>}
          <dl className="glossary">
            {[...outputs.study.glossario]
              .sort((a, b) => a.termine.localeCompare(b.termine, l))
              .map((g, i) => (
                <div key={i}>
                  <dt>{g.termine}</dt>
                  <dd>{g.definizione}</dd>
                </div>
              ))}
          </dl>
        </section>
      )}
      {parts.transcript && transcripts.length > 0 && (
        <section className="appendix">
          <h2 className="part">{tFor(l, "Trascrizioni e testi delle fonti")}</h2>
          {transcripts.map((s) => (
            <div key={s.id}>
              <h3>{s.originalName}</h3>
              <Markdown text={outputs.extracts[s.id]} exported lang={l} />
            </div>
          ))}
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
.outline, .outline ul { list-style: none; padding-left: 18px; border-left: 2px solid #bae6fd; margin: 4px 0; }
.outline { border: 0; padding: 0; font-size: 15px; }
.outline > li { font-weight: 700; }
.outline li li { font-weight: 400; }
.outline li { margin: 3px 0; }
.card { border: 1px solid #e4e4e7; border-radius: 8px; padding: 8px 12px; margin: 6px 0; }
.card summary { cursor: pointer; font-weight: 600; }
.card > div, .card > .prose-lezioni { margin-top: 6px; color: #3f3f46; }
.glossary div { display: grid; grid-template-columns: 180px 1fr; gap: 12px; padding: 6px 0; border-bottom: 1px solid #f4f4f5; }
.glossary dt { font-weight: 600; } .glossary dd { margin: 0; }
footer { margin-top: 48px; font-size: 11px; color: #a1a1aa; text-align: center; }
@media (max-width: 600px) { main { padding: 24px 16px; } .glossary div { grid-template-columns: 1fr; gap: 2px; } }
@media print {
  body { background: #fff; } main { max-width: none; padding: 0; }
  .lesson + .lesson { break-before: page; border: 0; margin: 0; padding: 0; }
  .appendix { break-before: page; }
  details > summary { list-style: none; } details > summary::-webkit-details-marker { display: none; }
  .card { break-inside: avoid; }
  @page { margin: 18mm 16mm; }
}
`;

/** Full HTML document. `forPrint` opens every flashcard (the answers are inside <details>). */
export function buildHtml(course: Course, lessons: ExportLesson[], parts: ExportParts, forPrint: boolean): string {
  const title = lessons.length === 1 ? `${lessons[0].lesson.title} · ${course.name}` : course.name;
  const l = course.materialsLanguage ?? "it";
  const body = renderToStaticMarkup(
    <main>
      {lessons.map((l) => (
        <LessonDoc key={l.lesson.id} {...l} parts={parts} course={course} drawMap={forPrint} />
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
