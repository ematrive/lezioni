import { useSyncExternalStore } from "react";

interface Toast {
  id: number;
  text: string;
  tone: "info" | "error";
}

let toasts: Toast[] = [];
let next = 1;
const listeners = new Set<() => void>();
const emit = () => listeners.forEach((l) => l());

/** Short message at the bottom right; errors stay longer. */
export function toast(text: string, tone: Toast["tone"] = "info") {
  const id = next++;
  toasts = [...toasts, { id, text, tone }].slice(-4);
  emit();
  setTimeout(
    () => {
      toasts = toasts.filter((t) => t.id !== id);
      emit();
    },
    tone === "error" ? 8000 : 3500,
  );
}

export function Toasts() {
  const list = useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => toasts,
  );
  return (
    <div className="pointer-events-none fixed right-4 bottom-4 z-50 flex max-w-md flex-col gap-2">
      {list.map((t) => (
        <div
          key={t.id}
          className={`anim-toast pointer-events-auto rounded-md px-3 py-2 whitespace-pre-line shadow-lg ${t.tone === "error" ? "bg-red-600 text-white" : "bg-zinc-800 text-white dark:bg-zinc-200 dark:text-zinc-900"}`}
        >
          {t.text}
        </div>
      ))}
    </div>
  );
}
