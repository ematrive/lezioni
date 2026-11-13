// Checks the English dictionary (src/i18n/en/*.json) against the calls t("…"), tn(n, "…", "…"),
// tFor(l, "…"), tnFor(l, n, "…", "…") in src/: missing entries, arguments that are not plain
// strings, {placeholders} that differ, entries no call uses, keys defined twice.
// Exit code 1 when something is missing or wrong (unused entries are only listed).
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const root = new URL("..", import.meta.url).pathname;
const src = join(root, "src");

const files = (dir) => readdirSync(dir).flatMap((f) => (statSync(join(dir, f)).isDirectory() ? files(join(dir, f)) : /\.(tsx?|mjs)$/.test(f) ? [join(dir, f)] : []));

// The dictionary.
const dictDir = join(src, "i18n", "en");
const dict = new Map();
const dupes = [];
for (const f of readdirSync(dictDir).filter((f) => f.endsWith(".json"))) {
  for (const [k, v] of Object.entries(JSON.parse(readFileSync(join(dictDir, f), "utf8")))) {
    if (dict.has(k) && dict.get(k).value !== v) dupes.push(`${k}  (${dict.get(k).file} e ${f})`);
    dict.set(k, { value: v, file: f });
  }
}

/** The arguments of the call starting at `i` (just after "("), as source text. */
function args(text, i) {
  const out = [];
  let depth = 0;
  let cur = "";
  for (; i < text.length; i++) {
    const c = text[i];
    if (c === '"' || c === "'" || c === "`") {
      let j = i + 1;
      while (j < text.length && text[j] !== c) j += text[j] === "\\" ? 2 : 1;
      cur += text.slice(i, j + 1);
      i = j;
      continue;
    }
    if ("([{".includes(c)) depth++;
    if (")]}".includes(c)) {
      if (depth === 0) {
        out.push(cur.trim());
        return out;
      }
      depth--;
    }
    if (c === "," && depth === 0) {
      out.push(cur.trim());
      cur = "";
      continue;
    }
    cur += c;
  }
  return out;
}

/** A plain string literal's value, or null. */
function literal(a) {
  if (!a) return null;
  const q = a[0];
  if ((q === '"' || q === "'") && a.endsWith(q)) return JSON.parse(q === '"' ? a : `"${a.slice(1, -1).replace(/\\'/g, "'").replace(/"/g, '\\"')}"`);
  if (q === "`" && a.endsWith("`") && !a.includes("${")) return a.slice(1, -1);
  return null;
}

const vars = (s) => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort().join(",");
const used = new Set();
const problems = [];
// Which arguments are texts: index list per function.
const TEXTS = { t: [0], tn: [1, 2], tFor: [1], tnFor: [2, 3] };
for (const f of files(src)) {
  if (f.includes(`${join("i18n", "")}`)) continue;
  const text = readFileSync(f, "utf8");
  for (const m of text.matchAll(/(?<![\w.$])(t|tn|tFor|tnFor)\(/g)) {
    const a = args(text, m.index + m[0].length);
    const line = text.slice(0, m.index).split("\n").length;
    const where = `${relative(root, f)}:${line}`;
    for (const k of TEXTS[m[1]]) {
      const v = literal(a[k]);
      if (v === null) {
        problems.push(`${where}  argomento non letterale: ${a[k]}`);
        continue;
      }
      used.add(v);
      const e = dict.get(v);
      if (!e) problems.push(`${where}  manca: ${JSON.stringify(v)}`);
      else if (vars(e.value) !== vars(v.split("|")[0])) problems.push(`${where}  segnaposti diversi: ${JSON.stringify(v)} → ${JSON.stringify(e.value)}`);
    }
  }
}

const unused = [...dict.keys()].filter((k) => !used.has(k));
for (const d of dupes) problems.push(`definita due volte con valori diversi: ${d}`);
if (unused.length) console.log(`Voci non usate (${unused.length}):\n` + unused.map((k) => `  ${JSON.stringify(k)} (${dict.get(k).file})`).join("\n"));
if (problems.length) {
  console.log(`\nProblemi (${problems.length}):\n` + problems.map((p) => "  " + p).join("\n"));
  process.exit(1);
}
console.log(`Traduzioni a posto: ${used.size} testi.`);
