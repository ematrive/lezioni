// UI smoke test on the fake backend (scripts/smoke/mock.js): the whole app with fake data,
// from the first run to sharing. Screenshots in the folder given as argument.
//   node scripts/smoke/run.mjs /tmp/shots       (pnpm test:ui)
//   MANUAL=1 node scripts/smoke/run.mjs docs/manuale/img   (pnpm docs:screenshots: the
//   images of the user guide, after the animations and without the toasts)
//   LANG_UI=en …   the English interface, with English fake data (pnpm test:ui:en; the images of
//   docs/manual/img)
import { createServer } from "vite";
import { chromium } from "playwright";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const out = path.resolve(process.argv[2] ?? path.join(root, "scripts/smoke/out"));
const manual = process.env.MANUAL === "1";
const lang = process.env.LANG_UI === "en" ? "en" : "it";
// The interface's English (src/i18n/en/*.json): T("Crea") is the text on screen, from its Italian
// key; an unknown key is an error in both languages, so a typo doesn't pass in Italian only.
const dict = Object.assign({}, ...readdirSync(path.join(root, "src/i18n/en")).filter((f) => f.endsWith(".json")).map((f) => JSON.parse(readFileSync(path.join(root, "src/i18n/en", f), "utf8"))));
const T = (it, vars) => {
  if (!(it in dict)) throw new Error(`T: «${it}» non è nei dizionari`);
  const s = lang === "en" ? dict[it] : it.split("|")[0];
  return vars ? s.replace(/\{(\w+)\}/g, (m, k) => (k in vars ? String(vars[k]) : m)) : s;
};
// Text that isn't a dictionary entry (fake data, regexes): both written out.
const L = (it, en) => (lang === "en" ? en : it);
// The user guide's chapters in the interface language (docs/manuale or docs/manual).
const chapterTitle = (n) => {
  const dir = path.join(root, "docs", lang === "en" ? "manual" : "manuale");
  const f = readdirSync(dir).find((x) => x.startsWith(n + "-") && x.endsWith(".md"));
  return readFileSync(path.join(dir, f), "utf8").match(/^#\s+(.+)$/m)[1].trim();
};
const course = L("Ricerca operativa", "Operations research");
const lessonTitle = L("Simplesso e degenerazione", "Simplex and degeneracy");
const home = L("/home/studente", "/home/student");
const rec = L("registrazione 10-11.m4a", "recording 10-11.m4a");
const slidesPdf = L("slide-cap3.pdf", "slides-ch3.pdf");
const board = L("lavagna.jpg", "whiteboard.jpg");
mkdirSync(out, { recursive: true });
// No file watching: writing the screenshots into docs/manuale/img (imported by the guide) would
// reload the page in the middle of the run.
const server = await createServer({ root, server: { port: 1431, strictPort: true, hmr: false, watch: null }, logLevel: "error" });
await server.listen();
const browser = await chromium.launch();
const errors = [];
let ok = false;
let page = null;
try {
  const p = (page = await browser.newPage({ viewport: { width: 1300, height: 820 } }));
  p.on("pageerror", (e) => errors.push(e.message));
  p.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  // The language before the mock and the app read it (settings_get, localStorage).
  await p.addInitScript((l) => {
    window.__LANG = l;
    try {
      localStorage.setItem("lezioni:lang", l);
    } catch {
      /* the settings say it too */
    }
  }, lang);
  await p.addInitScript({ path: path.join(root, "scripts/smoke/mock.js") });
  const shot = async (name) => {
    if (manual) {
      await p.waitForTimeout(450);
      await p.evaluate(() => {
        const s = document.createElement("style");
        s.id = "manual-shot";
        s.textContent = "[data-toasts]{display:none!important}";
        document.head.appendChild(s);
      });
    }
    await p.screenshot({ path: `${out}/${name}.png` });
    if (manual) await p.evaluate(() => document.getElementById("manual-shot")?.remove());
  };
  await p.goto("http://localhost:1431/");
  await p.getByText(T("Benvenuto in Lezioni")).waitFor();
  await shot("benvenuto");
  await p.getByRole("button", { name: L(/^Usa /, /^Use /) }).click();

  await p.getByRole("button", { name: T("Creane uno") }).click();
  await p.getByPlaceholder(T("Ricerca operativa")).fill(course);
  await p.getByRole("button", { name: T("Crea"), exact: true }).click();
  await p.getByRole("button", { name: T("Lezione"), exact: true }).click();
  await p.getByPlaceholder(L(/simplesso/, /simplex/)).fill(L("Metodo del simplesso", "The simplex method"));
  await p.getByRole("button", { name: T("Crea"), exact: true }).click();
  await p.getByText(T("Trascina qui registrazioni, PDF, foto e appunti")).waitFor();

  // Drag & drop: Tauri events with real paths.
  await p.evaluate(() => window.__emit("tauri://drag-enter", { paths: [], position: { x: 600, y: 300 } }));
  await p.getByText(L(/Rilascia per aggiungere/, /Drop to add to/)).waitFor();
  await shot("trascina");
  await p.evaluate((paths) => window.__emit("tauri://drag-drop", { paths, position: { x: 600, y: 300 } }), [`${home}/${rec}`, `${home}/${slidesPdf}`, `${home}/${board}`]);
  await p.getByText(rec).waitFor();

  // Pasted image (screenshot from the tablet app).
  await p.evaluate(() => {
    const dt = new DataTransfer();
    dt.items.add(new File([new Uint8Array([137, 80, 78, 71])], "image.png", { type: "image/png" }));
    window.dispatchEvent(new ClipboardEvent("paste", { clipboardData: dt, bubbles: true }));
  });
  await p.getByText(L(/^incollato-.*\.png$/, /^pasted-.*\.png$/)).waitFor();

  // Pasted text opens the dialog.
  await p.evaluate((text) => {
    const dt = new DataTransfer();
    dt.setData("text/plain", text);
    window.dispatchEvent(new ClipboardEvent("paste", { clipboardData: dt, bubbles: true }));
  }, L("Domande per il ricevimento:\n- degenerazione", "Questions for office hours:\n- degeneracy"));
  await p.getByText(T("Aggiungi testo")).waitFor();
  await p.getByPlaceholder(T("Testo incollato")).fill(L("Domande", "Office hours"));
  await p.getByRole("button", { name: T("Aggiungi"), exact: true }).click();
  await p.getByTitle(L("Domande.md", "Office hours.md")).waitFor();
  await shot("fonti");

  // Title edit renames.
  const title = p.getByTitle(T("Titolo della lezione"));
  await title.fill(lessonTitle);
  await title.press("Enter");
  await p.locator("li", { hasText: lessonTitle }).first().waitFor();

  // The language spoken in this lecture (⋯ menu): another one, then back to the course's.
  const lessonMenu = () => p.getByRole("button", { name: T("Altre azioni sulla lezione") }).last();
  await lessonMenu().click();
  await p.getByRole("menuitemradio", { name: L(T("Inglese"), T("Italiano")) }).click();
  await p.getByText(T("Lingua cambiata: rielabora anche le fonti per trascrivere di nuovo la registrazione.")).first().waitFor();
  await lessonMenu().click();
  await p.getByRole("menuitemradio", { name: T("Lingua del corso"), checked: false }).waitFor();
  await p.getByRole("menuitemradio", { name: T("Lingua del corso") }).click();

  // Processing with Gemini (fake): progress bar, then the four tabs.
  await p.getByRole("button", { name: T("Elabora"), exact: true }).first().click();
  await p.getByText(L(/Trascrizione sul computer di registrazione/, /Transcribing recording.* on the computer/)).waitFor();
  await p.getByText(L(/Whisper: 43% · ripulitura della parte 1 · 0:0/, /Whisper: 43% · cleaning up part 1 · 0:0/)).waitFor();
  await shot("elaborazione");
  await p.evaluate(() => window.__finishJob());
  await p.getByText(L(/elaborata: .*costo: 0,03 \$/, /processed: .*cost: \$0\.03/)).waitFor();
  await p.getByRole("button", { name: T("Sintesi"), exact: true }).click();
  await p.getByText(L("base ammissibile", "basic feasible solution")).waitFor();
  await shot("sintesi");
  // Earlier version of the summary: compared line by line (restored later, after the edits).
  await p.getByRole("button", { name: T("Versioni precedenti ({n})", { n: 1 }) }).click();
  await p.locator("[data-version]").getByText(L(/Si parte da una \*\*base\*\* e ci/, /We start from a \*\*basis\*\* and/)).waitFor();
  await shot("versioni");
  await p.getByRole("button", { name: T("Chiudi"), exact: true }).click();
  // [F1 00:12:30] jumps to the transcript.
  await p.getByRole("button", { name: "F1 00:12:30" }).click();
  await p.getByText(L("Oggi vediamo il metodo del simplesso.", "Today we look at the simplex method.")).waitFor();
  await p.locator('button[data-t="750"]').first().waitFor();
  // The transcript scrolled to that paragraph (not left at the top).
  await p.waitForTimeout(700);
  const visible = await p.locator('[data-t="750"]').first().evaluate((b) => {
    const r = b.getBoundingClientRect();
    return r.top >= 0 && r.bottom <= window.innerHeight;
  });
  if (!visible) throw new Error("la trascrizione non è andata al minuto 12:30");
  // A break Whisper filled with "grazie": one click and it becomes a grey pause line.
  await p.getByRole("button", { name: T("1 pausa da sistemare") }).click();
  await p.locator("[data-pause]").getByText(T("Pausa")).first().waitFor();
  await p.getByText(L("grazie grazie", "thank you thank you")).waitFor({ state: "detached" });
  await shot("trascrizione");
  // Whisper's text, before the cleanup.
  await p.getByRole("button", { name: "Whisper", exact: true }).click();
  await p.getByText(L(/Allora, ehm, oggi vediamo/, /So, uhm, today we look at/)).waitFor();
  await shot("trascrizione-whisper");
  await p.getByRole("button", { name: T("Ripulita"), exact: true }).click();
  // Bookmarks while listening: the player is at 12:30 (the fake audio element can't play).
  await p.evaluate(() => {
    const a = document.querySelector("audio");
    Object.defineProperty(a, "currentTime", { configurable: true, get: () => 752, set: () => {} });
    a.play = () => Promise.resolve();
  });
  await p.getByRole("button", { name: T("Non ho capito") }).click();
  await p.locator("[data-mark=dubbio]").waitFor();
  await p.getByRole("button", { name: new RegExp(T("Nota|segnaposto") + "$") }).click();
  const note = L("chiedere al ricevimento", "ask at office hours");
  await p.getByPlaceholder(T("Cosa vuoi ricordare di questo punto?")).fill(note);
  await p.getByRole("button", { name: T("Aggiungi"), exact: true }).click();
  await p.getByRole("button", { name: note }).waitFor();
  await p.getByText(new RegExp(`${T("Hai corretto la trascrizione")}|${T("Hai aggiunto dei segnaposti")}`)).first().waitFor();
  await shot("segnaposti");
  // Correction by hand with find and replace; the term goes to the course's ones.
  await p.getByRole("button", { name: T("Modifica"), exact: true }).first().click();
  await p.getByPlaceholder(T("Trova")).fill("team");
  await p.getByPlaceholder(T("Sostituisci con")).fill("TIM");
  await p.getByRole("button", { name: T("Sostituisci tutte ({n})", { n: 1 }) }).click();
  await shot("correzione");
  await p.getByRole("button", { name: T("Aggiungi «{term}» alle parole del corso", { term: "TIM" }) }).click();
  await p.getByText(L(/«TIM» aggiunta alle parole del corso/, /“TIM” added to the course terms/)).waitFor();
  // A paragraph marked as a pause by hand.
  await p.locator("[data-editor] textarea").evaluate((t) => {
    t.focus();
    const i = t.value.indexOf("[00:00:05]");
    t.setSelectionRange(i + 12, i + 20);
    t.dispatchEvent(new MouseEvent("mouseup", { bubbles: true }));
  });
  await p.getByRole("button", { name: T("Segna come pausa") }).click();
  if (!(await p.locator("[data-editor] textarea").inputValue()).includes(L("[00:00:05] *(pausa fino a 00:03:10)*", "[00:00:05] *(pause until 00:03:10)*"))) throw new Error("pausa a mano non segnata");
  // A piece cleaned again from Whisper's text: select the second paragraph.
  await p.getByText(T("Seleziona un pezzo del testo per farlo ripulire di nuovo dal testo di Whisper, anche con un altro modello.")).waitFor();
  await p.locator("[data-editor] textarea").evaluate((t) => {
    t.focus();
    const i = t.value.indexOf("[00:12:30]");
    t.setSelectionRange(i + 12, i + 40);
    t.dispatchEvent(new MouseEvent("mouseup", { bubbles: true }));
  });
  await p.getByRole("button", { name: T("Ripulisci di nuovo la selezione") }).click();
  const constraints = L("con i vincoli", "subject to the constraints");
  await p.locator("[data-proposal]").getByText(constraints).waitFor();
  await shot("ripulitura-pezzo");
  await p.getByRole("button", { name: T("Sostituisci"), exact: true }).click();
  if (!(await p.locator("[data-editor] textarea").inputValue()).includes(constraints)) throw new Error("pezzo non sostituito");
  await p.getByRole("button", { name: T("Salva"), exact: true }).click();
  await p.getByText(L(/libro di TIM/, /book by TIM/)).waitFor();
  await p.getByText(L(/corretta a mano il/, /corrected by hand on/)).waitFor();
  await p.getByText(L(/premi Rigenera per aggiornare sintesi/, /press Regenerate to update the summary/)).waitFor();
  await p.getByRole("button", { name: T("Mappa"), exact: true }).click();
  const ratio = L("Rapporto minimo", "Minimum ratio");
  await p.getByText(ratio).waitFor();
  await shot("mappa");
  // The map alone as a PDF (landscape, drawn as in the app).
  await p.getByRole("button", { name: T("Esporta in PDF") }).click();
  await p.getByText(L(/Salva come PDF/, /Save as PDF/)).first().waitFor();
  if (!(await p.evaluate((ratio) => window.__printed.includes("mt-box") && window.__printed.includes("landscape") && window.__printed.includes(ratio), ratio))) throw new Error("PDF della mappa incompleto");
  if (!manual) writeFileSync(`${out}/mappa.html`, await p.evaluate(() => window.__printed));
  // A concept to the point of the transcript: one with its own reference, one found in the summary.
  if ((await p.locator("[data-map-ref]").count()) < 2) throw new Error("mappa: mancano i rimandi");
  await p.getByRole("button", { name: T("Vai a {label}: {concept}", { label: "F1 00:12:30", concept: "Pivot" }), exact: true }).click();
  await p.locator('[data-t="750"]').first().waitFor();
  // From one document of the lesson to another: the menu on the name in the header.
  await p.getByText(T("In questa lezione:")).waitFor();
  await p.getByTitle(T("Vai a un altro documento della lezione")).first().click();
  await p.getByRole("menuitem", { name: slidesPdf }).click();
  await p.waitForTimeout(600);
  if (!(await p.getByRole("heading", { name: L("Pagina 1", "Page 1") }).evaluate((h) => h.getBoundingClientRect().top < window.innerHeight))) throw new Error("salto al documento non riuscito");
  // The slides next to the transcript: page drawn by pdf.js, "Slide 2" in the text moves it.
  await p.locator("[data-slides-panel] canvas").first().waitFor();
  // Scrolled there first: scrolling moves the slide too, and the click must come last.
  await p.locator("[data-slide-chip='2']").first().scrollIntoViewIfNeeded();
  await p.waitForTimeout(300);
  await p.locator("[data-slide-chip='2']").first().click();
  await p.locator("[data-slides-panel]").getByText(T("Slide {page} di {pages}", { page: 2, pages: 3 })).waitFor();
  await p.waitForFunction(() => {
    const c = document.querySelector("[data-slides-panel] canvas");
    return c && c.width > 50 && c.getContext("2d").getImageData(10, 10, 1, 1).data[3] > 0;
  });
  // Shown on the whole window, where text and slide sit side by side comfortably.
  await p.getByRole("button", { name: T("Tutto schermo") }).click();
  await p.locator("[data-slides-panel]").evaluate((e) => e.closest("section").scrollIntoView({ block: "start" }));
  await p.waitForTimeout(300);
  await shot("slide-accanto");
  // Scrolling the text moves the slide: back to the top → slide 1, then 00:12:30 → slide 2.
  const scrollTo = (y) => p.evaluate((y) => document.querySelector("[data-lesson-content]").scrollTo(0, y), y);
  await scrollTo(0);
  await p.locator("[data-slides-panel]").getByText(T("Slide {page} di {pages}", { page: 1, pages: 3 })).waitFor();
  await p.evaluate(() => {
    const box = document.querySelector("[data-lesson-content]");
    const b = box.querySelector('[data-t="750"]');
    box.scrollBy(0, b.getBoundingClientRect().top - box.getBoundingClientRect().top - box.clientHeight * 0.2);
  });
  await p.locator("[data-slides-panel]").getByText(T("Slide {page} di {pages}", { page: 2, pages: 3 })).waitFor();
  // Photos linked to a minute: a chip in the paragraph, the photo under the slide.
  await p.locator("[data-photos-panel]").getByText(T("Foto {i} di {n}", { i: 1, n: 1 })).waitFor();
  await p.locator("[data-photo-chip]").first().click();
  // A photo linked by hand: the button on the paragraph, then the photo in the menu.
  await p.evaluate(() => document.querySelectorAll("audio").forEach((a) => Object.defineProperty(a, "currentTime", { configurable: true, get: () => 752, set: () => {} })));
  const para = p.locator("p", { has: p.locator('[data-attach="750"]') }).first();
  await para.hover();
  await para.locator('[data-attach="750"]').click();
  await p.locator("[data-photo-picker]").getByText(T("non agganciata")).waitFor();
  await shot("aggancia-foto");
  await p.locator("[data-photo-picker]").getByRole("button", { name: T("In ascolto {time}", { time: "00:12:32" }) }).click();
  await p.locator("[data-photo-picker]").getByRole("menuitem", { name: board }).click();
  await p.getByText(T("{photo} agganciata a {time}.", { photo: board, time: "00:12:32" }), { exact: false }).waitFor();
  await p.keyboard.press("Escape");
  // In the Sources: where each photo is; a title, a description and the minute by hand.
  await p.getByRole("button", { name: T("Fonti"), exact: true }).click();
  await p.locator("[data-photo-card]").getByText(T("⚠ non agganciata")).waitFor();
  await p.locator("[data-photo-card]").getByText("00:12:32").waitFor();
  await shot("fonti-foto");
  const pastedCard = p.locator("[data-photo-card]", { hasText: T("non agganciata") });
  await pastedCard.hover();
  await pastedCard.getByTitle(T("Titolo, descrizione e minuto")).click();
  await p.getByPlaceholder(L(/^incollato-/, /^pasted-/)).fill(L("Tableau del simplesso", "Simplex tableau"));
  await p.getByPlaceholder(L(/Che cosa c'è/, /What it shows/)).fill(L("Le colonne delle variabili di base", "The columns of the basic variables"));
  await p.getByLabel(T("Minuto (hh:mm:ss)")).fill("00:12:30");
  await shot("foto-dialogo");
  await p.getByRole("button", { name: T("Salva"), exact: true }).click();
  await p.getByText(T("Foto salvata.")).waitFor();
  await p.locator("[data-photo-card]", { hasText: L("Tableau del simplesso", "Simplex tableau") }).getByText("00:12:30").waitFor();
  await p.getByRole("button", { name: T("Trascrizione"), exact: true }).click();
  // In the normal layout too, the button to link a photo is inside the text column, not cut off.
  const para2 = p.locator("p", { has: p.locator('[data-attach="750"]') }).first();
  await para2.waitFor({ state: "attached" });
  await p.waitForTimeout(500);
  await para2.scrollIntoViewIfNeeded();
  await para2.hover();
  const [btn, col] = await Promise.all([para2.locator("[data-attach]").boundingBox(), p.locator("[data-lesson-content]").boundingBox()]);
  if (!btn || btn.x < col.x || btn.x + btn.width > col.x + col.width) throw new Error("pulsante per agganciare una foto fuori dalla colonna");
  if (!manual) await shot("aggancia-pulsante");
  // Notes written by hand on the slides: pen blue, on their own line, without "nota a mano:".
  await p.locator(".handwritten").getByText(L(/regola di Bland/, /Bland's rule/)).waitFor();
  if (/nota a mano|handwritten/i.test(await p.locator(".handwritten").first().innerText())) throw new Error("nota a mano: ancora scritto");
  await p.locator(".handwritten").first().evaluate((e) => e.scrollIntoView({ block: "center" }));
  await p.waitForTimeout(400);
  await shot("appunti-a-mano");
  await p.getByRole("button", { name: T("Studio"), exact: true }).click();
  await p.getByText(L("Cos'è il test del rapporto minimo?", "What is the minimum ratio test?")).click();
  await p.getByText(L(/mantenendo l'ammissibilità/, /keeping the solution feasible/)).waitFor();
  if (!(await p.locator("[data-card] .katex").count())) throw new Error("flashcard: formula non resa");
  await shot("studio");
  // Own flashcard, added by hand.
  await p.getByRole("button", { name: T("Modifica flashcard e glossario") }).click();
  const cardsEditor = p.locator("section").filter({ has: p.getByRole("heading", { name: new RegExp("^" + T("Flashcard")) }) });
  await cardsEditor.getByRole("button", { name: T("Aggiungi"), exact: true }).click();
  await cardsEditor.getByLabel(T("Domanda"), { exact: true }).last().fill(L("Cos'è una base?", "What is a basis?"));
  await cardsEditor.getByLabel(T("Risposta"), { exact: true }).last().fill(L("Un insieme di variabili linearmente indipendenti.", "A set of linearly independent variables."));
  await p.getByRole("button", { name: T("Salva"), exact: true }).click();
  await p.getByText(T("Materiale di studio salvato.")).waitFor();
  await p.getByText(T("{i} di {n}", { i: 1, n: 2 })).waitFor();
  await p.getByText(L("Illustra il metodo del simplesso.", "Explain the simplex method.")).waitFor();
  // Spaced review: turn, then "La sapevo" / "Non la sapevo"; a card not known comes back.
  await p.getByRole("button", { name: T("Ripasso"), exact: true }).click();
  await p.getByText(T("{n} carte in questa sessione", { n: 2 })).waitFor();
  await p.getByRole("button", { name: T("Mostra la risposta") }).click();
  await shot("ripasso");
  await p.getByRole("button", { name: T("Non la sapevo") }).click();
  await p.getByText(T("{n} carte in questa sessione", { n: 2 })).waitFor();
  for (let k = 0; k < 3; k++) {
    await p.getByRole("button", { name: T("Mostra la risposta") }).click();
    await p.getByRole("button", { name: T("La sapevo"), exact: true }).click();
    if (await p.locator("[data-review-done]").count()) break;
  }
  await p.getByText(T("Ripasso di oggi finito!")).waitFor();
  await p.getByRole("button", { name: T("Sfoglia"), exact: true }).click();
  // Questions on the lesson, with a clickable reference to the recording.
  await p.getByRole("button", { name: T("Domande"), exact: true }).click();
  await p.getByPlaceholder(T("Chiedi qualcosa su questa lezione…")).fill(L("Cos'è il rapporto minimo?", "What is the minimum ratio?"));
  await p.keyboard.press("Enter");
  await p.getByText(L("sceglie la variabile uscente", "chooses the leaving variable")).waitFor();
  await shot("domande");
  await p.getByRole("button", { name: "F1 00:12:30" }).click();
  await p.locator(`audio[title^='${T("Clicca un minuto nel testo per ascoltare da quel punto").split(" ").slice(0, 3).join(" ")}']`).waitFor();

  // Find in the lesson (⌘F / Ctrl+F): occurrences counted, next one.
  await p.keyboard.press("Control+f");
  await p.getByRole("textbox", { name: T("Cerca nella lezione") }).fill(L("pausa", "break"));
  await p.getByText(L(/^1 di \d+$/, /^1 of \d+$/)).waitFor();
  await p.keyboard.press("Enter");
  await p.getByText(L(/^2 di \d+$/, /^2 of \d+$/)).waitFor();
  await shot("cerca-lezione");
  await p.keyboard.press("Escape");

  // Course page: review of the whole course (all done today), then what the AI cost.
  await p.getByTitle(L(/^Pagina del corso/, /^Course page/)).click();
  await p.getByText(T("Niente da ripassare oggi.")).waitFor();
  // Course summary, from the summaries of the lessons; [L1] opens the lesson.
  await p.getByRole("button", { name: T("Sintesi del corso"), exact: true }).click();
  await p.getByRole("button", { name: T("Genera la sintesi del corso") }).click();
  await p.getByText(L(/Sintesi del corso «Ricerca operativa» pronta/, /Course summary for “Operations research” ready/)).waitFor();
  await p.getByText(T("Domande d'esame del corso")).waitFor();
  await p.getByRole("button", { name: L(/1\. Illustra il metodo del simplesso e il caso degenere/, /1\. Explain the simplex method and the degenerate case/) }).click();
  await shot("sintesi-corso");
  // Glossary of the course: its own words, and the terms of the lessons (one added to the words).
  await p.getByRole("button", { name: T("Glossario"), exact: true }).click();
  await p.getByText(L("Insieme di m variabili linearmente indipendenti.", "A set of m linearly independent variables.")).waitFor();
  await p.getByRole("button", { name: T("+ parole del corso") }).first().click();
  const basis = L("Base", "Basis");
  await p.getByText(T("«{term}» aggiunta alle parole del corso: varrà dalle prossime elaborazioni.", { term: basis })).waitFor();
  // The course list reloads after the save: wait for the field to show it (slower on CI).
  await p.waitForFunction((basis) => document.querySelector("#course-terms")?.value.includes(basis), basis, { timeout: 10000 }).catch(() => {
    throw new Error("parola del corso non aggiunta");
  });
  await shot("corso-glossario");
  await p.getByRole("button", { name: T("Costi"), exact: true }).click();
  await p.getByText(T("spesi con l'AI per questo corso: elaborazioni, domande, ripuliture.").split(":")[0], { exact: false }).waitFor();
  await p.getByRole("cell", { name: L("0,03 $", "$0.03") }).first().waitFor();
  await shot("corso-costi");
  await p.getByRole("button", { name: lessonTitle }).last().click();
  await p.getByText(L("base ammissibile", "basic feasible solution")).waitFor();

  // The lesson on the whole window, Esc to come back.
  const searchAll = T("Cerca in tutte le lezioni");
  await p.getByRole("button", { name: T("Tutto schermo") }).click();
  if (await p.getByPlaceholder(searchAll).count()) throw new Error("tutto schermo: colonne ancora visibili");
  await shot("tutto-schermo");
  await p.keyboard.press("Escape");
  await p.getByPlaceholder(searchAll).waitFor();
  // Course actions in the "⋯" menu; the column of the courses folds away.
  await p.getByRole("button", { name: T("Azioni sul corso {name}", { name: course }) }).click();
  await p.getByRole("button", { name: T("Rinomina e modifica") }).waitFor();
  await shot("corso-menu");
  await p.keyboard.press("Escape");
  // Lesson actions in its own "⋯" menu in the list.
  await p.getByRole("button", { name: T("Azioni sulla lezione {title}", { title: lessonTitle }) }).click();
  await p.getByRole("button", { name: T("Condividi…") }).click();
  await p.getByText(T("Condividi «{title}»", { title: lessonTitle })).waitFor();
  await p.keyboard.press("Escape");
  await p.getByRole("button", { name: T("Nascondi i corsi") }).click();
  if (await p.getByPlaceholder(searchAll).count()) throw new Error("corsi non nascosti");
  await p.getByRole("button", { name: T("Mostra i corsi") }).click();
  await p.getByPlaceholder(searchAll).waitFor();

  // Share as a web page: the HTML is built in the UI, with the summary and embedded KaTeX fonts.
  await p.getByRole("button", { name: T("Condividi") }).click();
  await p.getByText(T("Pagina web")).click();
  await shot("condividi");
  await p.getByRole("button", { name: T("Esporta…") }).click();
  await p.getByText(T("Pagina salvata.")).waitFor();
  const html = await p.evaluate(() => window.__written.text);
  if (!html.includes(L("base ammissibile", "basic feasible solution")) || !html.includes("data:font/woff2;base64") || !html.includes('class="katex"') || !html.includes(ratio)) throw new Error("HTML esportato incompleto");
  if (!manual) writeFileSync(`${out}/lezione.html`, html);
  // PDF: same page, opened for printing with the answers visible.
  await p.getByRole("button", { name: T("Condividi") }).click();
  await p.getByText("PDF", { exact: true }).click();
  await p.evaluate(() => (window.__printed = ""));
  await p.getByRole("button", { name: T("Esporta…") }).click();
  await p.waitForFunction(() => window.__printed);
  if (!(await p.evaluate(() => window.__printed.includes("<details open")))) throw new Error("PDF: flashcard chiuse");

  // Search across the archive, then jump into the transcript.
  await p.getByPlaceholder(searchAll).fill(L("funzione obiettivo", "objective function"));
  await p.getByText(L(/risultat[oi] in tutto l'archivio/, /results? across the archive/)).waitFor();
  await shot("ricerca");
  await p.getByText(`· ${rec} · 00:12:30`).click();
  await p.locator(`audio[title^='${T("Clicca un minuto nel testo per ascoltare da quel punto").split(" ").slice(0, 3).join(" ")}']`).waitFor();
  await p.waitForTimeout(700);
  if (!(await p.locator('[data-t="750"]').first().evaluate((b) => b.getBoundingClientRect().top >= 0 && b.getBoundingClientRect().bottom <= window.innerHeight))) throw new Error("dalla ricerca la trascrizione non va al minuto");

  // A .lezione dropped on the window opens the import dialog.
  await p.evaluate((pkg) => window.__emit("tauri://drag-drop", { paths: [pkg], position: { x: 600, y: 300 } }), L("/home/studente/Scaricati/Break-even.lezione", "/home/student/Downloads/Break-even.lezione"));
  await p.getByText(T("Importa lezioni")).waitFor();
  await shot("importa");
  await p.getByRole("button", { name: T("Importa"), exact: true }).click();
  await p.getByText(T("1 lezione importata.")).waitFor();
  await p.getByText(T("file non incluso")).waitFor();
  if ((await p.evaluate(() => window.__calls.filter((c) => c === "sources_add_files").length)) !== 1) throw new Error("il pacchetto è stato aggiunto come fonte");

  // Version and what's new (the CHANGELOG), bottom left.
  await p.getByRole("button", { name: new RegExp(`^Lezioni \\d+\\.\\d+\\.\\d+ · ${T("Novità")}`) }).click();
  await p.getByRole("heading", { name: "1.1.0", exact: true }).waitFor();
  await p.getByRole("heading", { name: "0.7.0", exact: true }).waitFor();
  await shot("novita");
  await p.keyboard.press("Escape");

  // The user guide (F1), with its screenshots.
  await p.keyboard.press("F1");
  const [ch1, ch2] = [chapterTitle("01"), chapterTitle("02")];
  await p.getByRole("button", { name: `${ch2} →` }).waitFor();
  await p.getByRole("button", { name: `${ch2} →` }).click();
  await p.getByRole("heading", { name: ch2 }).waitFor();
  if ((await p.locator("img[src*='benvenuto']").count()) !== 1) throw new Error("guida: immagine mancante");
  await p.getByRole("button", { name: ch1, exact: true }).click();
  await shot("guida");
  await p.keyboard.press("Escape");

  await p.getByRole("button", { name: T("Impostazioni") }).click();
  await p.getByText("Gemini (Google)").waitFor();
  await p.getByText(T("Modello di trascrizione scaricato (574 MB).")).waitFor();
  await p.getByLabel(T("Parole per tutti i corsi")).fill(L("Politecnico di Milano, ARERA", "MIT, IEEE"));
  await p.getByText("Gemini (Google)").click();
  await p.getByText(T("Parole per tutti i corsi salvate: valgono dalle prossime elaborazioni.")).waitFor();
  await shot("impostazioni");
  // OpenRouter: price list and cost of a lesson under each model.
  await p.getByLabel(T("OpenRouter (molti modelli)")).check();
  await p.getByText(L(/Credito residuo|openrouter.ai\/keys/, /Credit left|openrouter.ai\/keys/)).first().waitFor();
  await p.getByText(L(/0,10 \$ \/ 0,50 \$ per milione di token .* una lezione di 1 h 30 ≈ 0,03 \$/, /\$0\.10 \/ \$0\.50 per million tokens .* a 1 h 30 lecture ≈ \$0\.03/)).waitFor();
  await p.getByRole("button", { name: T("Verifica"), exact: true }).click();
  await p.getByText(L("Credito residuo: 9,62 $", "Credit left: $9.62")).waitFor();
  await shot("impostazioni-openrouter");
  // Interface language: the app mounts again in the other one, then back.
  const langSelect = () => p.locator("select").filter({ has: p.locator('option[value="en"]') }).filter({ has: p.locator('option[value="system"]') });
  await langSelect().selectOption(L("en", "it"));
  await p.getByRole("button", { name: L("Settings", "Impostazioni") }).waitFor();
  await p.getByRole("button", { name: L("Settings", "Impostazioni") }).click();
  await langSelect().selectOption(L("it", "en"));
  await p.getByRole("button", { name: T("Impostazioni") }).waitFor();
  ok = errors.length === 0;
} catch (e) {
  errors.push(String(e));
  // What the page looked like when it failed.
  await page?.screenshot({ path: `${out}/errore.png` }).catch(() => {});
} finally {
  await browser.close();
  await server.close();
}
if (errors.length) console.error("ERRORI:\n" + errors.join("\n"));
console.log(ok ? `OK, screenshot in ${out}` : "FALLITO");
process.exit(ok ? 0 : 1);
