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
  /** When its text was corrected by hand (ISO date). */
  edited?: string;
  /** A PDF: the teacher's slides (linked to the recording) or other material. */
  role?: "slide" | "materiale";
  /** `role` proposed by the model, not chosen. */
  roleGuessed?: boolean;
}

export interface SlideAlignment {
  recording: string;
  pdf: string;
  pages: number;
  steps: { at: number; page: number }[];
}

export interface Course {
  id: string;
  name: string;
  teacher: string;
  /** Recurring terms and names, given to Whisper and to the model. */
  terms: string;
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
  createCourse: (name: string, teacher: string, terms: string) => invoke<Course>("course_create", { name, teacher, terms }),
  updateCourse: (id: string, name: string, teacher: string, terms: string) => invoke<Course>("course_update", { id, name, teacher, terms }),
  deleteCourse: (id: string) => invoke<void>("course_delete", { id }),
  courseCosts: (courseId: string) => invoke<CourseCosts>("course_costs", { courseId }),
  /** Terms for all the courses (`parole.txt` in the archive). */
  globalTerms: () => invoke<string>("terms_get"),
  setGlobalTerms: (terms: string) => invoke<void>("terms_set", { terms }),
  /** The glossaries of the lessons of a course, merged; `lessons` = [id, title]. */
  courseGlossary: (courseId: string) => invoke<{ termine: string; definizione: string; lessons: [string, string][] }[]>("course_glossary", { courseId }),

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

/** USD, with more decimals for small amounts. */
export function fmtUsd(x: number): string {
  return `${(x > 0 && x < 0.01 ? x.toFixed(4) : x.toFixed(2)).replace(".", ",")} $`;
}

export interface CourseCosts {
  lessons: { lessonId: string; title: string; date: string; total: number }[];
  course: number;
  months: { month: string; total: number }[];
  total: number;
  estimated: boolean;
}

export function fmtDate(iso: string): string {
  const [y, m, d] = iso.slice(0, 10).split("-").map(Number);
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

export type ProviderKind = "gemini" | "openrouter";

/** Models on OpenRouter. */
export interface ModelSet {
  transcribe: string;
  generate: string;
  fallback: string;
}

export interface AiSettings {
  hasKey: boolean;
  hasOpenrouterKey: boolean;
  provider: ProviderKind;
  models: Models;
  openrouter: ModelSet;
  whisperReady: boolean;
}

/** A model of OpenRouter's price list (USD per million tokens). */
export interface ModelInfo {
  id: string;
  name: string;
  input: number;
  output: number;
  images: boolean;
  files: boolean;
}

/** Rough cost of a 1 h 30 lesson with one model for everything (cleanup, PDF, summary):
 *  about 75k tokens in and 46k out, measured with tests/openrouter_it.rs. */
export function lessonCost(m: ModelInfo): number {
  return (75_000 * m.input + 46_000 * m.output) / 1e6;
}

export interface Usage {
  promptTokenCount: number;
  candidatesTokenCount: number;
  thoughtsTokenCount: number;
  /** USD actually spent, when the provider says it (OpenRouter). */
  cost?: number;
}

export interface MapNode {
  etichetta: string;
  /** where the sources talk about it: "F1 00:12:30" or "F3" (maps made from Nov 2026) */
  rif?: string;
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
  /** source id → Whisper's text before the cleanup */
  raw: Record<string, string>;
  summary: string | null;
  map: MapNode | null;
  study: Study | null;
  info: { generated: string; model: string; usage: Usage; suggestedTitle: string; sources: Record<string, string>; edited?: string } | null;
  bookmarks: Bookmark[];
  /** earlier versions, newest first */
  versions: Version[];
  /** source id → pauses Whisper filled with invented words, still to be marked */
  pauses: Record<string, number>;
  /** slides linked to recordings */
  slides: SlideAlignment[];
}

export interface Version {
  area: "generati" | "estratti";
  /** UTC "2026-11-11 14-03-22" */
  stamp: string;
  files: string[];
}

/** When a version was made, from its folder name (UTC). */
export function versionDate(stamp: string): Date {
  return new Date(stamp.replace(/^(\d{4}-\d{2}-\d{2}) (\d{2})-(\d{2})-(\d{2}).*$/, "$1T$2:$3:$4Z"));
}

export interface DueCard {
  lessonId: string;
  lessonTitle: string;
  key: string;
  domanda: string;
  risposta: string;
  /** null = never seen */
  box: number | null;
}

export interface CourseSummary {
  summary: string | null;
  questions: { domanda: string; traccia: string; lezioni: string[] }[];
  info: { generated: string; model: string; usage: Usage; refs: Record<string, string> } | null;
  /** lessons with a summary now */
  ready: number;
  /** lessons added or summarized again since */
  changed: number;
}

export const courseAi = {
  summary: (courseId: string) => invoke<CourseSummary>("course_summary_get", { courseId }),
  async summarize(courseId: string, onEvent: (e: JobEvent) => void): Promise<string> {
    const jobId = crypto.randomUUID();
    const channel = new Channel<JobEvent>();
    channel.onmessage = onEvent;
    await invoke("course_summarize", { courseId, jobId, channel });
    return jobId;
  },
};

export const review = {
  /** Cards to review today: seen ones that are due, then at most 20 new ones. */
  due: (scope: "lesson" | "course", id: string) => invoke<{ cards: DueCard[]; due: number; new: number }>("review_due", { scope, id }),
  answer: (lessonId: string, key: string, knew: boolean) => invoke<{ box: number; due: string; seen: number; wrong: number }>("review_answer", { lessonId, key, knew }),
};

export type BookmarkKind = "importante" | "dubbio" | "nota";

export interface Bookmark {
  id: string;
  sourceId: string;
  /** seconds from the start of the recording */
  at: number;
  kind: BookmarkKind;
  note: string;
  created: string;
}

export type JobEvent =
  | { event: "step"; done: number; total: number; message: string }
  | { event: "detail"; message: string }
  | { event: "done"; usage: Usage; warnings: string[] }
  | { event: "failed"; message: string };

export const ai = {
  settings: () => invoke<AiSettings>("ai_settings_get"),
  openrouterModels: () => invoke<ModelInfo[]>("openrouter_models"),
  /** Starts the download in the background; follow it with `whisperStatus`. */
  whisperDownload: () => invoke<void>("whisper_download"),
  whisperStatus: () => invoke<{ ready: boolean; downloading: string | null; error: string | null }>("whisper_status"),
  whisperCancel: () => invoke<void>("whisper_cancel"),
  whisperDelete: () => invoke<void>("whisper_delete"),
  setSettings: (s: { key?: string; openrouterKey?: string; provider: ProviderKind; models: Models; openrouter: ModelSet }) =>
    invoke<void>("ai_settings_set", { key: s.key ?? null, openrouterKey: s.openrouterKey ?? null, provider: s.provider, models: s.models, openrouter: s.openrouter }),
  /** Checks the key typed (or the saved one); returns a line for Settings (e.g. the credit left). */
  check: (provider: ProviderKind, key: string | null) => invoke<string>("ai_check", { provider, key }),
  outputs: (id: string) => invoke<Outputs>("lesson_outputs", { id }),
  saveExtract: (lessonId: string, sourceId: string, text: string) => invoke<Lesson>("extract_save", { lessonId, sourceId, text }),
  saveSummary: (lessonId: string, text: string) => invoke<void>("summary_save", { lessonId, text }),
  saveStudy: (lessonId: string, study: Study) => invoke<void>("study_save", { lessonId, study }),
  /** Cleans again the piece from `from` to `to` (seconds; null = the end) with `model` (null = the cleanup model). */
  reclean: (lessonId: string, sourceId: string, from: number, to: number | null, model: string | null) =>
    invoke<{ text: string; usage: Usage; model: string }>("extract_reclean", { lessonId, sourceId, from, to, model }),
  /** Whisper's invented words on a break → a pause line; returns how many. */
  setRole: (lessonId: string, sourceId: string, role: "slide" | "materiale") => invoke<Lesson>("source_set_role", { lessonId, sourceId, role }),
  /** Links a PDF of slides to the recordings now; returns how many steps were found. */
  alignSlides: (lessonId: string, pdfId: string) => invoke<number>("slides_align", { lessonId, pdfId }),
  /** The bytes of a source (for pdf.js). */
  sourceBytes: (lessonId: string, sourceId: string) => invoke<ArrayBuffer>("source_read", { lessonId, sourceId }),
  markPauses: (lessonId: string, sourceId: string) => invoke<number>("extract_mark_pauses", { lessonId, sourceId }),
  versionRead: (lessonId: string, area: Version["area"], stamp: string, file: string) => invoke<string>("version_read", { lessonId, area, stamp, file }),
  versionRestore: (lessonId: string, area: Version["area"], stamp: string, file: string | null) => invoke<Lesson>("version_restore", { lessonId, area, stamp, file }),
  addBookmark: (lessonId: string, sourceId: string, at: number, kind: BookmarkKind, note = "") => invoke<Bookmark[]>("bookmark_add", { lessonId, sourceId, at, kind, note }),
  updateBookmark: (lessonId: string, id: string, kind: BookmarkKind, note: string) => invoke<Bookmark[]>("bookmark_update", { lessonId, id, kind, note }),
  removeBookmark: (lessonId: string, id: string) => invoke<Bookmark[]>("bookmark_remove", { lessonId, id }),
  cancel: (jobId: string) => invoke<boolean>("job_cancel", { jobId }),
  async process(lessonId: string, options: { force?: boolean; extractOnly?: boolean }, onEvent: (e: JobEvent) => void): Promise<string> {
    const jobId = crypto.randomUUID();
    const channel = new Channel<JobEvent>();
    channel.onmessage = onEvent;
    await invoke("lesson_process", { lessonId, options, jobId, channel });
    return jobId;
  },
};

/** What a run cost: the real figure from OpenRouter, or "would cost" at Gemini's paid list price. */
export function costLine(u: Usage): string {
  return u.cost != null ? `costo: ${fmtUsd(u.cost)}` : `a pagamento sarebbero circa ${fmtUsd(costEstimate(u))}`;
}

/** Paid-tier list price of gemini-3.8-flash (USD per 1M tokens, Nov 2026): rough cost of a run.
 *  Same figure as `Usage::cost_or_estimate` in Rust (course totals). */
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
