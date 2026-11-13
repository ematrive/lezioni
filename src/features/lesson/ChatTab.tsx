import { MessageCircleQuestion, SendHorizontal, Trash2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { chat, toAppError, type ChatMessage, type ChatScope, type Lesson } from "../../lib/api";
import { Markdown } from "../../lib/Markdown";
import { toast } from "../../lib/toast";
import { t } from "../../i18n";

// A function: the examples follow the interface language.
const examples = (scope: ChatScope): string[] =>
  scope === "lesson"
    ? [t("Spiegami il passaggio più difficile della lezione"), t("Fammi un esempio numerico"), t("Cosa ha detto il docente sull'esame?")]
    : [t("Quali argomenti collegano le lezioni?"), t("Fammi uno schema per ripassare tutto il corso"), t("Dove si parla di ottimizzazione?")];

/**
 * Questions answered only from the archive. `onRef` gets the code's target: a source id and
 * seconds (lesson answers), or a lesson id (course answers).
 */
export function ChatTab({ lesson, onSource, onLesson }: { lesson: Lesson; onSource: (sourceId: string, secs: number) => void; onLesson: (lessonId: string) => void }) {
  const [msgs, setMsgs] = useState<ChatMessage[]>([]);
  const [scope, setScope] = useState<ChatScope>("lesson");
  const [q, setQ] = useState("");
  const [busy, setBusy] = useState(false);
  const end = useRef<HTMLDivElement>(null);
  useEffect(() => {
    chat.load(lesson.id).then(setMsgs, () => setMsgs([]));
  }, [lesson.id]);
  useEffect(() => {
    // Braces: in recent browsers scrollIntoView returns a Promise, which React takes for a cleanup.
    end.current?.scrollIntoView({ behavior: "smooth" });
  }, [msgs.length, busy]);

  const ask = async (text: string) => {
    if (!text.trim() || busy) return;
    setBusy(true);
    setMsgs((m) => [...m, { role: "user", text, scope, at: new Date().toISOString(), refs: {} }]);
    setQ("");
    try {
      const r = await chat.ask(lesson.id, scope, text);
      setMsgs(r.messages);
    } catch (e) {
      setMsgs((m) => m.slice(0, -1));
      setQ(text);
      toast(toAppError(e).message, "error");
    } finally {
      setBusy(false);
    }
  };
  const onRef = (m: ChatMessage) => (href: string) => {
    const r = href.match(/^#([tfl])=([FL]\d+)(?:@(\d+))?$/);
    const target = r && m.refs[r[2]];
    if (!r || !target) return;
    if (r[1] === "l") onLesson(target);
    else onSource(target, r[3] ? Number(r[3]) : -1);
  };
  const shown = msgs.filter((m) => m.scope === scope);

  return (
    <div className="mx-auto flex h-full max-w-3xl flex-col">
      <div className="mb-3 flex items-center gap-2">
        <div className="inline-flex rounded-md border border-zinc-300 p-0.5 dark:border-zinc-700">
          {(["lesson", "course"] as const).map((s) => (
            <button key={s} className={`rounded px-3 py-1 ${scope === s ? "bg-sky-600 text-white" : "text-zinc-600 dark:text-zinc-400"}`} onClick={() => setScope(s)}>
              {s === "lesson" ? t("Questa lezione") : t("Tutto il corso")}
            </button>
          ))}
        </div>
        <span className="flex-1 text-xs text-zinc-500">{scope === "lesson" ? t("Risponde da trascrizioni, appunti e slide di questa lezione.") : t("Risponde dalle sintesi di tutte le lezioni del corso.")}</span>
        {shown.length > 0 && (
          <button
            className="btn"
            title={t("Cancella la conversazione")}
            onClick={async () => {
              await chat.clear(lesson.id, scope);
              setMsgs((m) => m.filter((x) => x.scope !== scope));
            }}
          >
            <Trash2 className="size-4" />
          </button>
        )}
      </div>
      <div className="min-h-0 flex-1 space-y-4 overflow-auto pb-4">
        {shown.length === 0 && !busy && (
          <div className="pt-8 text-center text-zinc-500">
            <MessageCircleQuestion className="mx-auto mb-2 size-8 text-zinc-400" />
            <p>{t("Fai una domanda: la risposta cita le fonti e puoi cliccare i riferimenti.")}</p>
            <div className="mt-4 flex flex-wrap justify-center gap-2">
              {examples(scope).map((e) => (
                <button key={e} className="rounded-full border border-zinc-300 px-3 py-1 text-sm hover:bg-zinc-100 dark:border-zinc-700 dark:hover:bg-zinc-900" onClick={() => ask(e)}>
                  {e}
                </button>
              ))}
            </div>
          </div>
        )}
        {shown.map((m, i) =>
          m.role === "user" ? (
            <div key={i} className="ml-auto max-w-[80%] rounded-2xl rounded-br-sm bg-sky-600 px-4 py-2 whitespace-pre-wrap text-white select-text">
              {m.text}
            </div>
          ) : (
            <div key={i} className="rounded-2xl rounded-bl-sm border border-zinc-200 bg-white px-4 py-3 dark:border-zinc-800 dark:bg-zinc-900">
              <Markdown text={m.text} onRef={onRef(m)} />
            </div>
          ),
        )}
        {busy && <div className="animate-pulse text-sm text-zinc-500">{t("Gemini sta leggendo le fonti…")}</div>}
        <div ref={end} />
      </div>
      <form
        className="flex gap-2 border-t border-zinc-200 pt-3 dark:border-zinc-800"
        onSubmit={(e) => {
          e.preventDefault();
          ask(q);
        }}
      >
        <textarea
          className="field min-h-10 flex-1 resize-none"
          rows={2}
          placeholder={scope === "lesson" ? t("Chiedi qualcosa su questa lezione…") : t("Chiedi qualcosa su tutto il corso…")}
          value={q}
          onChange={(e) => setQ(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              ask(q);
            }
          }}
        />
        <button className="btn btn-primary self-end" disabled={busy || !q.trim()} title={t("Invia (Invio; Maiusc+Invio per andare a capo)")}>
          <SendHorizontal className="size-4" />
        </button>
      </form>
    </div>
  );
}

