import { useEffect, useRef } from "react";

/** Calls `onTime` with the minute of the moment: the recording's while it plays, and while
 *  scrolling the text the paragraph a third of the way down the view (the one being read),
 *  also inside long paragraphs: the last minute that has gone above that line. Whatever the
 *  user did last wins. `deps` restart the listeners (the getters are read through refs). */
export function useMomentTime(player: () => HTMLMediaElement | null, section: () => HTMLElement | null, onTime: (t: number) => void, deps: unknown[]) {
  const follow = useRef(onTime);
  follow.current = onTime;
  const playerRef = useRef(player);
  playerRef.current = player;
  const sectionRef = useRef(section);
  sectionRef.current = section;
  const scrolledAt = useRef(0);

  // While the recording plays (and the text was not scrolled just now): its minute.
  useEffect(() => {
    const p = playerRef.current();
    if (!p) return;
    const on = () => {
      if (!p.paused && Date.now() - scrolledAt.current > 4000) follow.current(p.currentTime);
    };
    p.addEventListener("timeupdate", on);
    return () => p.removeEventListener("timeupdate", on);
  }, deps);

  useEffect(() => {
    const sec = sectionRef.current();
    const scroller = sec?.closest<HTMLElement>("[data-lesson-content]");
    if (!sec || !scroller) return;
    let frame = 0;
    const update = () => {
      frame = 0;
      const box = scroller.getBoundingClientRect();
      const line = box.top + box.height / 3;
      const r = sec.getBoundingClientRect();
      if (r.bottom < line || r.top > box.bottom) return;
      const times = [...sec.querySelectorAll<HTMLElement>("[data-t]")];
      // At the start of the text, before any minute reaches the line: the first one.
      let t: number | null = times.length ? Number(times[0].dataset.t) : null;
      for (const el of times) {
        if (el.getBoundingClientRect().top > line) break;
        t = Number(el.dataset.t);
      }
      if (t !== null) follow.current(t);
    };
    const onScroll = () => {
      scrolledAt.current = Date.now();
      if (!frame) frame = requestAnimationFrame(update);
    };
    scroller.addEventListener("scroll", onScroll, { passive: true });
    return () => {
      scroller.removeEventListener("scroll", onScroll);
      cancelAnimationFrame(frame);
    };
  }, deps);
}

export const clock = (secs: number) => {
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(Math.floor(secs / 3600))}:${p(Math.floor(secs / 60) % 60)}:${p(Math.floor(secs % 60))}`;
};
