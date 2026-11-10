// Single-file page of one or more lessons (HTML export, and PDF through the browser's print).
// Built with the same Markdown renderer as the app; KaTeX CSS and fonts are embedded, so the
// page works offline and prints formulas correctly.
import katexCss from "katex/dist/katex.min.css?raw";
import { renderToStaticMarkup } from "react-dom/server";
import type { Course, Lesson, MapNode, Outputs } from "../../lib/api";
import { fmtDate } from "../../lib/api";
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

function LessonDoc({ lesson, outputs, parts, course }: ExportLesson & { parts: ExportParts; course: Course }) {
  const codes = Object.entries(outputs.info?.sources ?? {});
  const name = (id: string) => lesson.sources.find((s) => s.id === id)?.originalName ?? "";
  const transcripts = lesson.sources.filter((s) => outputs.extracts[s.id] !== undefined);
  return (
    <article className="lesson">
      <header>
        <p className="course">
          {course.name}
          {course.teacher ? ` · ${course.teacher}` : ""}
        </p>
        <h1>{lesson.title}</h1>
        <p className="date">{fmtDate(lesson.date)}</p>
      </header>
      {parts.summary && outputs.summary && (
        <section>
          <h2 className="part">Sintesi</h2>
          <Markdown text={outputs.summary} exported />
          {codes.length > 0 && (
            <p className="legend">
              Fonti:{" "}
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
          <h2 className="part">Mappa concettuale</h2>
          <ul className="outline">
            <Outline node={outputs.map} />
          </ul>
        </section>
      )}
      {parts.study && outputs.study && (
        <section>
          <h2 className="part">Studio</h2>
          {outputs.study.flashcard.length > 0 && <h3>Flashcard</h3>}
          {outputs.study.flashcard.map((c, i) => (
            <details key={i} className="card">
              <summary>{c.domanda}</summary>
              <div>{c.risposta}</div>
            </details>
          ))}
          {outputs.study.domandeEsame.length > 0 && <h3>Domande d'esame</h3>}
          <ol>
            {outputs.study.domandeEsame.map((q, i) => (
              <li key={i}>
                <details className="card">
                  <summary>{q.domanda}</summary>
                  <Markdown text={q.traccia} exported />
                </details>
              </li>
            ))}
          </ol>
          {outputs.study.glossario.length > 0 && <h3>Glossario</h3>}
          <dl className="glossary">
            {[...outputs.study.glossario]
              .sort((a, b) => a.termine.localeCompare(b.termine, "it"))
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
          <h2 className="part">Trascrizioni e testi delle fonti</h2>
          {transcripts.map((s) => (
            <div key={s.id}>
              <h3>{s.originalName}</h3>
              <Markdown text={outputs.extracts[s.id]} exported />
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
  const body = renderToStaticMarkup(
    <main>
      {lessons.map((l) => (
        <LessonDoc key={l.lesson.id} {...l} parts={parts} course={course} />
      ))}
      <footer>Creato con Lezioni · {new Date().toLocaleDateString("it-IT")}</footer>
    </main>,
  );
  const html = forPrint ? body.replace(/<details /g, "<details open ") : body;
  const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;");
  return `<!doctype html>
<html lang="it"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>${esc(title)}</title>
<style>${inlineKatexFonts(katexCss)}</style>
<style>${proseCss}${PAGE_CSS}</style>
</head><body>${html}</body></html>`;
}
