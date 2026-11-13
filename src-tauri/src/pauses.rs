//! Pauses in a recording (a break, the recorder left on): Whisper fills the silence with words
//! it learned from subtitles ("grazie grazie grazie…"). Such paragraphs become one line
//! `[00:12:23] *(pausa fino a 00:16:19)*`, shown in grey by the app.
//! The line is written in the lesson's language (`*(pause until 00:16:19)*` in English) and both
//! forms are read, whatever the lesson.
use crate::lang::Lang;
use crate::pipeline::hms;

/// What Whisper writes on silence or noise (lower case, without punctuation).
const INVENTED: &[&str] = &[
    "grazie",
    "grazie a tutti",
    "grazie mille",
    "grazie per la visione",
    "grazie per l'ascolto",
    "grazie per l'attenzione",
    "sottotitoli a cura di qtss",
    "sottotitoli creati dalla comunità amara.org",
    "sottotitoli e revisione a cura di qtss",
    "iscriviti al canale",
    "thank you",
    "thanks",
    "thank you for watching",
    "thanks for watching",
    "please subscribe",
    "subtitles by the amara.org community",
];

/// A single paragraph shorter than this is kept: a real "grazie" said in class.
const MIN_SECS: u64 = 20;

fn time_of(p: &str) -> Option<(u64, &str)> {
    let p = p.trim_start();
    let rest = p.strip_prefix('[')?;
    let (t, rest) = rest.split_once(']')?;
    let parts: Vec<u64> = t.split(':').map(|x| x.parse().ok()).collect::<Option<_>>()?;
    let secs = match parts[..] {
        [h, m, s] => h * 3600 + m * 60 + s,
        [m, s] => m * 60 + s,
        _ => return None,
    };
    Some((secs, rest.trim()))
}

/// The paragraph is already a pause line.
pub fn is_pause(text: &str) -> bool {
    let t = text.trim().trim_matches(['*', '_', '(', ')']).trim();
    t.starts_with("pausa fino a") || t == "pausa fino alla fine" || t.starts_with("pause until") || t == "pause until the end"
}

/// Text made only of what Whisper writes on silence: words of the list, or one short phrase
/// (1–3 words) said again at least three times.
fn invented(text: &str) -> bool {
    let norm: String = text.to_lowercase().chars().map(|c| if c.is_alphanumeric() || c == '\'' || c == '.' { c } else { ' ' }).collect();
    let words: Vec<&str> = norm.split_whitespace().map(|w| w.trim_matches('.')).filter(|w| !w.is_empty()).collect();
    if words.is_empty() {
        return false;
    }
    // Repeated phrases from the list, in any number ("grazie grazie grazie").
    let mut i = 0;
    let mut only_list = true;
    'outer: while i < words.len() {
        for phrase in INVENTED {
            let pw: Vec<&str> = phrase.split_whitespace().map(|w| w.trim_matches('.')).collect();
            if words.len() - i >= pw.len() && words[i..i + pw.len()] == pw[..] {
                i += pw.len();
                continue 'outer;
            }
        }
        only_list = false;
        break;
    }
    if only_list {
        return true;
    }
    (1..=3).any(|n| words.len() >= 3 * n && words.len().is_multiple_of(n) && words.chunks(n).all(|c| c == &words[..n]))
}

/// Turns the paragraphs invented by Whisper into pause lines, and joins pauses next to each other
/// (the end of a part and the start of the next). Returns the text and how many pauses were
/// made or joined (0 = unchanged).
pub fn mark(text: &str, lang: Lang) -> (String, usize) {
    mark_until(text, None, lang)
}

/// As `mark`, for a piece of a recording that ends at `end` (seconds): a pause at its end lasts
/// until there, not "until the end".
pub fn mark_until(text: &str, end: Option<u64>, lang: Lang) -> (String, usize) {
    let paras: Vec<&str> = text.split("\n\n").collect();
    let timed: Vec<Option<(u64, &str)>> = paras.iter().map(|p| time_of(p)).collect();
    let pause: Vec<bool> = timed.iter().map(|t| t.is_some_and(|(_, body)| is_pause(body))).collect();
    let bad: Vec<bool> = timed.iter().zip(&pause).map(|(t, &p)| p || t.is_some_and(|(_, body)| invented(body))).collect();
    let mut out: Vec<String> = Vec::new();
    let mut n = 0;
    let mut i = 0;
    while i < paras.len() {
        if !bad[i] {
            out.push(paras[i].to_string());
            i += 1;
            continue;
        }
        let mut j = i;
        while j < paras.len() && bad[j] {
            j += 1;
        }
        let from = timed[i].unwrap().0;
        // The next paragraph with a time is where the lesson starts again.
        let until = timed[j..].iter().flatten().next().map(|(t, _)| *t).or(end);
        let long = until.is_none_or(|u| u.saturating_sub(from) >= MIN_SECS);
        if j - i == 1 && pause[i] {
            // Already a pause, alone: as it is.
            out.push(paras[i].to_string());
        } else if j - i >= 2 || long {
            // Pauses joined keep the language they were written in.
            let lang = if pause[i] { if paras[i].contains("pause until") { Lang::En } else { Lang::It } } else { lang };
            let to = match (lang, until) {
                (Lang::It, Some(u)) => format!("pausa fino a {}", hms(u)),
                (Lang::It, None) => "pausa fino alla fine".into(),
                (Lang::En, Some(u)) => format!("pause until {}", hms(u)),
                (Lang::En, None) => "pause until the end".into(),
            };
            out.push(format!("[{}] *({to})*", hms(from)));
            n += 1;
        } else {
            out.extend(paras[i..j].iter().map(|p| p.to_string()));
        }
        i = j;
    }
    (if n == 0 { text.to_string() } else { out.join("\n\n") }, n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whispers_thanks_become_one_pause() {
        let t = "[00:11:50] e quindi la diga.\n\n[00:12:23] grazie\n\n[00:12:53] Grazie.\n\n[00:14:23] grazie grazie grazie grazie\n\n[00:16:19] grazie ma una nuova che si riaccende ok, siete?";
        let (m, n) = mark(t, Lang::It);
        assert_eq!(n, 1);
        assert_eq!(m, "[00:11:50] e quindi la diga.\n\n[00:12:23] *(pausa fino a 00:16:19)*\n\n[00:16:19] grazie ma una nuova che si riaccende ok, siete?");
        assert_eq!(mark(&m, Lang::It), (m.clone(), 0), "a pause stays as it is");
    }

    #[test]
    fn pauses_of_two_parts_are_joined() {
        let part = "[00:19:30] fine della parte.\n\n[00:19:40] grazie grazie grazie";
        assert_eq!(mark_until(part, Some(1200), Lang::It).0, "[00:19:30] fine della parte.\n\n[00:19:40] *(pausa fino a 00:20:00)*");
        let joined = "[00:19:40] *(pausa fino a 00:20:00)*\n\n[00:20:00] *(pausa fino a 00:21:10)*\n\n[00:21:10] Riprendiamo.";
        assert_eq!(mark(joined, Lang::It), ("[00:19:40] *(pausa fino a 00:21:10)*\n\n[00:21:10] Riprendiamo.".into(), 1));
    }

    #[test]
    fn a_short_real_thanks_stays_and_loops_go() {
        let t = "[00:01:00] Domanda?\n\n[00:01:05] Grazie.\n\n[00:01:12] Allora proseguiamo.";
        assert_eq!(mark(t, Lang::It).1, 0);
        let t = "[00:01:00] Allora.\n\n[00:01:05] Sottotitoli a cura di QTSS\n\n[00:02:00] Va bene.";
        assert_eq!(mark(t, Lang::It).0, "[00:01:00] Allora.\n\n[00:01:05] *(pausa fino a 00:02:00)*\n\n[00:02:00] Va bene.");
        let t = "[00:00:00] Ok.\n\n[00:00:10] la diga la diga la diga la diga";
        assert_eq!(mark(t, Lang::It).0, "[00:00:00] Ok.\n\n[00:00:10] *(pausa fino alla fine)*");
        assert!(!invented("la diga a gravità è la diga più comune"));
    }

    #[test]
    fn english_pauses() {
        let t = "[00:01:00] So.\n\n[00:01:05] Thank you. Thank you. Thank you.\n\n[00:02:00] Right.";
        assert_eq!(mark(t, Lang::En).0, "[00:01:00] So.\n\n[00:01:05] *(pause until 00:02:00)*\n\n[00:02:00] Right.");
        assert!(is_pause("*(pause until 00:02:00)*") && is_pause("*(pausa fino a 00:02:00)*"));
        // An Italian pause in an English lesson (an old transcript) is still a pause.
        assert_eq!(mark("[00:01:00] So.\n\n[00:01:05] *(pausa fino a 00:02:00)*\n\n[00:02:00] Right.", Lang::En).1, 0);
    }
}
