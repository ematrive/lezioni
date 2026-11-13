// "Novità": the CHANGELOG for the users (CHANGELOG.md in Italian, CHANGELOG.en.md in English),
// bundled at build time.
import changelogIt from "../../../CHANGELOG.md?raw";
import changelogEn from "../../../CHANGELOG.en.md?raw";
import { t, uiLang } from "../../i18n";
import { Markdown } from "../../lib/Markdown";
import { Modal } from "../../lib/Modal";

const SEEN = "lezioni:news-seen";

export const VERSION = __APP_VERSION__;

/** The changes of this version not yet looked at (a dot next to the version). */
export function hasNews(): boolean {
  try {
    return localStorage.getItem(SEEN) !== VERSION;
  } catch {
    return false;
  }
}

export function markNewsSeen() {
  try {
    localStorage.setItem(SEEN, VERSION);
  } catch {
    /* not remembered */
  }
}

/** The changelog without its title; "Prossima versione" only in the development build (it is
 *  what has been done since the last release, not yet in an installed app). */
function text(): string {
  const changelog = uiLang() === "en" ? changelogEn : changelogIt;
  const body = changelog.replace(/^#\s+.*\n+/, "");
  if (import.meta.env.DEV) return body.replace(/^## (?:Prossima versione|Next version)/m, `## ${t("In sviluppo (non ancora rilasciata)")}`);
  return body.replace(/^## (?:Prossima versione|Next version)\n[\s\S]*?(?=^## )/m, "");
}

export function WhatsNew({ onClose }: { onClose: () => void }) {
  return (
    <Modal wide title={t("Novità · Lezioni {version}", { version: VERSION })} onClose={onClose}>
      <div className="max-h-[65vh]">
        <Markdown text={text()} />
      </div>
    </Modal>
  );
}
