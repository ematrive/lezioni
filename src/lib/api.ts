import { Channel, convertFileSrc, invoke } from "@tauri-apps/api/core";

export type SourceKind = "audio" | "video" | "pdf" | "image" | "text" | "document" | "other";

export interface Source {
  id: string;
  file: string;
  kind: SourceKind;
  originalName: string;
  size: number;
  added: string;
  /** Not in the package this lesson came from (usually the recording): transcript only. */
  omitted?: boolean;
}

export interface Course {
  id: string;
  name: string;
  teacher: string;
  created: string;
  path: string;
  lessons: number;
}

export interface Lesson {
  id: string;
  title: string;
  /** YYYY-MM-DD */
  date: string;
  notes: string;
  created: string;
  sources: Source[];
  courseId: string;
  path: string;
}

export interface AppError {
  kind: "noLibrary" | "other";
  message: string;
}

export function toAppError(e: unknown): AppError {
  if (e && typeof e === "object" && "message" in e) return { kind: "other", ...(e as object) } as AppError;
  return { kind: "other", message: String(e) };
}

export interface SettingsView {
  libraryDir: string | null;
  suggestedDir: string | null;
}

export const api = {
  settings: () => invoke<SettingsView>("settings_get"),
  setLibrary: (dir: string) => invoke<void>("settings_set_library", { dir }),

  courses: () => invoke<Course[]>("courses_list"),
  createCourse: (name: string, teacher: string) => invoke<Course>("course_create", { name, teacher }),
  updateCourse: (id: string, name: string, teacher: string) => invoke<Course>("course_update", { id, name, teacher }),
  deleteCourse: (id: string) => invoke<void>("course_delete", { id }),

  lessons: (courseId: string) => invoke<Lesson[]>("lessons_list", { courseId }),
  lesson: (id: string) => invoke<Lesson>("lesson_get", { id }),
  createLesson: (courseId: string, title: string, date: string) => invoke<Lesson>("lesson_create", { courseId, title, date }),
  updateLesson: (id: string, patch: { title?: string; date?: string; notes?: string }) => invoke<Lesson>("lesson_update", { id, patch }),
  deleteLesson: (id: string) => invoke<void>("lesson_delete", { id }),

  addFiles: (lessonId: string, paths: string[]) => invoke<{ lesson: Lesson; warnings: string[] }>("sources_add_files", { lessonId, paths }),
  addBytes: (lessonId: string, fileName: string, bytes: Uint8Array) => invoke<Lesson>("source_add_bytes", bytes, { headers: { "lesson-id": lessonId, "file-name": fileName } }),
  addText: (lessonId: string, title: string, text: string) => invoke<Lesson>("source_add_text", { lessonId, title, text }),
  removeSource: (lessonId: string, sourceId: string) => invoke<Lesson>("source_remove", { lessonId, sourceId }),
  openSource: (lessonId: string, sourceId: string) => invoke<void>("source_open", { lessonId, sourceId }),
  reveal: (path: string) => invoke<void>("reveal", { path }),
  revealLog: () => invoke<void>("log_reveal"),
};

/** Path of a source on disk. */
export function sourceFile(lesson: Lesson, s: Source): string {
  const sep = lesson.path.includes("\\") ? "\\" : "/";
  return [lesson.path, "fonti", s.file].join(sep);
}

/** URL the webview can load (img, audio, video) for a file of the archive. */
export const fileUrl = (path: string) => convertFileSrc(path);

export function fmtSize(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 ** 2) return `${(n / 1024).toFixed(0)} KB`;
  if (n < 1024 ** 3) return `${(n / 1024 ** 2).toFixed(1)} MB`;
  return `${(n / 1024 ** 3).toFixed(2)} GB`;
}

export function fmtDate(iso: string): string {
  const [y, m, d] = iso.split("-").map(Number);
  return new Date(y, m - 1, d).toLocaleDateString("it-IT", { weekday: "short", day: "numeric", month: "short", year: "numeric" });
}

export function today(): string {
  const d = new Date();
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

// ---------- AI ----------

export interface Models {
  /** Recordings: Whisper on the computer, or Gemini. */
  engine: "local" | "gemini";
  transcribe: string;
  generate: string;
  /** Used when the model above is overloaded (503); empty = off. */
  fallback: string;
}

export interface Usage {
  promptTokenCount: number;
  candidatesTokenCount: number;
  thoughtsTokenCount: number;
}

export interface MapNode {
  etichetta: string;
  figli?: MapNode[];
}

export interface Study {
  glossario: { termine: string; definizione: string }[];
  flashcard: { domanda: string; risposta: string }[];
  domandeEsame: { domanda: string; traccia: string }[];
}

export interface Outputs {
  /** source id → extracted text */
  extracts: Record<string, string>;
  summary: string | null;
  map: MapNode | null;
  study: Study | null;
  info: { generated: string; model: string; usage: Usage; suggestedTitle: string; sources: Record<string, string> } | null;
}

export type JobEvent =
  | { event: "step"; done: number; total: number; message: string }
  | { event: "detail"; message: string }
  | { event: "done"; usage: Usage; warnings: string[] }
  | { event: "failed"; message: string };

export const ai = {
  settings: () => invoke<{ hasKey: boolean; models: Models; whisperReady: boolean }>("ai_settings_get"),
  /** Starts the download in the background; follow it with `whisperStatus`. */
  whisperDownload: () => invoke<void>("whisper_download"),
  whisperStatus: () => invoke<{ ready: boolean; downloading: string | null; error: string | null }>("whisper_status"),
  whisperCancel: () => invoke<void>("whisper_cancel"),
  whisperDelete: () => invoke<void>("whisper_delete"),
  setSettings: (key: string | null, models: Models) => invoke<void>("ai_settings_set", { key, models }),
  check: (key: string | null) => invoke<void>("ai_check", { key }),
  outputs: (id: string) => invoke<Outputs>("lesson_outputs", { id }),
  cancel: (jobId: string) => invoke<boolean>("job_cancel", { jobId }),
  async process(lessonId: string, options: { force?: boolean; extractOnly?: boolean }, onEvent: (e: JobEvent) => void): Promise<string> {
    const jobId = crypto.randomUUID();
    const channel = new Channel<JobEvent>();
    channel.onmessage = onEvent;
    await invoke("lesson_process", { lessonId, options, jobId, channel });
    return jobId;
  },
};

/** Paid-tier list price of gemini-3.8-flash (USD per 1M tokens, Nov 2026): rough cost of a run. */
export function costEstimate(u: Usage): number {
  return (u.promptTokenCount * 0.75 + (u.candidatesTokenCount + u.thoughtsTokenCount) * 3.75) / 1e6;
}

// ---------- search and sharing ----------

export interface Hit {
  courseId: string;
  course: string;
  lessonId: string;
  lesson: string;
  date: string;
  area: "title" | "notes" | "summary" | "study" | "extract";
  sourceId: string | null;
  label: string;
  snippet: string;
  time: number | null;
  score: number;
}

export interface PackageInfo {
  course: { name: string; teacher: string };
  exported: string;
  lessons: { id: string; title: string; date: string; sources: number; omitted: number; exists: boolean }[];
  matchingCourse: string | null;
}

export type OnExisting = "skip" | "copy" | "replace";

export const share = {
  search: (query: string) => invoke<Hit[]>("search_archive", { query }),
  exportPackage: (lessonIds: string[], include: { media: boolean; files: boolean; notes: boolean }, dest: string) => invoke<void>("export_package", { lessonIds, include, dest }),
  inspect: (path: string) => invoke<PackageInfo>("package_inspect", { path }),
  import: (path: string, courseId: string | null, onExisting: OnExisting) => invoke<{ courseId: string; imported: string[]; skipped: string[] }>("package_import", { path, courseId, onExisting }),
  markdown: (lessonIds: string[], dest: string) => invoke<number>("export_markdown", { lessonIds, dest }),
  anki: (lessonIds: string[], dest: string) => invoke<number>("export_anki", { lessonIds, dest }),
  writeText: (dest: string, text: string) => invoke<void>("export_write_text", { dest, text }),
  print: (name: string, html: string) => invoke<string>("export_print", { name, html }),
  opened: () => invoke<string[]>("opened_packages"),
};

// ---------- chat ----------

export type ChatScope = "lesson" | "course";
export interface ChatMessage {
  role: "user" | "model";
  text: string;
  scope: ChatScope;
  at: string;
  /** "F1" → source id, "L1" → lesson id */
  refs: Record<string, string>;
}

export const chat = {
  load: (lessonId: string) => invoke<ChatMessage[]>("chat_load", { lessonId }),
  clear: (lessonId: string, scope: ChatScope) => invoke<void>("chat_clear", { lessonId, scope }),
  ask: (lessonId: string, scope: ChatScope, question: string) => invoke<{ messages: ChatMessage[]; usage: Usage }>("chat_ask", { lessonId, scope, question }),
};
