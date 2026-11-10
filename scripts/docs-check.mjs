// The user guide is consistent: every image it cites exists, every image in docs/manuale/img
// is cited, every link to another chapter points to a real file.   pnpm docs:check
import { readdirSync, readFileSync, existsSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const dir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../docs/manuale");
const chapters = readdirSync(dir).filter((f) => f.endsWith(".md"));
const images = new Set(readdirSync(path.join(dir, "img")).filter((f) => f.endsWith(".png")));
const used = new Set();
const errors = [];
for (const c of chapters) {
  const text = readFileSync(path.join(dir, c), "utf8");
  if (!/^#\s+\S/m.test(text)) errors.push(`${c}: manca il titolo (# …)`);
  for (const [, src] of text.matchAll(/!\[[^\]]*\]\(([^)]+)\)/g)) {
    const f = src.replace(/^\.?\/?img\//, "");
    used.add(f);
    if (!images.has(f)) errors.push(`${c}: immagine mancante ${src}`);
  }
  for (const [, href] of text.matchAll(/(?<!!)\[[^\]]*\]\(([^)#]+\.md)(?:#[^)]*)?\)/g)) {
    if (!existsSync(path.join(dir, href))) errors.push(`${c}: link a un capitolo inesistente ${href}`);
  }
}
for (const i of images) if (!used.has(i)) errors.push(`img/${i}: non usata nel manuale`);
if (errors.length) {
  console.error(errors.join("\n"));
  process.exit(1);
}
console.log(`Manuale a posto: ${chapters.length} capitoli, ${images.size} immagini.`);
