# What's new

## 1.2.0

- **English**: the app can be used in English. By default it follows the system language; you change it in **Settings → Language**.
- Each course has a **language of the lectures** (the one the lecturer speaks, for the transcription) and a **language of the materials** (summary, map, study material, chat and exports). A course taught in English can have its materials in Italian, and vice versa.
- A single lecture can have a **different language** from the course: you choose it from the ⋯ button at the top of the lecture.
- `.lezione` packages carry the course's languages with them; a lecture imported into a course with another language keeps its own.
- The instructions given to the model no longer assume a Management Engineering course: they work for any university course.

## 1.1.0

- **Summaries** no longer show `\n` and `##` instead of line breaks and headings, nor formulas broken across several lines ("mu", "sqrt2g"): this happened when the model garbled the text halfway through its answer. Summaries already saved display correctly again without reprocessing the lecture; the same goes for the glossary, flashcards, exam questions and course summary.
- References to PDF pages in the summary, like **[F1 p. 2]**, become clickable and open the source, like the others.
- **Photos** (whiteboard, notes) are **linked to the minute** of the recording at which the lecturer talks about them: from the time the photo was taken when available, otherwise with the model. In the Transcript they appear in the right paragraph and next to the text, like slides; in Sources each photo shows its minute, or **not linked** with the reason.
- Each photo can have a **title** and a **short description**, written by you: they help the linking and appear instead of the file name.
- **Linking by hand**: from the button on a paragraph of the transcript (even at the minute you are listening to) or from the photo's card in Sources, where you can also unlink it.
- Processing always transcribes the **recordings** first and then reads photos and PDFs, in whatever order you added them.
- The **summary** can show photos where they help understanding, also in the HTML, PDF and Markdown exports.

## 1.0.3

- The **map** no longer ends up with just the lecture's topic: the fallback model (Gemini Flash-Lite on OpenRouter) sometimes answered without the concepts below the root.
- If a slide can't be drawn next to the transcript, the reason appears below the page instead of a blank box.

## 1.0.2

- On Mac the app no longer asks at every launch for access to the Documents folder and for the Keychain password: answer once (**Allow**, and **Always Allow** for the Keychain) and it remembers. After a macOS update it may ask once more.
- The Gemini and OpenRouter keys are kept in a single Keychain item: at most one request instead of two.

## 1.0.1

- Installer for **Windows**: it was missing in 1.0.0 because the Windows build failed. Nothing changes on Mac and Linux.

## 1.0.0

### Transcript
- **Corrections by hand** of transcripts and texts read, with **find and replace**; **Whisper's** text before the cleanup can be seen with the *Cleaned up | Whisper* switch.
- **Clean up a piece again**: select a passage that came out badly and a model (even a different one) redoes it from Whisper's text; you choose whether to replace it.
- **Breaks**: the words Whisper makes up during a silence or a break ("grazie grazie grazie…" [thank you thank you…]) become a grey line "Break 00:12:23 – 00:16:19" and don't reach the model. For lectures already processed there is the "breaks to fix" button; by hand, *Mark as break*.
- **Bookmarks while you listen**: *Important* (key I), *Didn't understand* (D) and *Note* (N) mark the minute; the paragraph is highlighted and the summary takes them into account.
- **Find in the lecture** (⌘F / Ctrl+F) and, with several documents, the **In this lecture** row and the menu on the name to move from one to another.
- From search, the summary and the map, the transcript goes to the paragraph at that minute.

### Slides and PDFs
- **Slides linked to the recording**: each PDF is either *Lecture slides* or *Other material* (the app suggests which). The slides are linked to the minutes of the lecture and in the Transcript the **current slide appears next to the text**, following the audio and your scrolling.
- **Handwritten notes** on slides and photos are transcribed and shown in blue, with a pencil.
- With OpenRouter, PDFs can be up to 50 MB.

### Study
- **Spaced review** of flashcards, for a lecture or for the whole course: the ones you know come back after 1, 3, 7, 14 and 30 days.
- The summary, flashcards, exam questions and glossary can be corrected by hand; **formulas** are shown as formulas in flashcards too.
- **Earlier versions**: summaries replaced by Regenerate and redone transcripts can be compared with the current one and restored.
- **Map**: the arrow next to a concept takes you to that point of the recording; **Export to PDF** prints it drawn on a landscape page (also in the lecture's PDF).

### Courses
- **Course page** (click the course name): review, **course summary** by topic with the most recurrent exam questions, course **glossary** and **costs** by month and by lecture.
- **Course terms** and **terms for all courses** (Settings): terms and names suggested to Whisper, the cleanup and the summary.

### Interface
- **⋯** menu for courses and lectures, a courses column you can **hide**, the lecture in **full screen** (Esc to go back), buttons all the same height.
- At the bottom left, the **version** with **what's new** (a blue dot when there is something new).

### Reliability
- If the summary model answers with nothing or writes a truncated answer, the app retries (also with the fallback model) and never replaces the saved summary.
- Fixed: with OpenRouter, resuming after a failed cleanup tried to send the audio ("too large for OpenRouter"); now it restarts from Whisper's text.
- Messages during processing no longer mention Gemini when OpenRouter is in use.

## 0.7.0

- **OpenRouter** as an alternative to Gemini (Settings → AI service): one key for hundreds of models, paid as you go. Next to each model the app shows the price, whether it reads images and PDFs and what a lecture would cost; at the end of processing it tells you the **real cost**. Defaults: Gemini 3.5 Flash-Lite for cleaning up and reading, Claude Haiku 5.5 for the summary. Only providers that don't use the data to train models.
- With Gemini, cleaning up and reading the sources now use `gemini-3.5-flash-lite` (faster and less often overloaded).
- The button is now called **Process**.
- Headings that some models write after the timestamp are put in the right place.

## 0.6.0

- **Transcription on the computer** with Whisper: free, with no quota limits and without sending the audio to Google. The model (574 MB) is downloaded only once, also from Settings; the download carries on with Settings closed and can be cancelled. Gemini then cleans up the text part by part (paragraphs, headings, formulas, students' questions and comments); if it doesn't respond, Whisper's text with its timestamps is kept, and the cleanup is retried next time. Transcription with Gemini is still available in Settings.
- If the Mac's graphics card can't transcribe, Whisper redoes the part on the processor (slower).
- **Fallback model**: when Google replies twice that the model is overloaded, the request goes to `gemini-3.5-flash-lite` (can be changed in Settings) and the app tells you.

## 0.5.1

- **Clearer processing**: the bar moves forward part by part and a second line says what is happening (upload to Gemini with the MB sent, waiting for the answer, new attempts when Gemini is busy) and for how long.
- No more endless waits: if Gemini doesn't respond within 10 minutes the app retries only once, and an upload stuck for a minute and a half stops with a message. Before, a wait could last more than an hour with no sign of life.
- An internal error now ends processing with a message, instead of leaving the bar stuck.
- Removing a source explains what happens: the copy in the archive goes to the Trash, the original stays where it was.
- Fixed the right-hand corner of **Process with Gemini** next to the menu arrow.
- **Log file** (Settings → Diagnostics → Show the log file) to attach to your reports.

## 0.5.0

- **Guide** in the app (*Guide* button at the bottom left or <kbd>F1</kbd>): all the chapters of the manual with table of contents, search and screenshots, available offline too.

## 0.4.0

- **Questions**: a chat about the lecture (it answers from transcripts, notes and slides) or about the whole course (from the lecture summaries). Answers cite the sources: [F1 00:12:30] reopens the recording at that minute, [L3] opens the lecture. The conversation is saved in the lecture folder and is never included in shared packages; your personal notes go into the package only if you choose so.

## 0.3.0

- **Search** across the whole archive (titles, notes, transcripts, texts of the sources, summaries, flashcards), ignoring accents and upper case. A result in a transcript opens the lecture at the right minute.
- **Share** a lecture, or a whole course from the course menu:
  - **.lezione package** for people who use Lezioni: it opens with a double-click or by dragging it into the window, with sources, transcripts and summaries ready to use. Recordings are excluded by default (they are large and you need the lecturer's permission): the transcript is included anyway;
  - **PDF**: a handout with summary, map, flashcards, questions, glossary and, optionally, the transcripts in an appendix; the browser's print dialog opens, where you choose "Save as PDF";
  - **web page** in a single file, readable on a phone and offline too;
  - **Markdown** for Obsidian and NotebookLM;
  - **flashcards for Anki**.
- **Import lectures…**: if a lecture is already there you can leave it, replace it or keep both.

## 0.2.0

- **Process with Gemini**: recordings are split by the app into 20-minute parts and transcribed one part at a time (even lectures several hours long; each minute is paid for only once). If processing is interrupted, for example by the daily quota, the next run resumes from the parts already done. Photos, notes and PDFs are read with formulas and descriptions of charts, and from all the sources together you get the summary, concept map, glossary, flashcards and exam questions.
- Transcript with clickable timestamps: the audio plays from that point. In the summary, references like [F1 00:12:30] lead to the transcript and the recording.
- A second processing reuses the transcripts already made (you only pay for the summary); "Process the sources again too" redoes everything. Earlier versions stay in `generati/versioni`.
- Settings: Gemini API key (in the keychain), key check, choice of models.
- LaTeX formulas and tables in transcripts and summaries.

## 0.1.0

- Archive in a folder of your choice, with a subfolder per course and one per lecture.
- Courses: create, rename, delete to the Trash.
- Lectures with title, date and notes; list by date, filter.
- Sources: drag files from Finder / File Explorer, choose from a window, paste images and screenshots, typed or pasted text. Photo previews and listening to recordings in the app.
