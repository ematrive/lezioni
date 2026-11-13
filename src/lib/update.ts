// At startup: if GitHub has a newer release, offer to install it (signed with the project's
// updater key, see docs/rilascio.md). Silent when offline or in development.
import { ask } from "@tauri-apps/plugin-dialog";
import { relaunch } from "@tauri-apps/plugin-process";
import { check } from "@tauri-apps/plugin-updater";
import { t } from "../i18n";
import { toast } from "./toast";

export async function checkForUpdate() {
  if (import.meta.env.DEV) return;
  try {
    const u = await check();
    if (!u) return;
    const ok = await ask(
      t("È disponibile Lezioni {version} (hai la {current}).", { version: u.version, current: u.currentVersion }) +
        (u.body ? `\n\n${u.body.slice(0, 600)}` : "") +
        `\n\n${t("Aggiornare ora? L'app si riavvierà.")}`,
      {
        title: t("Aggiornamento disponibile"),
        okLabel: t("Aggiorna"),
        cancelLabel: t("Più tardi"),
      },
    );
    if (!ok) return;
    toast(t("Download dell'aggiornamento…"));
    await u.downloadAndInstall();
    await relaunch();
  } catch (e) {
    console.warn("aggiornamenti:", e);
  }
}
