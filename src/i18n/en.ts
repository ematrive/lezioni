/**
 * English for the interface: Italian text → English, one JSON file per area in `en/` (plain
 * data, read also by the smoke test). `pnpm i18n:check` finds missing and unused entries.
 */
const files = import.meta.glob<Record<string, string>>("./en/*.json", { eager: true, import: "default" });

export const en: Record<string, string> = Object.assign({}, ...Object.values(files));
