//! Plain-English query parsing → the structured inputs `find` already takes.
//!
//! Local and rule-based (no LLM, keeping the engine self-contained): keyword-spot
//! the appearance/archetype attributes, and split `"<trait> like <name>"` clauses
//! into look references ("looks like X") vs vibe references ("vibe like Y",
//! "acts like Z"). Handles shapes like "blue-eyed tsunderes that look like Taiga
//! Aisaka", "pink-haired girls with the vibe of Yuno Gasai", and the
//! two-reference form "… look like X with the vibe of Y". Names are resolved
//! against the library later by `find`.

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ParsedQuery {
    /// Visual references ("looks like X").
    pub looks_like: Vec<String>,
    /// Personality references ("vibe like Y", "acts like Z").
    pub vibe_like: Vec<String>,
    pub hair: Option<String>,
    pub eye: Option<String>,
    pub archetype: Option<String>,
}

// Filler words skipped when finding a clause's qualifier, and dropped from the
// attribute head. Break words also terminate a reference name.
const STOP: &[&str] = &[
    "a", "an", "the", "with", "of", "her", "his", "is", "are", "that",
];
const LOOK_QUAL: &[&str] = &[
    "look",
    "looks",
    "looking",
    "face",
    "resembles",
    "resembling",
];
const VIBE_QUAL: &[&str] = &[
    "vibe",
    "vibes",
    "personality",
    "energy",
    "acts",
    "attitude",
    "aura",
    "feel",
    "feels",
];
const BREAKS: &[&str] = &["with", "and", "that", "who", "but", "plus"];

const EYE: &[(&str, &str)] = &[
    ("blue eye", "Blue"),
    ("green eye", "Green"),
    ("red eye", "Red"),
    ("gold eye", "Gold"),
    ("amber eye", "Amber"),
    ("purple eye", "Purple"),
    ("lavender eye", "Lavender"),
    ("aqua eye", "Aqua"),
    ("grey eye", "Grey"),
    ("gray eye", "Grey"),
    ("brown eye", "Brown"),
];
const HAIR: &[(&str, &str)] = &[
    ("blonde", "Blonde"),
    ("blond", "Blonde"),
    ("pink hair", "Pink"),
    ("pink-hair", "Pink"),
    ("blue hair", "Blue"),
    ("blue-hair", "Blue"),
    ("purple hair", "Purple"),
    ("silver hair", "Silver"),
    ("white hair", "White"),
    ("green hair", "Green"),
    ("redhead", "Red"),
    ("red hair", "Red"),
    ("brunette", "Brown"),
    ("brown hair", "Brown"),
    ("black hair", "Black"),
];
const ARCHETYPE: &[(&str, &str)] = &[
    ("tsundere", "Tsundere"),
    ("yandere", "Yandere"),
    ("kuudere", "Kuudere"),
    ("dandere", "Dandere"),
    ("deredere", "Deredere"),
    ("genki", "Genki"),
    ("hot-blooded", "Hot-Blooded"),
    ("hot blooded", "Hot-Blooded"),
    ("stoic", "Stoic"),
    ("mastermind", "Mastermind"),
    ("villain", "Villain"),
];

pub fn parse(text: &str) -> ParsedQuery {
    let lower = text.to_lowercase();
    let words: Vec<&str> = lower.split_whitespace().collect();
    let mut q = ParsedQuery::default();

    // ── Reference clauses ──
    // A clause is a connector ("like", or a qualifying "of" as in "vibe of X")
    // followed by a name, classified look-vs-vibe by the nearest preceding
    // non-filler qualifier.
    let mut i = 0;
    let mut first_clause: Option<usize> = None;
    while i + 1 < words.len() {
        let qual = words[..i].iter().rev().find(|w| !STOP.contains(w)).copied();
        let is_vibe = qual.map(|q| VIBE_QUAL.contains(&q)).unwrap_or(false);
        let is_look = qual.map(|q| LOOK_QUAL.contains(&q)).unwrap_or(false);
        let is_clause = match words[i] {
            "like" => true,             // "like X" — defaults to a look reference
            "of" => is_vibe || is_look, // only a clause after a vibe/look qualifier
            _ => false,
        };
        if !is_clause {
            i += 1;
            continue;
        }
        first_clause.get_or_insert(i);
        let start = i + 1;
        let mut j = start;
        while j < words.len() && !BREAKS.contains(&words[j]) {
            j += 1;
        }
        let name = titlecase(&words[start..j].join(" "));
        if !name.is_empty() {
            if is_vibe {
                q.vibe_like.push(name);
            } else {
                q.looks_like.push(name);
            }
        }
        i = j;
    }

    // ── Attribute head: everything before the first clause's qualifier ──
    let head_words: &[&str] = match first_clause {
        Some(ci) => {
            let mut end = ci;
            while end > 0
                && (STOP.contains(&words[end - 1])
                    || LOOK_QUAL.contains(&words[end - 1])
                    || VIBE_QUAL.contains(&words[end - 1]))
            {
                end -= 1;
            }
            &words[..end]
        }
        None => &words[..],
    };
    let head = head_words.join(" ");
    q.eye = pick(&head, EYE);
    q.hair = pick(&head, HAIR);
    q.archetype = pick(&head, ARCHETYPE);
    q
}

fn pick(head: &str, table: &[(&str, &str)]) -> Option<String> {
    table
        .iter()
        .find(|(kw, _)| head.contains(kw))
        .map(|(_, v)| v.to_string())
}

fn titlecase(s: &str) -> String {
    s.split_whitespace()
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eyed_tsunderes_who_look_like() {
        let q = parse("blue eyed tsunderes that look like Taiga Aisaka");
        assert_eq!(q.eye.as_deref(), Some("Blue"));
        assert_eq!(q.archetype.as_deref(), Some("Tsundere"));
        assert_eq!(q.looks_like, vec!["Taiga Aisaka".to_string()]);
        assert!(q.vibe_like.is_empty());
    }

    #[test]
    fn pink_haired_with_vibe_of() {
        let q = parse("pink haired girls with the vibe of Yuno Gasai");
        assert_eq!(q.hair.as_deref(), Some("Pink"));
        assert_eq!(q.vibe_like, vec!["Yuno Gasai".to_string()]);
        assert!(q.looks_like.is_empty());
    }

    #[test]
    fn two_references_look_and_vibe() {
        let q = parse("characters that look like Rin Tohsaka with the vibe of Levi Ackerman");
        assert_eq!(q.looks_like, vec!["Rin Tohsaka".to_string()]);
        assert_eq!(q.vibe_like, vec!["Levi Ackerman".to_string()]);
    }

    #[test]
    fn archetype_only() {
        let q = parse("recommend me a stoic mastermind");
        // first match wins per table order — both are present in the head
        assert!(q.archetype.is_some());
        assert!(q.looks_like.is_empty() && q.vibe_like.is_empty());
    }
}
