// UI smoke test on the fake backend (scripts/smoke/mock.js): first run, course, lesson,
// drag & drop, pasted image, pasted text. Screenshots in the folder given as argument.
//   node scripts/smoke/run.mjs /tmp/shots
import { createServer } from "vite";
import { chromium } from "playwright";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { mkdirSync, writeFileSync } from "node:fs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const out = process.argv[2] ?? path.join(root, "scripts/smoke/out");
mkdirSync(out, { recursive: true });
const server = await createServer({ root, server: { port: 1431, strictPort: true }, logLevel: "error" });
await server.listen();
const browser = await chromium.launch();
const errors = [];
let ok = false;
try {
  const p = await browser.newPage({ viewport: { width: 1300, height: 820 } });
  p.on("pageerror", (e) => errors.push(e.message));
  p.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  await p.addInitScript({ path: path.join(root, "scripts/smoke/mock.js") });
  await p.goto("http://localhost:1431/");
  await p.getByText("Benvenuto in Lezioni").waitFor();
  await p.screenshot({ path: `${out}/1-benvenuto.png` });
  await p.getByRole("button", { name: /^Usa / }).click();

  await p.getByRole("button", { name: "Creane uno" }).click();
  await p.getByPlaceholder("Ricerca operativa").fill("Ricerca operativa");
  await p.getByRole("button", { name: "Crea", exact: true }).click();
  await p.getByRole("button", { name: "Lezione", exact: true }).click();
  await p.getByPlaceholder(/simplesso/).fill("Metodo del simplesso");
  await p.getByRole("button", { name: "Crea", exact: true }).click();
  await p.getByText("Trascina qui registrazioni").waitFor();

  // Drag & drop: Tauri events with real paths.
  await p.evaluate(() => window.__emit("tauri://drag-enter", { paths: [], position: { x: 600, y: 300 } }));
  await p.getByText(/Rilascia per aggiungere/).waitFor();
  await p.screenshot({ path: `${out}/2-trascina.png` });
  await p.evaluate(() => window.__emit("tauri://drag-drop", { paths: ["/home/studente/registrazione 10-11.m4a", "/home/studente/slide-cap3.pdf", "/home/studente/lavagna.jpg"], position: { x: 600, y: 300 } }));
  await p.getByText("registrazione 10-11.m4a").waitFor();

  // Pasted image (screenshot from the tablet app).
  await p.evaluate(() => {
    const dt = new DataTransfer();
    dt.items.add(new File([new Uint8Array([137, 80, 78, 71])], "image.png", { type: "image/png" }));
    window.dispatchEvent(new ClipboardEvent("paste", { clipboardData: dt, bubbles: true }));
  });
  await p.getByText(/^incollato-.*\.png$/).waitFor();

  // Pasted text opens the dialog.
  await p.evaluate(() => {
    const dt = new DataTransfer();
    dt.setData("text/plain", "Domande per il ricevimento:\n- degenerazione");
    window.dispatchEvent(new ClipboardEvent("paste", { clipboardData: dt, bubbles: true }));
  });
  await p.getByText("Aggiungi testo").waitFor();
  await p.getByPlaceholder("Testo incollato").fill("Domande");
  await p.getByRole("button", { name: "Aggiungi", exact: true }).click();
  await p.getByTitle("Domande.md").waitFor();
  await p.screenshot({ path: `${out}/3-lezione.png` });

  // Title edit renames.
  const title = p.getByTitle("Titolo della lezione");
  await title.fill("Simplesso e degenerazione");
  await title.press("Enter");
  await p.locator("li", { hasText: "Simplesso e degenerazione" }).first().waitFor();

  // Processing with Gemini (fake): progress bar, then the four tabs.
  await p.getByRole("button", { name: "Elabora con Gemini" }).first().click();
  await p.getByText(/Trascrizione di registrazione/).waitFor();
  await p.screenshot({ path: `${out}/4-elaborazione.png` });
  await p.evaluate(() => window.__finishJob());
  await p.getByText(/elaborata: /).waitFor();
  await p.getByRole("button", { name: "Sintesi", exact: true }).click();
  await p.getByText("base ammissibile").waitFor();
  await p.screenshot({ path: `${out}/5-sintesi.png` });
  // [F1 00:12:30] jumps to the transcript.
  await p.getByRole("button", { name: "F1 00:12:30" }).click();
  await p.getByText("Oggi vediamo il metodo del simplesso.").waitFor();
  await p.getByRole("button", { name: "00:12:30" }).waitFor();
  await p.screenshot({ path: `${out}/6-trascrizione.png` });
  await p.getByRole("button", { name: "Mappa", exact: true }).click();
  await p.getByText("Rapporto minimo").waitFor();
  await p.screenshot({ path: `${out}/7-mappa.png` });
  await p.getByRole("button", { name: "Studio", exact: true }).click();
  await p.getByText("Cos'è il test del rapporto minimo?").click();
  await p.getByText(/mantenendo l'ammissibilità/).waitFor();
  await p.screenshot({ path: `${out}/8-studio.png` });
  // Questions on the lesson, with a clickable reference to the recording.
  await p.getByRole("button", { name: "Domande", exact: true }).click();
  await p.getByPlaceholder("Chiedi qualcosa su questa lezione…").fill("Cos'è il rapporto minimo?");
  await p.keyboard.press("Enter");
  await p.getByText("sceglie la variabile uscente").waitFor();
  await p.screenshot({ path: `${out}/9b-domande.png` });
  await p.getByRole("button", { name: "F1 00:12:30" }).click();
  await p.getByText("Clicca un tempo per ascoltare da quel punto.").waitFor();

  // Share as a web page: the HTML is built in the UI, with the summary and embedded KaTeX fonts.
  await p.getByRole("button", { name: "Condividi" }).click();
  await p.getByText("Pagina web").click();
  await p.screenshot({ path: `${out}/10-condividi.png` });
  await p.getByRole("button", { name: "Esporta…" }).click();
  await p.getByText("Pagina salvata.").waitFor();
  const html = await p.evaluate(() => window.__written.text);
  if (!html.includes("base ammissibile") || !html.includes("data:font/woff2;base64") || !html.includes('class="katex"') || !html.includes("Rapporto minimo")) throw new Error("HTML esportato incompleto");
  writeFileSync(`${out}/lezione.html`, html);
  // PDF: same page, opened for printing with the answers visible.
  await p.getByRole("button", { name: "Condividi" }).click();
  await p.getByText("PDF", { exact: true }).click();
  await p.getByRole("button", { name: "Esporta…" }).click();
  await p.getByText(/Salva come PDF/).first().waitFor();
  if (!(await p.evaluate(() => window.__printed.includes("<details open")))) throw new Error("PDF: flashcard chiuse");

  // Search across the archive, then jump into the transcript.
  await p.getByPlaceholder("Cerca in tutte le lezioni").fill("simplesso");
  await p.getByText(/risultati in tutto l'archivio/).waitFor();
  await p.screenshot({ path: `${out}/11-ricerca.png` });
  await p.getByText("· registrazione 10-11.m4a · 00:12:30").click();
  await p.getByText("Clicca un tempo per ascoltare da quel punto.").waitFor();

  // A .lezione dropped on the window opens the import dialog.
  await p.evaluate(() => window.__emit("tauri://drag-drop", { paths: ["/home/studente/Scaricati/Break-even.lezione"], position: { x: 600, y: 300 } }));
  await p.getByText("Importa lezioni").waitFor();
  await p.screenshot({ path: `${out}/12-importa.png` });
  await p.getByRole("button", { name: "Importa", exact: true }).click();
  await p.getByText("1 lezione importata.").waitFor();
  await p.getByText("file non incluso").waitFor();
  if ((await p.evaluate(() => window.__calls.filter((c) => c === "sources_add_files").length)) !== 1) throw new Error("il pacchetto è stato aggiunto come fonte");

  await p.getByRole("button", { name: "Impostazioni" }).click();
  await p.getByText("Gemini (Google)").waitFor();
  await p.waitForTimeout(400);
  await p.screenshot({ path: `${out}/9-impostazioni.png` });
  ok = errors.length === 0;
} catch (e) {
  errors.push(String(e));
} finally {
  await browser.close();
  await server.close();
}
if (errors.length) console.error("ERRORI:\n" + errors.join("\n"));
console.log(ok ? `OK, screenshot in ${out}` : "FALLITO");
process.exit(ok ? 0 : 1);
