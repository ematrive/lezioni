// "Novità": the CHANGELOG (Italian, for the users), bundled at build time.
import changelog from "../../../CHANGELOG.md?raw";
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
  const body = changelog.replace(/^#\s+.*\n+/, "");
  if (import.meta.env.DEV) return body.replace(/^## Prossima versione/m, "## In sviluppo (non ancora rilasciata)");
  return body.replace(/^## Prossima versione\n[\s\S]*?(?=^## )/m, "");
}

export function WhatsNew({ onClose }: { onClose: () => void }) {
  return (
    <Modal wide title={`Novità · Lezioni ${VERSION}`} onClose={onClose}>
      <div className="max-h-[65vh]">
        <Markdown text={text()} />
      </div>
    </Modal>
  );
}
