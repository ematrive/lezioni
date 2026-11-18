/**
 * Images placed by hand in a summary. They are written `![Titolo](img:<source id>)` on a line of
 * their own: the id of a source does not change, unlike the `F3` codes of a generation (which only
 * cover the sources of that generation). Everything here works on lines of the Markdown text.
 */

/** `img:<source id>`: an image of the lesson placed by hand. */
export const imageRef = (sourceId: string) => `img:${sourceId}`;

/** The source id of an `img:<id>` reference, or null. */
export function imageRefId(src: string): string | null {
  const m = src.match(/^img:([\w-]+)$/);
  return m ? m[1] : null;
}

/** `![caption](src)`; brackets and line breaks would end the caption. */
export function imageLine(caption: string, src: string): string {
  return `![${caption.replace(/[[\]\n\r]+/g, " ").replace(/\s{2,}/g, " ").trim()}](${src})`;
}

/** Ids of the sources placed with `img:<id>`. */
export function placedImages(md: string): Set<string> {
  return new Set([...md.matchAll(/!\[[^\]\n]*\]\(img:([\w-]+)\)/g)].map((m) => m[1]));
}

/**
 * For each line of the text the renderer gets (`linkRefs`), its line in `md` (1-based):
 * `$$…$$` alone on a line becomes three lines there.
 */
export function displayLines(md: string): number[] {
  const out: number[] = [];
  md.split("\n").forEach((l, i) => {
    const n = /^[ \t]*\$\$([^\n]+?)\$\$[ \t]*$/.test(l) ? 3 : 1;
    for (let k = 0; k < n; k++) out.push(i + 1);
  });
  return out;
}

const blank = (l: string | undefined) => l !== undefined && !l.trim();

/** `snippet` as a paragraph of its own after line `line` (1-based; 0 = at the top). */
export function insertAfterLine(md: string, line: number, snippet: string): string {
  const lines = md.split("\n");
  const before = lines.slice(0, Math.max(0, Math.min(line, lines.length)));
  const after = lines.slice(before.length);
  while (blank(before[before.length - 1])) before.pop();
  while (after.length > 1 && blank(after[0])) after.shift();
  const end = after.length === 0 || (after.length === 1 && blank(after[0])) ? [""] : ["", ...after];
  return [...before, ...(before.length ? [""] : []), snippet, ...end].join("\n");
}

/** `snippet` as a paragraph after the line holding offset `pos` (the cursor of the editor). */
export function insertAtOffset(md: string, pos: number, snippet: string): string {
  return insertAfterLine(md, md.slice(0, pos).split("\n").length, snippet);
}

function imageRe(src: string): RegExp {
  return new RegExp(String.raw`!\[[^\]\n]*\]\(` + src.replace(/[.*+?^${}()|[\]\\]/g, "\\$&") + String.raw`\)`);
}

/** Index (0-based) of the line with the image `src`: `line` (1-based) when it has it there (the
 *  same image can be in the summary twice), otherwise the first one; -1 when there is none. */
function imageIndex(lines: string[], src: string, line?: number): number {
  const re = imageRe(src);
  if (line !== undefined && re.test(lines[line - 1] ?? "")) return line - 1;
  return lines.findIndex((l) => re.test(l));
}

/** The text without the image `src` on line `line` (the whole line, when the image was alone on it). */
export function removeImage(md: string, src: string, line?: number): string {
  const lines = md.split("\n");
  const i = imageIndex(lines, src, line);
  if (i < 0) return md;
  const rest = lines[i].replace(imageRe(src), "");
  if (rest.trim()) {
    lines[i] = rest.replace(/ {2,}/g, " ").trimEnd();
    return lines.join("\n");
  }
  lines.splice(i, 1);
  // Two blank lines left together: one is enough.
  if (blank(lines[i]) && (i === 0 || blank(lines[i - 1]))) lines.splice(i, 1);
  return lines.join("\n");
}

/**
 * The image `src` moved one block up or down. `blocks` are the top-level blocks of `md` as
 * [first line, last line] (1-based). An image alone in its paragraph trades places with the block
 * next to it; one inside a paragraph becomes a paragraph of its own before or after that paragraph.
 */
export function moveImage(md: string, src: string, dir: -1 | 1, blocks: [number, number][], line?: number): string {
  const lines = md.split("\n");
  const i = imageIndex(lines, src, line);
  if (i < 0) return md;
  const b = blocks.findIndex(([s, e]) => s <= i + 1 && i + 1 <= e);
  if (b < 0) return md;
  const [s, e] = blocks[b];
  const alone = s === e && lines[i].trim() === lines[i].match(imageRe(src))?.[0];
  if (!alone) {
    const img = lines[i].match(imageRe(src))![0];
    const without = removeImage(md, src, i + 1);
    // Its whole line went away (an image glued under the text): the block ends one line earlier.
    const shift = lines[i].replace(imageRe(src), "").trim() ? 0 : 1;
    return dir < 0 ? insertAfterLine(without, s - 1, img) : insertAfterLine(without, e - shift, img);
  }
  const n = blocks[b + dir];
  if (!n) return md;
  const [first, second] = dir < 0 ? [n, blocks[b]] : [blocks[b], n];
  const gap = lines.slice(first[1], second[0] - 1);
  return [
    ...lines.slice(0, first[0] - 1),
    ...lines.slice(second[0] - 1, second[1]),
    ...(gap.length ? gap : [""]),
    ...lines.slice(first[0] - 1, first[1]),
    ...lines.slice(second[1]),
  ].join("\n");
}
