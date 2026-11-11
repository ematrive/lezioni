import "katex/dist/katex.min.css";
import { useMemo } from "react";
import ReactMarkdown from "react-markdown";
import rehypeKatex from "rehype-katex";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";

/** Seconds from "hh:mm:ss" or "mm:ss". */
export function toSecs(t: string): number {
  return t.split(":").map(Number).reduce((a, b) => a * 60 + b, 0);
}

const TIME = String.raw`\d{1,2}:\d{2}(?::\d{2})?`;

/**
 * Turns the references the model writes into links the app handles:
 * - `[00:12:30]` at the start of a transcript paragraph → `#t=<source>@750` (with `source` set);
 * - `[F1 00:12:30]`, `[F3]`, `[F1 00:12:30; F2]` in the summary → `#t=F1@750`, `#f=F3`;
 * - `[L3]` (a lesson, in course answers) → `#l=L3`.
 */
export function linkRefs(md: string, source?: string): string {
  // `$$…$$` alone on a line is a display formula (remark-math wants the $$ on their own lines).
  md = md.replace(/^[ \t]*\$\$([^\n]+?)\$\$[ \t]*$/gm, (_, f: string) => `$$\n${f.trim()}\n$$`);
  let out = md.replace(new RegExp(String.raw`\[((?:[FL]\d+(?:\s+${TIME})?\s*[,;]?\s*)+)\]`, "g"), (_, inner: string) =>
    inner
      .split(/[,;]/)
      .map((p) => p.trim())
      .filter(Boolean)
      .map((p) => {
        const [code, time] = p.split(/\s+/);
        if (code.startsWith("L")) return `[${code}](#l=${code})`;
        return time ? `[${code} ${time}](#t=${code}@${toSecs(time)})` : `[${code}](#f=${code})`;
      })
      .join(" "),
  );
  if (source) out = out.replace(new RegExp(String.raw`\[(${TIME})\](?!\()`, "g"), (_, t: string) => `[${t}](#t=${source}@${toSecs(t)})`);
  return out;
}

/** A point the student marked in a recording (bookmark). */
export interface Mark {
  at: number;
  kind: "importante" | "dubbio" | "nota";
  note: string;
}

export const MARK_ICON: Record<Mark["kind"], string> = { importante: "★", dubbio: "?", nota: "✎" };
export const MARK_LABEL: Record<Mark["kind"], string> = { importante: "Importante", dubbio: "Non ho capito", nota: "Nota" };
const MARK_STYLE: Record<Mark["kind"], string> = {
  dubbio: "border-rose-400 bg-rose-50/70 dark:bg-rose-950/30",
  importante: "border-amber-400 bg-amber-50/70 dark:bg-amber-950/30",
  nota: "border-sky-400 bg-sky-50/70 dark:bg-sky-950/30",
};

/** Each mark goes to the paragraph it falls in: the last one starting at or before it. */
function marksByParagraph(text: string, marks: Mark[]): Map<number, Mark[]> {
  const starts = [...text.matchAll(new RegExp(String.raw`^\s*\[(${TIME})\]`, "gm"))].map((m) => toSecs(m[1])).sort((a, b) => a - b);
  const out = new Map<number, Mark[]>();
  for (const m of marks) {
    let t: number | undefined;
    for (const s of starts) if (s <= m.at) t = s;
    if (t === undefined) t = starts[0];
    if (t !== undefined) out.set(t, [...(out.get(t) ?? []), m]);
  }
  return out;
}

/** Seconds of the first `#t=…@N` link of a paragraph (hast node). */
function firstTime(node: unknown): number | null {
  const kids = (node as { children?: { type: string; tagName?: string; properties?: { href?: unknown } }[] } | undefined)?.children ?? [];
  for (const k of kids) {
    if (k.type === "element" && k.tagName === "a") {
      const m = String(k.properties?.href ?? "").match(/^#t=[^@]+@(\d+)$/);
      return m ? Number(m[1]) : null;
    }
    if (k.type === "text" && String((k as { value?: string }).value ?? "").trim()) return null;
  }
  return null;
}

/** Markdown with GFM tables and LaTeX; `#t=…` / `#f=…` links go to `onRef`. */
export function Markdown({
  text,
  source,
  onRef,
  exported,
  imgSrc,
  onLink,
  marks,
  inline,
}: {
  /** A short text inside a line (a flashcard question, a glossary term): no paragraphs. */
  inline?: boolean;
  text: string;
  /** Bookmarks of the recording: their paragraphs are highlighted. */
  marks?: Mark[];
  source?: string;
  onRef?: (href: string) => void;
  exported?: boolean;
  /** Maps a relative image path (`img/x.png`) to a URL (user guide). */
  imgSrc?: (src: string) => string | undefined;
  /** Relative links (`02-lezioni.md`) inside the user guide. */
  onLink?: (href: string) => void;
}) {
  const byParagraph = useMemo(() => (marks?.length ? marksByParagraph(text, marks) : null), [text, marks]);
  const Wrap = inline ? "span" : "div";
  return (
    <Wrap className={inline ? "select-text" : "prose-lezioni select-text"}>
      <ReactMarkdown
        remarkPlugins={[remarkGfm, remarkMath]}
        rehypePlugins={[rehypeKatex]}
        components={{
          p: ({ node, children }) => {
            if (inline) return <>{children}</>;
            const t = byParagraph ? firstTime(node) : null;
            const ms = t === null ? undefined : byParagraph?.get(t);
            if (!ms) return <p>{children}</p>;
            const top = ms.find((m) => m.kind === "dubbio") ?? ms.find((m) => m.kind === "importante") ?? ms[0];
            return (
              <p className={`-ml-3 rounded-r border-l-4 py-1 pl-2 ${MARK_STYLE[top.kind]}`} data-mark={top.kind}>
                <span className="mr-1 select-none font-semibold" title={ms.map((m) => MARK_LABEL[m.kind] + (m.note ? `: ${m.note}` : "")).join("\n")}>
                  {[...new Set(ms.map((m) => MARK_ICON[m.kind]))].join("")}
                </span>
                {children}
              </p>
            );
          },
          img: ({ src, alt }) => {
            const url = typeof src === "string" ? (imgSrc?.(src) ?? src) : undefined;
            return url ? <img src={url} alt={alt ?? ""} className="my-2 rounded-lg border border-zinc-200 shadow-sm dark:border-zinc-800" loading="lazy" /> : null;
          },
          a: ({ href, children }) =>
            onLink && href && !/^(mailto:|#)/.test(href) ? (
              <button className="text-sky-600 underline" onClick={() => onLink(href)}>
                {children}
              </button>
            ) : href?.startsWith("#") && exported ? (
              <span className="ref">{children}</span>
            ) : href?.startsWith("#") ? (
              <button className="mx-0.5 rounded bg-sky-100 px-1 font-mono text-xs text-sky-800 hover:bg-sky-200 dark:bg-sky-950 dark:text-sky-300 dark:hover:bg-sky-900" onClick={() => onRef?.(href)}>
                {children}
              </button>
            ) : (
              <a href={href} target="_blank" rel="noreferrer" className="text-sky-600 underline">
                {children}
              </a>
            ),
        }}
      >
        {linkRefs(text, source)}
      </ReactMarkdown>
    </Wrap>
  );
}
