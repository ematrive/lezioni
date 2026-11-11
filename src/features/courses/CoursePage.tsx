import { ChevronDown, ChevronRight, Sparkles } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { api, costLine, courseAi, fmtDate, fmtUsd, toAppError, type Course, type CourseCosts, type CourseSummary } from "../../lib/api";
import { Markdown } from "../../lib/Markdown";
import { addCourseTerm, JobBar, startCourseSummary, useJob } from "../lesson/Outputs";
import { ReviewSession } from "../study/Review";
import { toast } from "../../lib/toast";

const TABS = [
  { id: "review", label: "Ripasso" },
  { id: "summary", label: "Sintesi del corso" },
  { id: "glossary", label: "Glossario" },
  { id: "costs", label: "Costi" },
] as const;
type Tab = (typeof TABS)[number]["id"];

/** What belongs to the whole course: shown when no lesson is chosen. */
export function CoursePage({ course, rev, onOpenLesson, onReviewed, onCourseChanged }: { course: Course; rev: number; onOpenLesson: (id: string) => void; onReviewed?: () => void; onCourseChanged?: () => void }) {
  const [tab, setTab] = useState<Tab>("review");
  const job = useJob(course.id);
  return (
    <div className="flex h-full flex-col">
      {job && <JobBar job={job} />}
      <header className="border-b border-zinc-200 px-4 py-3 dark:border-zinc-800">
        <h1 className="text-lg font-semibold">{course.name}</h1>
        <p className="text-xs text-zinc-500">
          {course.lessons} {course.lessons === 1 ? "lezione" : "lezioni"}
          {course.teacher && <> · {course.teacher}</>}
        </p>
      </header>
      <nav className="flex gap-1 border-b border-zinc-200 px-4 dark:border-zinc-800">
        {TABS.map((t) => (
          <button
            key={t.id}
            className={`-mb-px border-b-2 px-3 py-2 ${tab === t.id ? "border-sky-600 font-medium text-sky-700 dark:text-sky-400" : "border-transparent text-zinc-500 hover:text-zinc-800 dark:hover:text-zinc-200"}`}
            onClick={() => setTab(t.id)}
          >
            {t.label}
          </button>
        ))}
      </nav>
      <div className="min-h-0 flex-1 overflow-auto p-4">
        {tab === "review" && (
          <div className="mx-auto max-w-2xl">
            <p className="mb-4 text-sm text-zinc-500">Le flashcard di tutte le lezioni del corso da ripassare oggi. Quelle che sai tornano dopo 1, 3, 7, 14 e 30 giorni; quelle che non sai tornano subito.</p>
            <ReviewSession scope="course" id={course.id} showLesson onOpenLesson={onOpenLesson} onDone={onReviewed} />
          </div>
        )}
        {tab === "summary" && <SummaryTab course={course} rev={rev} busy={!!job} onOpenLesson={onOpenLesson} />}
        {tab === "glossary" && <GlossaryTab course={course} rev={rev} onOpenLesson={onOpenLesson} onCourseChanged={onCourseChanged} />}
        {tab === "costs" && <CostsTab course={course} rev={rev} onOpenLesson={onOpenLesson} />}
      </div>
    </div>
  );
}

const MONTHS = ["gennaio", "febbraio", "marzo", "aprile", "maggio", "giugno", "luglio", "agosto", "settembre", "ottobre", "novembre", "dicembre"];
const monthName = (m: string) => {
  const [y, n] = m.split("-");
  return `${MONTHS[Number(n) - 1] ?? n} ${y}`;
};

function CostsTab({ course, rev, onOpenLesson }: { course: Course; rev: number; onOpenLesson: (id: string) => void }) {
  const [costs, setCosts] = useState<CourseCosts | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    api
      .courseCosts(course.id)
      .then((c) => live && setCosts(c))
      .catch((e) => live && setError(toAppError(e).message));
    return () => {
      live = false;
    };
  }, [course.id, rev]);
  if (error) return <p className="text-red-600">{error}</p>;
  if (!costs) return null;
  return (
    <div className="mx-auto max-w-3xl space-y-6">
      <div>
        <p className="text-3xl font-semibold tabular-nums">{fmtUsd(costs.total)}</p>
        <p className="text-sm text-zinc-500">
          spesi con l'AI per questo corso: elaborazioni, domande, ripuliture{costs.course > 0 && " e sintesi del corso"}.
          {costs.estimated && " Con Gemini il costo non arriva dalla risposta: per quelle voci è una stima al prezzo del piano a pagamento."}
        </p>
      </div>
      {costs.months.length > 0 && (
        <section>
          <h2 className="mb-2 font-semibold">Per mese</h2>
          <table className="w-full text-sm">
            <tbody>
              {costs.months.map((m) => (
                <tr key={m.month} className="border-b border-zinc-100 dark:border-zinc-800">
                  <td className="py-1.5">{monthName(m.month)}</td>
                  <td className="py-1.5 text-right tabular-nums">{fmtUsd(m.total)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </section>
      )}
      <section>
        <h2 className="mb-2 font-semibold">Per lezione</h2>
        <table className="w-full text-sm">
          <tbody>
            {costs.lessons.map((l) => (
              <tr key={l.lessonId} className="border-b border-zinc-100 dark:border-zinc-800">
                <td className="py-1.5">
                  <button className="text-left hover:underline" onClick={() => onOpenLesson(l.lessonId)}>
                    {l.title}
                  </button>
                </td>
                <td className="py-1.5 text-zinc-500">{fmtDate(l.date)}</td>
                <td className="py-1.5 text-right tabular-nums">{l.total > 0 ? fmtUsd(l.total) : "—"}</td>
              </tr>
            ))}
            {costs.course > 0 && (
              <tr className="border-b border-zinc-100 dark:border-zinc-800">
                <td className="py-1.5" colSpan={2}>
                  Sintesi del corso
                </td>
                <td className="py-1.5 text-right tabular-nums">{fmtUsd(costs.course)}</td>
              </tr>
            )}
          </tbody>
        </table>
      </section>
    </div>
  );
}

function SummaryTab({ course, rev, busy, onOpenLesson }: { course: Course; rev: number; busy: boolean; onOpenLesson: (id: string) => void }) {
  const [data, setData] = useState<CourseSummary | null>(null);
  const [open, setOpen] = useState<number | null>(null);
  const load = useCallback(() => {
    courseAi
      .summary(course.id)
      .then(setData)
      .catch((e) => toast(toAppError(e).message, "error"));
  }, [course.id]);
  useEffect(load, [load, rev, busy]);
  if (!data) return null;
  const refs = data.info?.refs ?? {};
  const openRef = (href: string) => {
    const m = href.match(/^#l=(L\d+)$/);
    const id = m && refs[m[1]];
    if (id) onOpenLesson(id);
  };
  const go = () => startCourseSummary(course, load);
  const button = (
    <button className="btn btn-primary" disabled={busy || !data.ready} onClick={go}>
      <Sparkles className="size-4" /> {data.summary ? "Rigenera" : "Genera la sintesi del corso"}
    </button>
  );
  if (!data.summary)
    return (
      <div className="mx-auto max-w-lg pt-12 text-center text-zinc-500">
        <p>
          Una sintesi di tutto il corso, organizzata per argomenti, con le domande d'esame più ricorrenti. È fatta dalle sintesi delle lezioni:{" "}
          {data.ready ? `ne ${data.ready === 1 ? "è pronta 1" : `sono pronte ${data.ready}`}.` : "elabora prima qualche lezione."}
        </p>
        <div className="mt-4">{button}</div>
      </div>
    );
  return (
    <div className="mx-auto max-w-3xl">
      {data.changed > 0 && (
        <p className="mb-4 rounded-md border border-amber-300 bg-amber-50 px-3 py-2 text-sm text-amber-900 dark:border-amber-800 dark:bg-amber-950/40 dark:text-amber-200">
          {data.changed === 1 ? "Una lezione è nuova o ha una sintesi nuova" : `${data.changed} lezioni sono nuove o hanno una sintesi nuova`} rispetto alla sintesi del corso: premi <b>Rigenera</b> per aggiornarla.
        </p>
      )}
      <div className="mb-4 flex items-start gap-3">
        {data.info && (
          <p className="flex-1 text-xs text-zinc-500">
            Fatta il {new Date(data.info.generated).toLocaleString("it-IT")} con {data.info.model} ({costLine(data.info.usage)}) dalle sintesi di {Object.keys(refs).length === 1 ? "1 lezione" : `${Object.keys(refs).length} lezioni`}. I riferimenti [L3] aprono la lezione.
          </p>
        )}
        {button}
      </div>
      <Markdown text={data.summary} onRef={openRef} />
      {data.questions.length > 0 && (
        <section className="mt-10">
          <h3 className="mb-3 font-semibold">Domande d'esame del corso</h3>
          <ul className="space-y-2">
            {data.questions.map((q, i) => (
              <li key={i} className="rounded-lg border border-zinc-200 dark:border-zinc-800">
                <button className="flex w-full items-start gap-2 px-3 py-2 text-left" onClick={() => setOpen(open === i ? null : i)}>
                  {open === i ? <ChevronDown className="mt-0.5 size-4 shrink-0" /> : <ChevronRight className="mt-0.5 size-4 shrink-0" />}
                  <span className="flex-1">
                    {i + 1}. {q.domanda}
                  </span>
                </button>
                {open === i && (
                  <div className="border-t border-zinc-200 px-3 py-2 dark:border-zinc-800">
                    <Markdown text={q.traccia} />
                    {q.lezioni.length > 0 && (
                      <p className="mt-2 text-xs text-zinc-500">
                        Da ripassare:{" "}
                        {q.lezioni.map((c) => (
                          <button key={c} className="mx-0.5 rounded bg-sky-100 px-1 font-mono text-sky-800 hover:bg-sky-200 dark:bg-sky-950 dark:text-sky-300" onClick={() => refs[c] && onOpenLesson(refs[c])}>
                            {c}
                          </button>
                        ))}
                      </p>
                    )}
                  </div>
                )}
              </li>
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}

/** The course's own words for Whisper and the models, and the glossaries of its lessons. */
function GlossaryTab({ course, rev, onOpenLesson, onCourseChanged }: { course: Course; rev: number; onOpenLesson: (id: string) => void; onCourseChanged?: () => void }) {
  const [terms, setTerms] = useState(course.terms);
  const [items, setItems] = useState<Awaited<ReturnType<typeof api.courseGlossary>> | null>(null);
  const [filter, setFilter] = useState("");
  useEffect(() => setTerms(course.terms), [course.terms]);
  useEffect(() => {
    let live = true;
    api
      .courseGlossary(course.id)
      .then((g) => live && setItems(g))
      .catch((e) => toast(toAppError(e).message, "error"));
    return () => {
      live = false;
    };
  }, [course.id, rev]);
  const have = new Set(course.terms.split(/[,;\n]/).map((t) => t.trim().toLowerCase()));
  const f = filter.trim().toLowerCase();
  const shown = (items ?? []).filter((t) => !f || t.termine.toLowerCase().includes(f) || t.definizione.toLowerCase().includes(f));
  return (
    <div className="mx-auto max-w-3xl space-y-8">
      <section>
        <label className="mb-1 block font-semibold" htmlFor="course-terms">
          Parole del corso
        </label>
        <textarea
          id="course-terms"
          className="field min-h-20 text-sm"
          placeholder="Termini tecnici e nomi che tornano spesso, scritti giusti e separati da virgole: per esempio ARERA, colpo d'ariete, Gauckler-Strickler"
          value={terms}
          onChange={(e) => setTerms(e.target.value)}
          onBlur={() => {
            if (terms.trim() === course.terms.trim()) return;
            api.updateCourse(course.id, course.name, course.teacher, terms).then(
              () => {
                toast("Parole del corso salvate: valgono dalle prossime elaborazioni.");
                onCourseChanged?.();
              },
              (e) => toast(toAppError(e).message, "error"),
            );
          }}
        />
        <p className="mt-1 text-xs text-zinc-500">Le suggeriamo a Whisper, alla ripulitura e alla sintesi delle lezioni di questo corso. Quelle che valgono per tutti i corsi sono nelle Impostazioni.</p>
      </section>
      <section>
        <div className="mb-2 flex items-center gap-3">
          <h3 className="flex-1 font-semibold">
            Termini delle lezioni {items && <span className="font-normal text-zinc-400">{items.length}</span>}
          </h3>
          {(items?.length ?? 0) > 8 && <input className="field max-w-56" placeholder="Filtra i termini" value={filter} onChange={(e) => setFilter(e.target.value)} />}
        </div>
        {items && !items.length && <p className="text-sm text-zinc-500">I glossari delle lezioni compaiono qui quando le lezioni sono elaborate.</p>}
        <dl className="divide-y divide-zinc-200 rounded-lg border border-zinc-200 dark:divide-zinc-800 dark:border-zinc-800">
          {shown.map((t) => (
            <div key={t.termine} className="group grid grid-cols-[minmax(8rem,14rem)_1fr] gap-3 px-3 py-2 text-sm">
              <dt className="font-medium">
                <Markdown text={t.termine} inline />
                {!have.has(t.termine.toLowerCase()) && (
                  <button
                    className="ml-2 text-xs font-normal text-sky-700 opacity-0 group-hover:opacity-100 hover:underline dark:text-sky-400"
                    onClick={() => addCourseTerm(course.id, t.termine).then(() => onCourseChanged?.(), (e) => toast(toAppError(e).message, "error"))}
                    title="Aggiungi alle parole del corso"
                  >
                    + parole del corso
                  </button>
                )}
              </dt>
              <dd className="text-zinc-700 dark:text-zinc-300">
                <Markdown text={t.definizione} inline />
                <span className="ml-2 text-xs text-zinc-400">
                  {t.lessons.map(([id, title], i) => (
                    <button key={id} className="hover:underline" onClick={() => onOpenLesson(id)}>
                      {i > 0 && ", "}
                      {title}
                    </button>
                  ))}
                </span>
              </dd>
            </div>
          ))}
        </dl>
      </section>
    </div>
  );
}
