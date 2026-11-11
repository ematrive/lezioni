import { useEffect, useState } from "react";
import { api, fmtDate, fmtUsd, toAppError, type Course, type CourseCosts } from "../../lib/api";
import { ReviewSession } from "../study/Review";

const TABS = [
  { id: "review", label: "Ripasso" },
  { id: "costs", label: "Costi" },
] as const;
type Tab = (typeof TABS)[number]["id"];

/** What belongs to the whole course: shown when no lesson is chosen. */
export function CoursePage({ course, rev, onOpenLesson, onReviewed }: { course: Course; rev: number; onOpenLesson: (id: string) => void; onReviewed?: () => void }) {
  const [tab, setTab] = useState<Tab>("review");
  return (
    <div className="flex h-full flex-col">
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
