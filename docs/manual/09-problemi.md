# Troubleshooting and FAQ

**"Quota exhausted" during processing.** Gemini's free tier has a daily limit. The parts of the recording already transcribed are saved: run **Process** again the next day and it picks up from there. Alternatively, enable billing on Google, or switch to OpenRouter (a few cents per lecture).

**"Gemini is overloaded" (error 503).** These are Google's servers, full at that moment (it happens mostly on the free tier). The app retries by itself and after two refusals switches to the fallback model; if that is not enough, try again later: the parts already done stay saved. Transcription on the computer is not affected.

**"OpenRouter credit exhausted".** Top up your credit at [openrouter.ai/settings/credits](https://openrouter.ai/settings/credits); **Check** in Settings shows how much is left.

**"Model not found on OpenRouter".** The model name is wrong or the model has been withdrawn: choose one from the field's suggestions in Settings.

**Photos and handwritten notes are not read (OpenRouter).** The model for cleaning up and reading is "text only": in Settings, choose one that reads images (for example `google/gemini-3.5-flash-lite`).

**"Part from … not cleaned up".** Whisper transcribed the part but the AI service could not clean it up: the lecture has Whisper's text, with the timestamps, and at the next processing the app retries just the cleanup.

**The transcription model download fails.** You need 574 MB free and a stable connection; the download starts again from the beginning on each attempt. You can start it from **Settings → Download now**.

**Processing seems stuck.** Look at the line below the bar: it says what it is waiting for and for how long. Uploading a part depends on the connection; Gemini's answer for a 20-minute part usually arrives in 1–3 minutes. If Gemini doesn't answer within 10 minutes, the app retries once and then stops with an error; if the upload makes no progress for a minute and a half, it stops straight away. The parts already done stay saved.

**Reporting a problem.** In **Settings → Diagnostics**, **Show the log file** opens the folder containing `lezioni.log`, which records what the app did (it contains file names, but not the API key or the text of the lectures): attach it to your report together with a screenshot.

**"API key not valid".** Check that you copied the whole key from [aistudio.google.com/apikey](https://aistudio.google.com/apikey) and press **Check** in Settings.

**The recording is sent whole and costs more.** This happens with formats the app cannot split (for example opus). Convert the recording to m4a or mp3 and add it again.

**A Word or PowerPoint file is skipped.** Gemini and OpenRouter don't read Office files: export it to PDF and add the PDF.

**The transcript has an incomplete piece.** The app tells you and doesn't save the incomplete text: process the lecture again with **Process the sources again too**.

**On Mac the app doesn't open ("unidentified developer" or "damaged").** The app is not signed by Apple. The first time, right-click the app → **Open** → **Open**. If that is not enough: System Settings → Privacy & Security → **Open Anyway**.

**On Mac you see "“Lezioni” would like to access files in your Documents folder", or it asks for the Keychain password.** This happens on first launch and after an update, only once: answer **Allow** and, for the Keychain, type your Mac password and choose **Always Allow**. The password is your Mac's, not the app's: it is needed to read the saved API key. If you keep the archive outside Documents, Desktop and Downloads (for example in your Home folder, `~/Lezioni`), the question about Documents no longer appears.

**On Windows you see "Windows protected your PC".** Click **More info** → **Run anyway**.

**Updates.** At start-up the app checks whether there is a new version and asks whether to install it; then it restarts by itself.

**Backups.** Just copy the archive folder, or keep it inside Google Drive, iCloud or Dropbox. Settings and the API key are per computer: on a new computer, choose the same folder and enter the key again.

**The PDF doesn't open in the browser (Linux).** Some browsers installed as Snap or Flatpak can't open temporary files: use **Web page**, save the file in a folder of your own, open it and print it to PDF from the browser.
