// At startup: if GitHub has a newer release, offer to install it (signed with the project's
// updater key, see docs/rilascio.md). Silent when offline or in development.
import { ask } from "@tauri-apps/plugin-dialog";
import { relaunch } from "@tauri-apps/plugin-process";
import { check } from "@tauri-apps/plugin-updater";
import { toast } from "./toast";

export async function checkForUpdate() {
  if (import.meta.env.DEV) return;
  try {
    const u = await check();
    if (!u) return;
    const ok = await ask(`È disponibile Lezioni ${u.version} (hai la ${u.currentVersion}).${u.body ? `\n\n${u.body.slice(0, 600)}` : ""}\n\nAggiornare ora? L'app si riavvierà.`, {
      title: "Aggiornamento disponibile",
      okLabel: "Aggiorna",
      cancelLabel: "Più tardi",
    });
    if (!ok) return;
    toast("Download dell'aggiornamento…");
    await u.downloadAndInstall();
    await relaunch();
  } catch (e) {
    console.warn("aggiornamenti:", e);
  }
}
