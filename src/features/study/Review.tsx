import { Check, RotateCcw, X } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { review, toAppError, type DueCard } from "../../lib/api";
import { toast } from "../../lib/toast";

/** Shuffled, but cards seen before stay ahead of the new ones. */
function order(cards: DueCard[]): DueCard[] {
  const mix = (cs: DueCard[]) => cs.map((c) => [Math.random(), c] as const).sort((a, b) => a[0] - b[0]).map(([, c]) => c);
  return [...mix(cards.filter((c) => c.box !== null)), ...mix(cards.filter((c) => c.box === null))];
}

/** A review session: the cards due today, "La sapevo" / "Non la sapevo" after turning each one.
 *  Cards not known come back at the end of the session. */
export function ReviewSession({ scope, id, showLesson, onOpenLesson, onDone }: { scope: "lesson" | "course"; id: string; showLesson?: boolean; onOpenLesson?: (lessonId: string) => void; onDone?: () => void }) {
  const [queue, setQueue] = useState<DueCard[] | null>(null);
  const [counts, setCounts] = useState({ due: 0, new: 0 });
  const [flipped, setFlipped] = useState(false);
  const [done, setDone] = useState({ known: 0, again: 0 });
  const [busy, setBusy] = useState(false);
  const load = useCallback(() => {
    review
      .due(scope, id)
      .then((d) => {
        setQueue(order(d.cards));
        setCounts({ due: d.due, new: d.new });
        setDone({ known: 0, again: 0 });
        setFlipped(false);
      })
      .catch((e) => toast(toAppError(e).message, "error"));
  }, [scope, id]);
  useEffect(load, [load]);

  if (!queue) return null;
  const card = queue[0];
  const answer = async (knew: boolean) => {
    if (!card || busy) return;
    setBusy(true);
    try {
      await review.answer(card.lessonId, card.key, knew);
      setDone((d) => (knew ? { ...d, known: d.known + 1 } : { ...d, again: d.again + 1 }));
      setQueue(knew ? queue.slice(1) : [...queue.slice(1), { ...card, box: 0 }]);
      setFlipped(false);
      if (knew && queue.length === 1) onDone?.();
    } catch (e) {
      toast(toAppError(e).message, "error");
    } finally {
      setBusy(false);
    }
  };

  if (!card) {
    const answered = done.known + done.again;
    return (
      <div className="rounded-xl border-2 border-emerald-300 bg-emerald-50 p-6 text-center dark:border-emerald-800 dark:bg-emerald-950/40" data-review-done>
        <p className="text-lg font-medium">{answered ? "Ripasso di oggi finito!" : "Niente da ripassare oggi."}</p>
        {answered > 0 && (
          <p className="mt-1 text-sm text-zinc-600 dark:text-zinc-300">
            {done.known} {done.known === 1 ? "carta sapevi" : "carte sapevi"}
            {done.again > 0 && `, ${done.again} ${done.again === 1 ? "volta hai dovuto rivederne una" : "volte hai dovuto rivederne qualcuna"}`}. Le carte tornano fra 1, 3, 7, 14 e 30 giorni se continui a saperle.
          </p>
        )}
        {counts.new > 0 && answered > 0 && (
          <button className="btn mt-3" onClick={load}>
            <RotateCcw className="size-4" /> Altre carte nuove
          </button>
        )}
      </div>
    );
  }
  return (
    <div>
      <p className="mb-2 text-sm text-zinc-500">
        {queue.length} {queue.length === 1 ? "carta" : "carte"} in questa sessione
        {counts.due > 0 && ` · ${counts.due} da ripassare`}
        {counts.new > 0 && ` · ${counts.new} mai viste`}
        {card.box === null && <span className="ml-2 rounded bg-sky-100 px-1.5 text-xs text-sky-800 dark:bg-sky-950 dark:text-sky-300">nuova</span>}
      </p>
      {showLesson && (
        <p className="mb-1 text-xs text-zinc-500">
          {onOpenLesson ? (
            <button className="hover:underline" onClick={() => onOpenLesson(card.lessonId)}>
              {card.lessonTitle}
            </button>
          ) : (
            card.lessonTitle
          )}
        </p>
      )}
      <button
        className={`flex min-h-44 w-full items-center justify-center rounded-xl border-2 p-6 text-center text-lg transition-colors ${flipped ? "border-emerald-400 bg-emerald-50 dark:border-emerald-800 dark:bg-emerald-950/40" : "border-sky-300 bg-white dark:border-sky-800 dark:bg-zinc-900"}`}
        onClick={() => setFlipped(!flipped)}
        title="Clicca per girare"
      >
        <span className="select-text">{flipped ? card.risposta : card.domanda}</span>
      </button>
      <div className="mt-3 flex justify-center gap-2">
        {!flipped ? (
          <button className="btn btn-primary" onClick={() => setFlipped(true)}>
            <RotateCcw className="size-4" /> Mostra la risposta
          </button>
        ) : (
          <>
            <button className="btn" disabled={busy} onClick={() => answer(false)}>
              <X className="size-4 text-rose-600" /> Non la sapevo
            </button>
            <button className="btn btn-primary" disabled={busy} onClick={() => answer(true)}>
              <Check className="size-4" /> La sapevo
            </button>
          </>
        )}
      </div>
    </div>
  );
}
