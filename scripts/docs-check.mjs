// The user guide is consistent, in both languages (docs/manuale in Italian, docs/manual in
// English): every image it cites exists, every image in img/ is cited, every link to another
// chapter points to a real file, and the two guides have the same chapters.   pnpm docs:check
import { readdirSync, readFileSync, existsSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const docs = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../docs");
const errors = [];
const summary = [];
const chaptersOf = {};
for (const name of ["manuale", "manual"]) {
  const dir = path.join(docs, name);
  if (!existsSync(dir)) {
    errors.push(`${name}: cartella mancante`);
    continue;
  }
  const chapters = readdirSync(dir).filter((f) => f.endsWith(".md"));
  chaptersOf[name] = chapters;
  const imgDir = path.join(dir, "img");
  if (!existsSync(imgDir)) errors.push(`${name}/img: cartella mancante (pnpm docs:screenshots)`);
  const images = new Set(existsSync(imgDir) ? readdirSync(imgDir).filter((f) => f.endsWith(".png")) : []);
  const used = new Set();
  for (const c of chapters) {
    const text = readFileSync(path.join(dir, c), "utf8");
    if (!/^#\s+\S/m.test(text)) errors.push(`${name}/${c}: manca il titolo (# …)`);
    for (const [, src] of text.matchAll(/!\[[^\]]*\]\(([^)]+)\)/g)) {
      const f = src.replace(/^\.?\/?img\//, "");
      used.add(f);
      if (!images.has(f)) errors.push(`${name}/${c}: immagine mancante ${src}`);
    }
    for (const [, href] of text.matchAll(/(?<!!)\[[^\]]*\]\(([^)#]+\.md)(?:#[^)]*)?\)/g)) {
      if (!existsSync(path.join(dir, href))) errors.push(`${name}/${c}: link a un capitolo inesistente ${href}`);
    }
  }
  for (const i of images) if (!used.has(i)) errors.push(`${name}/img/${i}: non usata nel manuale`);
  summary.push(`${name}: ${chapters.length} capitoli, ${images.size} immagini`);
}
const [it, en] = [chaptersOf.manuale ?? [], chaptersOf.manual ?? []];
for (const c of it) if (!en.includes(c)) errors.push(`manual/${c}: manca la versione inglese`);
for (const c of en) if (!it.includes(c)) errors.push(`manuale/${c}: manca la versione italiana`);
if (errors.length) {
  console.error(errors.join("\n"));
  process.exit(1);
}
console.log(`Manuale a posto: ${summary.join("; ")}.`);
