//! Heuristic trait extraction from free-text.
//!
//! AniList returns factual fields (name, gender, age, series, genres,
//! popularity, image) but *not* structured `hair_color` / `eye_color` /
//! `archetype`. Those are inferred here by keyword-spotting the character's
//! description — best-effort enrichment, the same rule-based spirit as the
//! natural-language [`crate::query`] parser. Curated seed data overrides this.

/// Title-cased canonical labels, matched case-insensitively as substrings.
const HAIR: &[(&str, &str)] = &[
    ("silver hair", "Silver"),
    ("white hair", "White"),
    ("blonde", "Blonde"),
    ("blond", "Blonde"),
    ("golden hair", "Blonde"),
    ("pink hair", "Pink"),
    ("purple hair", "Purple"),
    ("violet hair", "Purple"),
    ("blue hair", "Blue"),
    ("green hair", "Green"),
    ("red hair", "Red"),
    ("redhead", "Red"),
    ("crimson hair", "Red"),
    ("brown hair", "Brown"),
    ("brunette", "Brown"),
    ("grey hair", "Grey"),
    ("gray hair", "Grey"),
    ("black hair", "Black"),
    ("dark hair", "Black"),
];

const EYE: &[(&str, &str)] = &[
    ("amber eyes", "Amber"),
    ("golden eyes", "Gold"),
    ("gold eyes", "Gold"),
    ("aqua eyes", "Aqua"),
    ("lavender eyes", "Lavender"),
    ("purple eyes", "Purple"),
    ("violet eyes", "Purple"),
    ("pink eyes", "Pink"),
    ("blue eyes", "Blue"),
    ("green eyes", "Green"),
    ("red eyes", "Red"),
    ("crimson eyes", "Red"),
    ("brown eyes", "Brown"),
    ("grey eyes", "Grey"),
    ("gray eyes", "Grey"),
    ("black eyes", "Black"),
];

/// Archetype keywords, ordered most-specific first.
const ARCHETYPE: &[(&str, &str)] = &[
    ("tsundere", "Tsundere"),
    ("yandere", "Yandere"),
    ("kuudere", "Kuudere"),
    ("dandere", "Dandere"),
    ("deredere", "Deredere"),
    ("mastermind", "Mastermind"),
    ("strategist", "Mastermind"),
    ("manipulative", "Mastermind"),
    ("genius", "Mastermind"),
    ("antagonist", "Villain"),
    ("villain", "Villain"),
    ("hot-blooded", "Hot-Blooded"),
    ("hot blooded", "Hot-Blooded"),
    ("energetic", "Genki"),
    ("cheerful", "Genki"),
    ("upbeat", "Genki"),
    ("stoic", "Stoic"),
    ("calm and collected", "Stoic"),
    ("reserved", "Stoic"),
    ("aloof", "Kuudere"),
    ("shy", "Dandere"),
    ("timid", "Dandere"),
    ("kind-hearted", "Deredere"),
    ("cheerful and kind", "Deredere"),
];

fn pick(text: &str, table: &[(&str, &str)]) -> Option<String> {
    table
        .iter()
        .find(|(kw, _)| text.contains(kw))
        .map(|(_, v)| v.to_string())
}

pub fn hair_color(description: &str) -> Option<String> {
    pick(&description.to_lowercase(), HAIR)
}

pub fn eye_color(description: &str) -> Option<String> {
    pick(&description.to_lowercase(), EYE)
}

pub fn archetype(description: &str) -> Option<String> {
    pick(&description.to_lowercase(), ARCHETYPE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_visible_traits() {
        let d = "A fiery tsundere with long red hair and bright blue eyes.";
        assert_eq!(archetype(d).as_deref(), Some("Tsundere"));
        assert_eq!(hair_color(d).as_deref(), Some("Red"));
        assert_eq!(eye_color(d).as_deref(), Some("Blue"));
    }

    #[test]
    fn missing_traits_are_none() {
        let d = "A mysterious figure of unknown origin.";
        assert!(hair_color(d).is_none());
        assert!(eye_color(d).is_none());
        assert!(archetype(d).is_none());
    }
}
