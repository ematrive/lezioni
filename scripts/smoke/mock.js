// Fake Tauri backend for the UI smoke test: an in-memory archive, no files touched.
(() => {
  const cbs = {};
  const listeners = {}; // event name -> handler ids
  let n = 1;
  const id = () => "id-" + n++;
  const now = () => new Date().toISOString();
  let libraryDir = null;
  const courses = [];
  const lessons = [];
  const outputs = {};
  let aiProvider = "gemini";
  const chats = {};
  const costs = {}; // lesson id -> [{ when, total }]
  const marks = {}; // lesson id -> bookmarks
  // An earlier summary (as after a Rigenera), for the versions dialog.
  const OLD_SUMMARY = "## Metodo del simplesso\nSi parte da una **base** e ci si sposta sui vertici. [F1 00:12:30]\n\n$$\\max\\; c^T x \\quad \\text{s.t. } Ax \\le b,\\ x \\ge 0$$";
  const versions = {}; // lesson id -> versions
  const reviewed = {}; // "lesson|question" -> { box, due }
  const dueCards = (ls) => {
    const day = now().slice(0, 10);
    const cards = ls.flatMap((l) => (outputs[l.id]?.study?.flashcard ?? []).map((c) => ({ lessonId: l.id, lessonTitle: l.title, key: c.domanda, domanda: c.domanda, risposta: c.risposta, box: reviewed[l.id + "|" + c.domanda]?.box ?? null })));
    const old = cards.filter((c) => c.box !== null && reviewed[c.lessonId + "|" + c.key].due <= day);
    const fresh = cards.filter((c) => c.box === null);
    return { cards: [...old, ...fresh.slice(0, 20)], due: old.length, new: fresh.length };
  };
  let globalTerms = "";
  // A real PDF of 3 landscape pages ("slides"), for pdf.js in the side panel.
  const slidesPdf = (() => {
    const titles = ["Il metodo del simplesso", "Il pivot", "Degenerazione e regola di Bland"];
    const objs = ["<< /Type /Catalog /Pages 2 0 R >>", `<< /Type /Pages /Kids [${titles.map((_, i) => `${3 + i * 2} 0 R`).join(" ")}] /Count ${titles.length} >>`];
    titles.forEach((t, i) => {
      const stream = `0.85 0.92 1 rg 0 0 640 360 re f 0.05 0.3 0.6 rg BT /F1 30 Tf 40 300 Td (Slide ${i + 1}) Tj ET 0 0 0 rg BT /F1 24 Tf 40 240 Td (${t}) Tj ET`;
      objs.push(`<< /Type /Page /Parent 2 0 R /MediaBox [0 0 640 360] /Resources << /Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> >> >> /Contents ${4 + i * 2} 0 R >>`);
      objs.push(`<< /Length ${stream.length} >>\nstream\n${stream}\nendstream`);
    });
    let out = "%PDF-1.4\n";
    const offs = [];
    objs.forEach((o, i) => {
      offs.push(out.length);
      out += `${i + 1} 0 obj\n${o}\nendobj\n`;
    });
    const x = out.length;
    out += `xref\n0 ${objs.length + 1}\n0000000000 65535 f \n` + offs.map((o) => `${String(o).padStart(10, "0")} 00000 n \n`).join("") + `trailer\n<< /Size ${objs.length + 1} /Root 1 0 R >>\nstartxref\n${x}\n%%EOF\n`;
    return new TextEncoder().encode(out).buffer;
  })();
  const courseSummaries = {}; // course id -> summary
  const sortMarks = (lid) => (marks[lid] = (marks[lid] ?? []).sort((x, y) => x.at - y.at));
  const kind = (f) => {
    const e = f.split(".").pop().toLowerCase();
    if (["mp3", "m4a", "wav", "ogg"].includes(e)) return "audio";
    if (e === "pdf") return "pdf";
    if (["png", "jpg", "jpeg", "heic"].includes(e)) return "image";
    if (["md", "txt"].includes(e)) return "text";
    return "other";
  };
  const src = (file, originalName, size) => ({ id: id(), file, kind: kind(file), originalName, size, added: now() });
  const course = (c) => ({ terms: "", ...c, lessons: lessons.filter((l) => l.courseId === c.id).length });
  const find = (lid) => {
    const l = lessons.find((x) => x.id === lid);
    if (!l) throw { kind: "other", message: "lezione non trovata" };
    return l;
  };
  window.__emit = (event, payload) => (listeners[event] || []).forEach((h) => cbs[h]?.({ event, id: 0, payload }));
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
  window.__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: "main" }, currentWebview: { windowLabel: "main", label: "main" } },
    // A sheet of handwritten-looking notes, for photos in the screenshots.
    convertFileSrc: (p) =>
      "data:image/svg+xml," +
      encodeURIComponent(
        '<svg xmlns="http://www.w3.org/2000/svg" width="400" height="300"><rect width="400" height="300" fill="#fdfcf7"/>' +
          Array.from({ length: 9 }, (_, i) => `<path d="M30 ${50 + i * 26} q 40 -8 80 0 t 80 0 t 80 0 t ${60 + (i % 3) * 30} 0" stroke="#1e3a8a" stroke-width="3" fill="none" opacity="0.75"/>`).join("") +
          '<rect x="250" y="150" width="110" height="90" fill="none" stroke="#b91c1c" stroke-width="3"/></svg>',
      ) +
      "#" +
      p,
    transformCallback: (fn) => {
      const i = n++;
      cbs[i] = fn;
      return i;
    },
    unregisterCallback: () => {},
    invoke: async (cmd, a, opts) => {
      window.__calls = (window.__calls || []).concat([cmd]);
      switch (cmd) {
        case "plugin:event|listen":
          (listeners[a.event] ||= []).push(a.handler);
          return a.handler;
        case "plugin:event|unlisten":
          listeners[a.event] = (listeners[a.event] || []).filter((h) => h !== a.eventId);
          return null;
        case "plugin:window|set_theme":
          return null;
        case "plugin:dialog|ask":
          return true;
        // ask() with custom labels: the answer is the label of the button pressed (always OK here).
        case "plugin:dialog|message":
          return a.buttons?.OkCancelCustom?.[0] ?? a.buttons?.OkCustom ?? (a.buttons === "YesNo" ? "Yes" : "Ok");
        case "plugin:dialog|save":
          return "/home/studente/Scrivania/" + (a.options?.defaultPath ?? "file");
        case "export_write_text":
          window.__written = { dest: a.dest, text: a.text };
          return null;
        case "export_print":
          window.__printed = a.html;
          return "/tmp/stampa.html";
        case "opened_packages":
          return [];
        case "chat_load":
          return chats[a.lessonId] ?? [];
        case "chat_clear":
          chats[a.lessonId] = (chats[a.lessonId] ?? []).filter((m) => m.scope !== a.scope);
          return null;
        case "chat_ask": {
          const l = find(a.lessonId);
          const audio = l.sources.find((s) => s.kind === "audio");
          const at = now();
          chats[a.lessonId] = [
            ...(chats[a.lessonId] ?? []),
            { role: "user", text: a.question, scope: a.scope, at, refs: {} },
            { role: "model", text: "Il **test del rapporto minimo** sceglie la variabile uscente [F1 00:12:30].", scope: a.scope, at, refs: audio ? { F1: audio.id } : {} },
          ];
          return { messages: chats[a.lessonId], usage: { promptTokenCount: 30000, candidatesTokenCount: 300, thoughtsTokenCount: 0 } };
        }
        case "search_archive": {
          const q = a.query.toLowerCase();
          const hits = [];
          // Like the real backend (search.rs): no Markdown marks or [hh:mm:ss] in the snippet.
          const clean = (t) => t.replace(/\[\d{1,2}:\d{2}(?::\d{2})?\]/g, "").replace(/[#*`$_>|]/g, "").replace(/\s+/g, " ").trim().slice(0, 140);
          for (const l of lessons) {
            const c = courses.find((x) => x.id === l.courseId);
            const o = outputs[l.id];
            for (const [sid, t] of Object.entries(o?.extracts ?? {}))
              if (t.toLowerCase().includes(q)) hits.push({ courseId: c.id, course: c.name, lessonId: l.id, lesson: l.title, date: l.date, area: "extract", sourceId: sid, label: "registrazione 10-11.m4a", snippet: clean(t), time: 750, score: 1 });
            if (o?.summary?.toLowerCase().includes(q)) hits.push({ courseId: c.id, course: c.name, lessonId: l.id, lesson: l.title, date: l.date, area: "summary", sourceId: null, label: "Sintesi", snippet: clean(o.summary), time: null, score: 1 });
          }
          return hits;
        }
        case "package_inspect":
          return { course: { name: "Economia applicata", teacher: "Verdi" }, exported: now(), lessons: [{ id: "pkg-l1", title: "Break-even", date: "2026-11-05", sources: 3, omitted: 1, exists: false }], matchingCourse: null };
        case "package_import": {
          const c = { id: id(), name: "Economia applicata", teacher: "Verdi", created: now(), path: `${libraryDir}/Economia applicata` };
          courses.push(c);
          lessons.push({ id: "pkg-l1", title: "Break-even", date: "2026-11-05", notes: "", created: now(), sources: [{ id: "s-rec", file: "reg.m4a", kind: "audio", originalName: "reg.m4a", size: 1, added: now(), omitted: true }], courseId: c.id, path: `${c.path}/2026-11-05 Break-even` });
          return { courseId: c.id, imported: ["pkg-l1"], skipped: [] };
        }
        case "settings_get":
          return { libraryDir, suggestedDir: "/home/studente/Documenti/Lezioni" };
        case "settings_set_library":
          libraryDir = a.dir;
          return null;
        case "courses_list":
          return courses.map(course);
        case "course_create": {
          const c = { id: id(), name: a.name, teacher: a.teacher, terms: a.terms ?? "", created: now(), path: `${libraryDir}/${a.name}` };
          courses.push(c);
          return course(c);
        }
        case "course_update": {
          const c = courses.find((x) => x.id === a.id);
          Object.assign(c, { name: a.name, teacher: a.teacher, terms: a.terms });
          return course(c);
        }
        case "extract_save": {
          const l = find(a.lessonId);
          outputs[l.id].extracts[a.sourceId] = a.text;
          l.sources.find((s) => s.id === a.sourceId).edited = now();
          return l;
        }
        case "summary_save":
          outputs[a.lessonId].summary = a.text;
          outputs[a.lessonId].info.edited = now();
          return null;
        case "study_save":
          outputs[a.lessonId].study = a.study;
          outputs[a.lessonId].info.edited = now();
          return null;
        case "course_costs": {
          const ls = lessons.filter((l) => l.courseId === a.courseId).sort((x, y) => y.date.localeCompare(x.date));
          const rows = ls.map((l) => ({ lessonId: l.id, title: l.title, date: l.date, total: (costs[l.id] ?? []).reduce((t, e) => t + e.total, 0) }));
          const months = {};
          for (const l of ls) for (const e of costs[l.id] ?? []) months[e.when.slice(0, 7)] = (months[e.when.slice(0, 7)] ?? 0) + e.total;
          const total = rows.reduce((t, r) => t + r.total, 0);
          return { lessons: rows, course: 0, months: Object.entries(months).map(([month, total]) => ({ month, total })), total, estimated: false };
        }
        case "lessons_list":
          return lessons.filter((l) => l.courseId === a.courseId).sort((x, y) => y.date.localeCompare(x.date));
        case "lesson_get":
          return find(a.id);
        case "lesson_create": {
          const c = courses.find((x) => x.id === a.courseId);
          const l = { id: id(), title: a.title || "Lezione", date: a.date, notes: "", created: now(), sources: [], courseId: a.courseId, path: `${c.path}/${a.date} ${a.title}` };
          lessons.push(l);
          return l;
        }
        case "lesson_update":
          return Object.assign(find(a.id), Object.fromEntries(Object.entries(a.patch).filter(([, v]) => v !== undefined)));
        case "sources_add_files": {
          const l = find(a.lessonId);
          for (const p of a.paths) l.sources.push(src(p.split("/").pop(), p.split("/").pop(), 12_345_678));
          return { lesson: l, warnings: [] };
        }
        case "source_add_bytes": {
          const l = find(opts.headers["lesson-id"]);
          l.sources.push(src(opts.headers["file-name"], opts.headers["file-name"], a.length));
          return l;
        }
        case "source_add_text": {
          const l = find(a.lessonId);
          l.sources.push(src(`${a.title || "Testo incollato"}.md`, a.title || "Testo incollato", a.text.length));
          return l;
        }
        case "source_remove": {
          const l = find(a.lessonId);
          l.sources = l.sources.filter((s) => s.id !== a.sourceId);
          return l;
        }
        case "ai_settings_get":
          return {
            hasKey: true,
            hasOpenrouterKey: true,
            provider: aiProvider,
            models: { engine: "local", transcribe: "gemini-3.5-flash-lite", generate: "gemini-3.8-flash", fallback: "gemini-3.5-flash-lite" },
            openrouter: { transcribe: "google/gemini-3.5-flash-lite", generate: "anthropic/claude-haiku-5.5", fallback: "google/gemini-3.5-flash-lite" },
            whisperReady: true,
          };
        case "ai_settings_set":
          aiProvider = a.provider;
          return null;
        case "ai_check":
          return a.provider === "openrouter" ? "Credito residuo: 9,62 $ (usati 0,38 $)" : "Funziona";
        case "openrouter_models":
          return [
            { id: "anthropic/claude-haiku-5.5", name: "Anthropic: Claude Haiku 5.5", input: 0.1, output: 0.5, images: true, files: true },
            { id: "deepseek/deepseek-v4-flash", name: "DeepSeek: V4 Flash", input: 0.09, output: 0.18, images: false, files: false },
            { id: "google/gemini-3.5-flash-lite", name: "Google: Gemini 3.5 Flash Lite", input: 0.3, output: 2.5, images: true, files: true },
          ];
        case "whisper_status":
          return { ready: true, downloading: null, error: null };
        case "lesson_outputs":
          return { ...(outputs[a.id] ?? { extracts: {}, raw: {}, summary: null, map: null, study: null, info: null }), bookmarks: marks[a.id] ?? [], versions: versions[a.id] ?? [], slides: [], ...(outputs[a.id]?.slides && { slides: outputs[a.id].slides }), photos: outputs[a.id]?.photos ?? [] };
        case "terms_get":
          return globalTerms;
        case "terms_set":
          globalTerms = a.terms;
          return null;
        case "course_glossary": {
          const byKey = {};
          for (const l of lessons.filter((x) => x.courseId === a.courseId))
            for (const t of outputs[l.id]?.study?.glossario ?? []) (byKey[t.termine.toLowerCase()] ??= { termine: t.termine, definizione: t.definizione, lessons: [] }).lessons.push([l.id, l.title]);
          return Object.values(byKey).sort((x, y) => x.termine.localeCompare(y.termine));
        }
        case "course_summary_get": {
          const ls = lessons.filter((l) => l.courseId === a.courseId && outputs[l.id]?.summary);
          return { summary: null, questions: [], info: null, ready: ls.length, changed: 0, ...courseSummaries[a.courseId] };
        }
        case "course_summarize": {
          const ls = lessons.filter((l) => l.courseId === a.courseId && outputs[l.id]?.summary).sort((x, y) => x.date.localeCompare(y.date));
          const send = (message) => cbs[a.channel.id]?.({ index: 0, message });
          setTimeout(() => {
            courseSummaries[a.courseId] = {
              summary:
                "## Programmazione lineare\nUn problema di PL si scrive in **forma standard** $\\max\\, c^T x$ con $Ax = b,\\ x \\ge 0$; le soluzioni di base sono i vertici del poliedro. [L1]\n\n## Il metodo del simplesso\nSi passa da un vertice all'altro con il **pivot**: la variabile entrante ha costo ridotto negativo, la uscente si sceglie con il test del rapporto minimo. Con rapporti uguali la soluzione è **degenere** e serve la regola di Bland. [L1]",
              questions: [{ domanda: "Illustra il metodo del simplesso e il caso degenere.", traccia: "Forma standard, base ammissibile, pivot, rapporto minimo, regola di Bland.", lezioni: ["L1"] }],
              info: { generated: now(), model: "anthropic/claude-haiku-5.5", usage: { promptTokenCount: 9000, candidatesTokenCount: 2500, thoughtsTokenCount: 0, cost: 0.006 }, refs: Object.fromEntries(ls.map((l, i) => ["L" + (i + 1), l.id])) },
              ready: ls.length,
              changed: 0,
            };
            send({ event: "done", usage: { promptTokenCount: 9000, candidatesTokenCount: 2500, thoughtsTokenCount: 0, cost: 0.006 }, warnings: [] });
          }, 300);
          return null;
        }
        case "source_read":
          return slidesPdf.slice(0);
        case "source_set_role": {
          const s = find(a.lessonId).sources.find((x) => x.id === a.sourceId);
          Object.assign(s, { role: a.role, roleGuessed: false });
          return find(a.lessonId);
        }
        case "source_set_info": {
          const s = find(a.lessonId).sources.find((x) => x.id === a.sourceId);
          Object.assign(s, { title: a.title, description: a.description });
          return find(a.lessonId);
        }
        case "photos_link":
          return [];
        case "photo_set_link": {
          const o = outputs[a.lessonId];
          o.photos = (o.photos ?? []).filter((x) => x.image !== a.imageId).concat([{ image: a.imageId, recording: a.recordingId ?? "", at: a.recordingId ? a.at : null, by: "mano" }]);
          return null;
        }
        case "photo_forget_link":
          outputs[a.lessonId].photos = (outputs[a.lessonId].photos ?? []).filter((x) => x.image !== a.imageId);
          return null;
        case "slides_align":
          return outputs[a.lessonId].slides[0]?.steps.length ?? 0;
        case "extract_mark_pauses": {
          const o = outputs[a.lessonId];
          o.extracts[a.sourceId] = o.extracts[a.sourceId].replace("[00:03:10] grazie\n\n[00:03:40] grazie grazie grazie grazie", "[00:03:10] *(pausa fino a 00:12:30)*");
          o.pauses = {};
          find(a.lessonId).sources.find((s) => s.id === a.sourceId).edited = now();
          return 1;
        }
        case "review_due":
          return dueCards(a.scope === "lesson" ? [find(a.id)] : lessons.filter((l) => l.courseId === a.id));
        case "review_answer": {
          const k = a.lessonId + "|" + a.key;
          const box = a.knew ? Math.min((reviewed[k]?.box ?? 0) + 1, 5) : 0;
          const due = new Date(Date.now() + [0, 1, 3, 7, 14, 30][box] * 864e5).toISOString().slice(0, 10);
          return (reviewed[k] = { box, due, seen: 1, wrong: a.knew ? 0 : 1 });
        }
        case "version_read":
          return OLD_SUMMARY;
        case "version_restore":
          outputs[a.lessonId].summary = OLD_SUMMARY;
          return find(a.lessonId);
        case "extract_reclean":
          return { text: "[00:12:30] La funzione obiettivo è $\\max\\, c^T x$ con i vincoli $Ax \\le b$, come spiega bene il libro di TIM.", usage: { promptTokenCount: 900, candidatesTokenCount: 120, thoughtsTokenCount: 0, cost: 0.0004 }, model: "google/gemini-3.5-flash-lite" };
        case "bookmark_add":
          (marks[a.lessonId] ??= []).push({ id: id(), sourceId: a.sourceId, at: a.at, kind: a.kind, note: a.note, created: now() });
          return sortMarks(a.lessonId);
        case "bookmark_update":
          Object.assign(marks[a.lessonId].find((b) => b.id === a.id), { kind: a.kind, note: a.note });
          return sortMarks(a.lessonId);
        case "bookmark_remove":
          marks[a.lessonId] = marks[a.lessonId].filter((b) => b.id !== a.id);
          return sortMarks(a.lessonId);
        case "lesson_process": {
          // Fake run: a few steps, then the outputs below.
          const l = find(a.lessonId);
          const send = (() => {
            let i = 0;
            return (message) => cbs[a.channel.id]?.({ index: i++, message });
          })();
          const audio = l.sources.find((s) => s.kind === "audio");
          const pdf = l.sources.find((s) => s.kind === "pdf");
          const board = l.sources.find((s) => s.originalName === "lavagna.jpg");
          const pasted = l.sources.find((s) => s.kind === "image" && s !== board);
          setTimeout(() => send({ event: "step", done: 0.33, total: 3, message: "Trascrizione sul computer di registrazione.m4a, parte 2 di 3: 00:20:00–00:40:00 di 00:52:10" }), 50);
          setTimeout(() => send({ event: "detail", message: "Whisper: 43% · ripulitura della parte 1" }), 80);
          window.__finishJob = () => {
            outputs[l.id] = {
              extracts: audio ? { [audio.id]: "### Introduzione\n[00:00:05] Oggi vediamo il metodo del simplesso.\n\n[00:03:10] grazie\n\n[00:03:40] grazie grazie grazie grazie\n\n[00:12:30] La funzione obiettivo è $\\max\\, c^T x$ con vincoli $Ax \\le b$. Lo spiega bene il libro di Team." } : {},
              pauses: audio ? { [audio.id]: 1 } : {},
              ...(pdf && { pdfText: { [pdf.id]: "## Pagina 1\n**Metodo del simplesso**: si va di vertice in vertice. *(nota a mano: ricordare la regola di Bland per evitare i cicli!)*\n\n*(grafico del poliedro con i vertici numerati)*\n\n## Pagina 2\nIl pivot: variabile entrante e test del rapporto minimo.\n\n## Pagina 3\nDegenerazione e regola di Bland." } }),
              slides: audio && pdf ? [{ recording: audio.id, pdf: pdf.id, pages: 3, steps: [{ at: 5, page: 1 }, { at: 750, page: 2 }] }] : [],
              // The board photo found from the time it was taken; the pasted screenshot not found.
              photos: [...(audio && board ? [{ image: board.id, recording: audio.id, at: 750, by: "orario" }] : []), ...(pasted ? [{ image: pasted.id, recording: "", at: null, by: "modello", why: "nella trascrizione non si parla di questo schema" }] : [])],
              photoText: { ...(board && { [board.id]: "*(poliedro disegnato alla lavagna, con i vertici numerati da 1 a 5)*" }), ...(pasted && { [pasted.id]: "Schema del tableau con le colonne delle variabili di base." }) },
              raw: audio ? { [audio.id]: "[00:00:05] Allora, ehm, oggi vediamo il metodo del simplesso, cioè...\n\n[00:12:30] la funzione obiettivo è max c trasposto x con vincoli a x minore uguale b. Lo spiega bene il libro di team." } : {},
              summary: "## Metodo del simplesso\nSi parte da una **base ammissibile** e ci si sposta sui vertici. [F1 00:12:30]\n\n" + (board ? "![Il poliedro alla lavagna](F2)\n\n" : "") + "$$\\max\\; c^T x \\quad \\text{s.t. } Ax \\le b,\\ x \\ge 0$$\n\n| Passo | Azione |\n|---|---|\n| 1 | Scegli la variabile entrante |\n| 2 | Test del rapporto minimo |",
              map: { etichetta: "Simplesso", figli: [{ etichetta: "Forma standard", figli: [{ etichetta: "Variabili di slack" }] }, { etichetta: "Base ammissibile" }, { etichetta: "Pivot", rif: "F1 00:12:30", figli: [{ etichetta: "Variabile entrante" }, { etichetta: "Rapporto minimo" }] }] },
              study: {
                glossario: [{ termine: "Base", definizione: "Insieme di m variabili linearmente indipendenti." }],
                flashcard: [{ domanda: "Cos'è il test del rapporto minimo?", risposta: "Sceglie la variabile uscente mantenendo l'ammissibilità: $\\min_i b_i / a_{ij}$ con $a_{ij} > 0$." }],
                domandeEsame: [{ domanda: "Illustra il metodo del simplesso.", traccia: "Forma standard, base iniziale, pivot, criterio di ottimalità." }],
              },
              info: { generated: now(), model: "gemini-3.8-flash", usage: { promptTokenCount: 120000, candidatesTokenCount: 15000, thoughtsTokenCount: 2000, cost: 0.034 }, suggestedTitle: "Simplesso", sources: { ...(audio && { F1: audio.id }), ...(board && { F2: board.id }) } },
            };
            versions[l.id] = [{ area: "generati", stamp: "2026-11-10 09-15-00", files: ["info.json", "mappa.json", "mappa.mmd", "sintesi.md", "studio.json"] }];
            Object.assign(outputs[l.id].extracts, outputs[l.id].pdfText ?? {});
            Object.assign(outputs[l.id].extracts, outputs[l.id].photoText);
            delete outputs[l.id].photoText;
            if (pdf) Object.assign(pdf, { role: "slide", roleGuessed: true });
            delete outputs[l.id].pdfText;
            costs[l.id] = (costs[l.id] ?? []).concat([{ when: now(), total: 0.034 }]);
            send({ event: "done", usage: { promptTokenCount: 120000, candidatesTokenCount: 15000, thoughtsTokenCount: 2000, cost: 0.034 }, warnings: [] });
          };
          return null;
        }
        default:
          return null;
      }
    },
  };
})();
