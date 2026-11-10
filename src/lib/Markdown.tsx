import "katex/dist/katex.min.css";
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

/** Markdown with GFM tables and LaTeX; `#t=…` / `#f=…` links go to `onRef`. */
export function Markdown({
  text,
  source,
  onRef,
  exported,
  imgSrc,
  onLink,
}: {
  text: string;
  source?: string;
  onRef?: (href: string) => void;
  exported?: boolean;
  /** Maps a relative image path (`img/x.png`) to a URL (user guide). */
  imgSrc?: (src: string) => string | undefined;
  /** Relative links (`02-lezioni.md`) inside the user guide. */
  onLink?: (href: string) => void;
}) {
  return (
    <div className="prose-lezioni select-text">
      <ReactMarkdown
        remarkPlugins={[remarkGfm, remarkMath]}
        rehypePlugins={[rehypeKatex]}
        components={{
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
    </div>
  );
}
