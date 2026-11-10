import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useCallback, useEffect, useState } from "react";
import { api, share, toAppError, type Course, type Hit, type SettingsView } from "./lib/api";
import { Welcome } from "./features/settings/Welcome";
import { CourseList } from "./features/courses/CourseList";
import { LessonList } from "./features/courses/LessonList";
import { LessonView, type Focus } from "./features/lesson/LessonView";
import { SettingsDialog } from "./features/settings/SettingsDialog";
import { SearchResults } from "./features/search/SearchResults";
import { ImportDialog, pickPackage } from "./features/share/ImportDialog";
import { ShareDialog } from "./features/share/ShareDialog";
import { Guide } from "./features/help/Guide";
import { usePaneSize } from "./lib/colwidths";
import { Toasts, toast } from "./lib/toast";
import { checkForUpdate } from "./lib/update";

const LAST_COURSE = "lezioni:course";
const LAST_LESSON = "lezioni:lesson";

function remembered(k: string): string | null {
  try {
    return localStorage.getItem(k);
  } catch {
    return null;
  }
}
function remember(k: string, v: string | null) {
  try {
    if (v) localStorage.setItem(k, v);
    else localStorage.removeItem(k);
  } catch {
    /* not persisted */
  }
}

export const isPackage = (p: string) => p.toLowerCase().endsWith(".lezione");

export default function App() {
  const [settings, setSettings] = useState<SettingsView | null>(null);
  const [courses, setCourses] = useState<Course[]>([]);
  const [courseId, setCourseId] = useState<string | null>(() => remembered(LAST_COURSE));
  const [lessonId, setLessonId] = useState<string | null>(() => remembered(LAST_LESSON));
  /** Bumped when a lesson changes, so the list reloads. */
  const [lessonsRev, setLessonsRev] = useState(0);
  const [showSettings, setShowSettings] = useState(false);
  const [query, setQuery] = useState("");
  const [focus, setFocus] = useState<Focus | null>(null);
  /** `.lezione` files waiting to be imported (double click, drag & drop, menu). */
  const [packages, setPackages] = useState<string[]>([]);
  const [exporting, setExporting] = useState<Course | null>(null);
  const [guide, setGuide] = useState(false);
  const closeGuide = useCallback(() => setGuide(false), []);
  const coursePane = usePaneSize("courses", { def: 230, min: 170, max: 420 });
  const lessonPane = usePaneSize("lessons", { def: 300, min: 220, max: 520 });

  const clearLesson = useCallback(() => setLessonId(null), []);
  const loadSettings = useCallback(() => api.settings().then(setSettings), []);
  useEffect(() => {
    loadSettings();
  }, [loadSettings]);
  useEffect(() => {
    checkForUpdate();
    const k = (e: KeyboardEvent) => {
      if (e.key === "F1") {
        e.preventDefault();
        setGuide((g) => !g);
      }
    };
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  }, []);

  const loadCourses = useCallback(async () => {
    try {
      const cs = await api.courses();
      setCourses(cs);
      setCourseId((cur) => (cur && cs.some((c) => c.id === cur) ? cur : (cs[0]?.id ?? null)));
    } catch (e) {
      toast(toAppError(e).message, "error");
    }
  }, []);
  useEffect(() => {
    if (settings?.libraryDir) loadCourses();
  }, [settings?.libraryDir, loadCourses]);

  useEffect(() => remember(LAST_COURSE, courseId), [courseId]);
  useEffect(() => remember(LAST_LESSON, lessonId), [lessonId]);

  // Packages: opened with the app (double click), or dropped anywhere on the window.
  const ready = !!settings?.libraryDir;
  useEffect(() => {
    if (!ready) return;
    const take = () => share.opened().then((ps) => ps.length && setPackages((cur) => [...cur, ...ps]));
    take();
    const un1 = listen("packages-opened", take);
    const un2 = getCurrentWebview().onDragDropEvent((e) => {
      if (e.payload.type === "drop") {
        const ps = e.payload.paths.filter(isPackage);
        if (ps.length) setPackages((cur) => [...cur, ...ps]);
      }
    });
    return () => {
      un1.then((f) => f());
      un2.then((f) => f());
    };
  }, [ready]);

  const openHit = (h: Hit) => {
    setCourseId(h.courseId);
    setLessonId(h.lessonId);
    const tab = h.area === "extract" ? "transcript" : h.area === "summary" ? "summary" : h.area === "study" ? "study" : "sources";
    setFocus({ lessonId: h.lessonId, tab, sourceId: h.sourceId, secs: h.time, n: Date.now() });
  };

  const importNext = async () => {
    const p = await pickPackage();
    if (p) setPackages((cur) => [...cur, p]);
  };

  if (!settings) return null;
  if (!settings.libraryDir)
    return (
      <>
        <Welcome suggested={settings.suggestedDir} onDone={loadSettings} onHelp={() => setGuide(true)} />
        {guide && <Guide onClose={closeGuide} />}
      </>
    );

  const course = courses.find((c) => c.id === courseId) ?? null;

  return (
    <div className="flex h-full">
      <aside className="relative shrink-0 border-r border-zinc-200 bg-zinc-100/70 dark:border-zinc-800 dark:bg-zinc-900/60" style={{ width: coursePane.size }}>
        <CourseList
          courses={courses}
          selected={query.trim() ? null : courseId}
          query={query}
          onQuery={setQuery}
          onSelect={(id) => {
            setQuery("");
            setCourseId(id);
            setLessonId(null);
          }}
          onChanged={loadCourses}
          onSettings={() => setShowSettings(true)}
          onImport={importNext}
          onExport={setExporting}
          onHelp={() => setGuide(true)}
        />
        {coursePane.handle}
      </aside>
      <section className="relative shrink-0 border-r border-zinc-200 dark:border-zinc-800" style={{ width: lessonPane.size }}>
        {query.trim().length >= 2 ? (
          <SearchResults query={query} onOpen={openHit} />
        ) : course ? (
          <LessonList
            course={course}
            rev={lessonsRev}
            selected={lessonId}
            onSelect={setLessonId}
            onChanged={() => {
              setLessonsRev((r) => r + 1);
              loadCourses();
            }}
          />
        ) : (
          <Empty text="Crea un corso per cominciare, oppure importa un pacchetto .lezione ricevuto da un compagno." />
        )}
        {lessonPane.handle}
      </section>
      <main className="min-w-0 flex-1">
        {lessonId ? (
          <LessonView
            key={lessonId}
            lessonId={lessonId}
            course={courses.find((c) => c.id === courseId) ?? null}
            focus={focus?.lessonId === lessonId ? focus : null}
            onMissing={clearLesson}
            onOpenLesson={(id) => {
              setLessonId(id);
              setFocus({ lessonId: id, tab: "summary", sourceId: null, secs: null, n: Date.now() });
            }}
            onChanged={() => setLessonsRev((r) => r + 1)}
            onDeleted={() => {
              setLessonId(null);
              setLessonsRev((r) => r + 1);
              loadCourses();
            }}
          />
        ) : (
          <Empty text={course ? "Scegli una lezione o creane una nuova." : ""} />
        )}
      </main>
      {showSettings && (
        <SettingsDialog
          libraryDir={settings.libraryDir}
          onClose={() => setShowSettings(false)}
          onChanged={() => {
            setLessonId(null);
            loadSettings();
          }}
        />
      )}
      {exporting && <ShareDialog course={exporting} lesson={null} onClose={() => setExporting(null)} />}
      {packages.length > 0 && (
        <ImportDialog
          key={packages[0]}
          path={packages[0]}
          courses={courses}
          onClose={() => setPackages((cur) => cur.slice(1))}
          onImported={(cid, lid) => {
            setPackages((cur) => cur.slice(1));
            setQuery("");
            loadCourses();
            setCourseId(cid);
            setLessonId(lid);
            setLessonsRev((r) => r + 1);
          }}
        />
      )}
      {guide && <Guide onClose={closeGuide} />}
      <Toasts />
    </div>
  );
}

function Empty({ text }: { text: string }) {
  return <div className="flex h-full items-center justify-center p-6 text-center text-zinc-400">{text}</div>;
}
