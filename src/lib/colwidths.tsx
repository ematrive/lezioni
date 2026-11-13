import { useCallback, useEffect, useRef, useState } from "react";
import { t } from "../i18n";

export const MIN_COL_W = 40;
export const MAX_COL_W = 1200;
const clamp = (w: number) => Math.round(Math.max(MIN_COL_W, Math.min(MAX_COL_W, w)));

function load(storageKey?: string): Record<string, number> {
  if (!storageKey) return {};
  try {
    const raw = localStorage.getItem(`colw:${storageKey}`);
    return raw ? JSON.parse(raw) : {};
  } catch {
    return {};
  }
}

/** User-set column widths keyed by column id, optionally remembered per `storageKey`. */
export function useColumnWidths(storageKey?: string) {
  const [custom, setCustom] = useState<Record<string, number>>(() => load(storageKey));
  const keyRef = useRef(storageKey);
  useEffect(() => {
    if (keyRef.current !== storageKey) {
      keyRef.current = storageKey;
      setCustom(load(storageKey));
    }
  }, [storageKey]);
  const save = useCallback(
    (next: Record<string, number>) => {
      setCustom(next);
      if (!storageKey) return;
      try {
        if (Object.keys(next).length) localStorage.setItem(`colw:${storageKey}`, JSON.stringify(next));
        else localStorage.removeItem(`colw:${storageKey}`);
      } catch {
        /* storage unavailable: widths stay for this session */
      }
    },
    [storageKey],
  );
  const customRef = useRef(custom);
  customRef.current = custom;
  const set = useCallback((id: string, w: number) => save({ ...customRef.current, [id]: clamp(w) }), [save]);
  const setMany = useCallback(
    (ws: Record<string, number>) => save({ ...customRef.current, ...Object.fromEntries(Object.entries(ws).map(([k, w]) => [k, clamp(w)])) }),
    [save],
  );
  const reset = useCallback(() => save({}), [save]);
  return { custom, set, setMany, reset };
}

let canvas: HTMLCanvasElement | null = null;
/** Pixel width of the widest text, measured with the given CSS font. */
export function measureMax(texts: Iterable<string>, font: string): number {
  canvas ??= document.createElement("canvas");
  const ctx = canvas.getContext("2d");
  if (!ctx) return 0;
  ctx.font = font;
  let max = 0;
  for (const t of texts) max = Math.max(max, ctx.measureText(t.length > 300 ? t.slice(0, 300) : t).width);
  return max;
}

/**
 * Drag handle on the right edge of a header cell. Click/pointer events stop here so the
 * header's own click (sorting) never fires when resizing.
 */
export function ResizeHandle({ width, onResize, onFit }: { width: number; onResize: (w: number) => void; onFit?: () => void }) {
  return (
    <div
      className="absolute top-0 right-0 bottom-0 z-10 w-[6px] cursor-col-resize hover:bg-sky-500/60 active:bg-sky-500"
      title={t("Trascina per ridimensionare · doppio clic per adattare")}
      onPointerDown={(e) => {
        e.stopPropagation();
        e.preventDefault();
        const el = e.currentTarget;
        el.setPointerCapture(e.pointerId);
        const x0 = e.clientX;
        const move = (ev: PointerEvent) => onResize(width + ev.clientX - x0);
        const up = () => {
          el.removeEventListener("pointermove", move);
          el.removeEventListener("pointerup", up);
          el.removeEventListener("pointercancel", up);
        };
        el.addEventListener("pointermove", move);
        el.addEventListener("pointerup", up);
        el.addEventListener("pointercancel", up);
      }}
      onClick={(e) => e.stopPropagation()}
      onDoubleClick={(e) => {
        e.stopPropagation();
        onFit?.();
      }}
    />
  );
}

/**
 * Resizable columns for plain HTML tables: render `<colgroup>{cols}</colgroup>` with
 * `table-layout: fixed` and put `handle(i)` inside each relatively-positioned `<th>`.
 */
export function useTableColumns(ids: string[], defaults: number[], storageKey?: string) {
  const { custom, set, reset } = useColumnWidths(storageKey);
  const widths = ids.map((id, i) => custom[id] ?? defaults[i] ?? 120);
  const tableRef = useRef<HTMLTableElement>(null);
  const fit = (i: number) => {
    const t = tableRef.current;
    if (!t) return;
    const texts: string[] = [];
    let font = "";
    for (const row of Array.from(t.rows).slice(0, 501)) {
      const cell = row.cells[i];
      if (!cell) continue;
      texts.push(cell.innerText);
      if (!font && row.parentElement?.tagName === "TBODY") font = getComputedStyle(cell).font;
    }
    if (!font && t.rows[0]?.cells[i]) font = getComputedStyle(t.rows[0].cells[i]).font;
    set(ids[i], measureMax(texts, font) + 24);
  };
  return {
    tableRef,
    tableStyle: { width: widths.reduce((a, b) => a + b, 0) },
    cols: widths.map((w, i) => <col key={ids[i]} style={{ width: w }} />),
    handle: (i: number) => <ResizeHandle width={widths[i]} onResize={(w) => set(ids[i], w)} onFit={() => fit(i)} />,
    reset,
    customized: Object.keys(custom).length > 0,
  };
}

/**
 * Size of a pane the user can drag: a width (`axis: "x"`, side panes) or a height
 * (`axis: "y"`, editor over results, detail under a list…), between `min` and `max`,
 * remembered under `pane:<storageKey>`. Double click on the handle restores `def`.
 *
 * `edge` is where the handle sits on the pane: "end" (right / bottom edge: dragging right
 * or down grows it) or "start" (left / top edge: dragging left or up grows it). The handle
 * is absolutely positioned, so the pane needs `relative`. The upper bound also leaves
 * `reserve` px (default 360 wide, 160 tall) to the content next to it.
 *
 * Every side pane or split of the app should use this (see CLAUDE.md).
 */
export function usePaneSize(storageKey: string, opts: { axis?: "x" | "y"; def: number; min: number; max: number; edge?: "start" | "end"; reserve?: number }) {
  const { axis = "x", def, min, max, edge = "end" } = opts;
  const reserve = opts.reserve ?? (axis === "x" ? 360 : 160);
  const k = `pane:${storageKey}`;
  const fit = useCallback(
    (v: number) => Math.round(Math.max(min, Math.min(max, (axis === "x" ? window.innerWidth : window.innerHeight) - reserve, v))),
    [min, max, axis, reserve],
  );
  const [size, setSize] = useState(() => {
    try {
      const v = Number(localStorage.getItem(k));
      return v ? fit(v) : def;
    } catch {
      return def;
    }
  });
  const sizeRef = useRef(size);
  sizeRef.current = size;
  const save = useCallback(
    (v: number) => {
      setSize(v);
      try {
        if (v === def) localStorage.removeItem(k);
        else localStorage.setItem(k, String(v));
      } catch {
        /* storage unavailable */
      }
    },
    [k, def],
  );
  const pos =
    axis === "x"
      ? `top-0 bottom-0 w-[6px] cursor-col-resize ${edge === "end" ? "-right-[3px]" : "-left-[3px]"}`
      : `left-0 right-0 h-[6px] cursor-row-resize ${edge === "end" ? "-bottom-[3px]" : "-top-[3px]"}`;
  const handle = (
    <div
      className={`absolute z-20 ${pos} hover:bg-sky-500/60 active:bg-sky-500`}
      title={`Trascina per ridimensionare (${min}–${max}px) · doppio clic per ripristinare`}
      onPointerDown={(e) => {
        e.preventDefault();
        const el = e.currentTarget;
        el.setPointerCapture(e.pointerId);
        const p0 = axis === "x" ? e.clientX : e.clientY;
        const s0 = sizeRef.current;
        const sign = edge === "end" ? 1 : -1;
        let last = s0;
        const move = (ev: PointerEvent) => {
          last = fit(s0 + sign * ((axis === "x" ? ev.clientX : ev.clientY) - p0));
          setSize(last);
        };
        const up = () => {
          el.removeEventListener("pointermove", move);
          el.removeEventListener("pointerup", up);
          el.removeEventListener("pointercancel", up);
          save(last);
        };
        el.addEventListener("pointermove", move);
        el.addEventListener("pointerup", up);
        el.addEventListener("pointercancel", up);
      }}
      onDoubleClick={() => save(def)}
    />
  );
  return { size, handle };
}

/** Width of a side pane (table list, key list…): `usePaneSize` on the x axis, handle on the right. */
export function usePaneWidth(storageKey: string, def: number, min: number, max: number) {
  const { size, handle } = usePaneSize(storageKey, { def, min, max });
  return { width: size, handle };
}
