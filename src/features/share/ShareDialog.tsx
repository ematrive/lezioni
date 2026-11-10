import { open, save } from "@tauri-apps/plugin-dialog";
import { FileDown, FileText, Globe, Layers, Package } from "lucide-react";
import { useState } from "react";
import { ai, api, share, toAppError, type Course, type Lesson } from "../../lib/api";
import { Modal } from "../../lib/Modal";
import { toast } from "../../lib/toast";
import { buildHtml, type ExportParts } from "./html";

type Format = "package" | "pdf" | "html" | "markdown" | "anki";

const FORMATS: { id: Format; label: string; hint: string; icon: React.ReactNode }[] = [
  { id: "package", label: "Pacchetto .lezione", hint: "Per chi usa Lezioni: si apre con doppio clic, con fonti, trascrizioni e sintesi già pronte.", icon: <Package className="size-5" /> },
  { id: "pdf", label: "PDF", hint: "Dispensa per chiunque. Si apre il browser con la stampa: scegli «Salva come PDF».", icon: <FileDown className="size-5" /> },
  { id: "html", label: "Pagina web", hint: "Un file .html che si apre in qualsiasi browser, anche dal telefono, con flashcard cliccabili.", icon: <Globe className="size-5" /> },
  { id: "markdown", label: "Markdown (Obsidian, NotebookLM)", hint: "Una cartella per lezione con file .md: da mettere in un vault di Obsidian o caricare su NotebookLM.", icon: <FileText className="size-5" /> },
  { id: "anki", label: "Flashcard per Anki", hint: "File di testo da importare in Anki (File → Importa): flashcard e domande d'esame.", icon: <Layers className="size-5" /> },
];

const slug = (s: string) => s.normalize("NFD").replace(/[̀-ͯ]/g, "").replace(/[^A-Za-z0-9 _-]+/g, "").trim().replace(/\s+/g, " ") || "lezione";

/** Export one lesson, or a whole course (`lesson` null). */
export function ShareDialog({ course, lesson, onClose }: { course: Course; lesson: Lesson | null; onClose: () => void }) {
  const [format, setFormat] = useState<Format>("package");
  const [media, setMedia] = useState(false);
  const [files, setFiles] = useState(true);
  const [notes, setNotes] = useState(false);
  const [parts, setParts] = useState<ExportParts>({ summary: true, map: true, study: true, transcript: false });
  const [busy, setBusy] = useState(false);
  const name = lesson ? `${lesson.date} ${lesson.title}` : course.name;

  const lessons = async () => (lesson ? [lesson] : (await api.lessons(course.id)).sort((a, b) => a.date.localeCompare(b.date)));

  const run = async () => {
    setBusy(true);
    try {
      const ls = await lessons();
      if (!ls.length) throw { message: "Il corso non ha lezioni" };
      const ids = ls.map((l) => l.id);
      if (format === "package") {
        const dest = await save({ title: "Salva il pacchetto", defaultPath: `${slug(name)}.lezione`, filters: [{ name: "Lezione", extensions: ["lezione"] }] });
        if (!dest) return;
        await share.exportPackage(ids, { media, files, notes }, dest);
        toast(`Pacchetto salvato: ${ls.length === 1 ? "1 lezione" : `${ls.length} lezioni`}. Mandalo come un normale file (WhatsApp, mail, Drive).`);
      } else if (format === "pdf" || format === "html") {
        const docs = await Promise.all(ls.map(async (l) => ({ lesson: l, outputs: await ai.outputs(l.id) })));
        if (!docs.some((d) => d.outputs.summary || d.outputs.study || Object.keys(d.outputs.extracts).length)) throw { message: "Niente da esportare: elabora prima la lezione con Gemini" };
        if (format === "pdf") {
          await share.print(slug(name), buildHtml(course, docs, parts, true));
          toast("Si è aperto il browser: nella finestra di stampa scegli «Salva come PDF».");
        } else {
          const dest = await save({ title: "Salva la pagina", defaultPath: `${slug(name)}.html`, filters: [{ name: "Pagina web", extensions: ["html"] }] });
          if (!dest) return;
          await share.writeText(dest, buildHtml(course, docs, parts, false));
          toast("Pagina salvata.");
        }
      } else if (format === "markdown") {
        const dest = await open({ directory: true, title: "Cartella in cui creare i file Markdown (ad esempio il vault di Obsidian)" });
        if (typeof dest !== "string") return;
        const n = await share.markdown(ids, dest);
        toast(n === 1 ? "Creata 1 cartella di file Markdown." : `Create ${n} cartelle di file Markdown.`);
        api.reveal(dest);
      } else {
        const dest = await save({ title: "Salva le flashcard per Anki", defaultPath: `${slug(name)} - Anki.txt`, filters: [{ name: "Testo", extensions: ["txt"] }] });
        if (!dest) return;
        const n = await share.anki(ids, dest);
        toast(`${n} carte salvate. In Anki: File → Importa e scegli il file.`);
      }
      onClose();
    } catch (e) {
      toast(toAppError(e).message, "error");
    } finally {
      setBusy(false);
    }
  };

  const check = (label: string, on: boolean, set: (v: boolean) => void, hint?: string) => (
    <label className="flex items-start gap-2">
      <input type="checkbox" className="mt-1" checked={on} onChange={(e) => set(e.target.checked)} />
      <span>
        {label}
        {hint && <span className="block text-xs text-zinc-500">{hint}</span>}
      </span>
    </label>
  );

  return (
    <Modal
      wide
      title={lesson ? `Condividi «${lesson.title}»` : `Esporta il corso «${course.name}»`}
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>
            Annulla
          </button>
          <button className="btn btn-primary" disabled={busy} onClick={run}>
            {busy ? "Esportazione…" : "Esporta…"}
          </button>
        </>
      }
    >
      <div className="space-y-4">
        <div className="grid gap-2">
          {FORMATS.map((f) => (
            <label key={f.id} className={`flex cursor-pointer items-start gap-3 rounded-lg border p-3 ${format === f.id ? "border-sky-500 bg-sky-50 dark:bg-sky-950/40" : "border-zinc-200 hover:bg-zinc-50 dark:border-zinc-800 dark:hover:bg-zinc-900"}`}>
              <input type="radio" name="format" className="sr-only" checked={format === f.id} onChange={() => setFormat(f.id)} />
              <span className="mt-0.5 text-sky-600">{f.icon}</span>
              <span>
                <span className="font-medium">{f.label}</span>
                <span className="block text-xs text-zinc-500">{f.hint}</span>
              </span>
            </label>
          ))}
        </div>
        {format === "package" && (
          <div className="space-y-2 rounded-lg bg-zinc-100 p-3 dark:bg-zinc-900">
            {check("Includi foto, PDF e appunti", files, setFiles)}
            {check("Includi le registrazioni", media, setMedia, "Pesano molto e servono il permesso del docente. Senza, la trascrizione viaggia comunque.")}
            {check("Includi le mie note sulla lezione", notes, setNotes, "Le conversazioni nella scheda Domande restano sempre solo sul tuo computer.")}
          </div>
        )}
        {(format === "pdf" || format === "html") && (
          <div className="grid grid-cols-2 gap-2 rounded-lg bg-zinc-100 p-3 dark:bg-zinc-900">
            {check("Sintesi", parts.summary, (v) => setParts({ ...parts, summary: v }))}
            {check("Mappa concettuale", parts.map, (v) => setParts({ ...parts, map: v }))}
            {check("Flashcard, domande e glossario", parts.study, (v) => setParts({ ...parts, study: v }))}
            {check("Trascrizioni (in appendice)", parts.transcript, (v) => setParts({ ...parts, transcript: v }))}
          </div>
        )}
      </div>
    </Modal>
  );
}
