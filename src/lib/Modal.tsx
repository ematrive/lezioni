import { useEffect, type ReactNode } from "react";

export function Modal({ title, onClose, children, footer, wide }: { title: string; onClose: () => void; children: ReactNode; footer?: ReactNode; wide?: boolean }) {
  useEffect(() => {
    const k = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  }, [onClose]);
  return (
    <div className="anim-fade fixed inset-0 z-40 flex items-center justify-center bg-black/30 p-6" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className={`anim-pop flex max-h-full w-full flex-col rounded-lg bg-white shadow-xl dark:bg-zinc-900 ${wide ? "max-w-2xl" : "max-w-md"}`}>
        <h2 className="border-b border-zinc-200 px-4 py-3 text-base font-semibold dark:border-zinc-800">{title}</h2>
        <div className="min-h-0 overflow-auto p-4">{children}</div>
        {footer && <div className="flex justify-end gap-2 border-t border-zinc-200 px-4 py-3 dark:border-zinc-800">{footer}</div>}
      </div>
    </div>
  );
}
