import { useSyncExternalStore } from "react";
import { en } from "./en";

/**
 * Interface language. The Italian text is the key: `t("Nuovo corso")` gives it back as it is
 * in Italian and looks it up in `en.ts` in English (falling back to the Italian, and logged in
 * development, when a translation is missing; `pnpm i18n:check` lists them). Variables are
 * written `{name}`: `t("Lezione del {date}", { date })`.
 */
export type Lang = "it" | "en";
export type UiLang = "system" | Lang;

const KEY = "lezioni:lang";

/** The system's language: Italian for Italian, English for anything else. */
export const systemLang = (): Lang => ((navigator.languages?.[0] ?? navigator.language ?? "it").toLowerCase().startsWith("it") ? "it" : "en");

function read(): UiLang {
  try {
    const v = localStorage.getItem(KEY);
    return v === "it" || v === "en" ? v : "system";
  } catch {
    return "system";
  }
}

let choice: UiLang = read();
let lang: Lang = choice === "system" ? systemLang() : choice;
const listeners = new Set<() => void>();
document.documentElement.lang = lang;

/** Sets the choice (from the settings, or when the user changes it). */
export function setUiLang(c: UiLang) {
  if (c !== "it" && c !== "en") c = "system";
  choice = c;
  try {
    if (c === "system") localStorage.removeItem(KEY);
    else localStorage.setItem(KEY, c);
  } catch {
    /* not persisted: settings.json has it */
  }
  const next = c === "system" ? systemLang() : c;
  if (next === lang) return listeners.forEach((l) => l());
  lang = next;
  document.documentElement.lang = lang;
  listeners.forEach((l) => l());
}

export const uiLang = (): Lang => lang;
export const uiChoice = (): UiLang => choice;

/** Re-renders on a change of language; returns the language in use. */
export function useUiLang(): Lang {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => lang,
  );
}

const missing = new Set<string>();

function fill(s: string, vars?: Record<string, string | number>): string {
  return vars ? s.replace(/\{(\w+)\}/g, (m, k: string) => (k in vars ? String(vars[k]) : m)) : s;
}

/** `it` in the language `l`. A key may carry a context after "|" when one Italian word needs two
 * English ones ("Nota|segnaposto"): the context is not shown. */
export function tFor(l: Lang, it: string, vars?: Record<string, string | number>): string {
  if (l === "it") return fill(it.split("|")[0], vars);
  const e = en[it];
  if (e === undefined && import.meta.env.DEV && !missing.has(it)) {
    missing.add(it);
    console.warn("[i18n] manca:", JSON.stringify(it));
  }
  return fill(e ?? it.split("|")[0], vars);
}

/** `it` in the interface language. */
export const t = (it: string, vars?: Record<string, string | number>) => tFor(lang, it, vars);

/** Singular or plural by `n` (also as `{n}`): `tn(n, "1 lezione", "{n} lezioni")`. */
export function tnFor(l: Lang, n: number, one: string, other: string, vars?: Record<string, string | number>): string {
  return tFor(l, n === 1 ? one : other, { n, ...vars });
}
export const tn = (n: number, one: string, other: string, vars?: Record<string, string | number>) => tnFor(lang, n, one, other, vars);

/** BCP 47 locale for dates and numbers. */
export const localeOf = (l: Lang) => (l === "it" ? "it-IT" : "en-GB");
export const locale = () => localeOf(lang);
