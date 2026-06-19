//! Character data sources.
//!
//! * [`SeedStore`] — the bundled, curated dataset (`data/seeds.json`), compiled
//!   into the binary so the tool works fully offline with zero setup.
//! * [`AniListClient`] — the live, keyless AniList GraphQL API
//!   (`https://graphql.anilist.co`). Factual fields come straight from AniList;
//!   hair/eye/archetype are enriched from the description via [`crate::extract`].

use crate::extract;
use crate::models::Character;
use anyhow::{anyhow, Context, Result};
use serde::Deserialize;

// ── Seed dataset ────────────────────────────────────────────────────────────

/// The curated dataset, embedded at compile time.
const SEEDS_JSON: &str = include_str!("../data/seeds.json");

pub struct SeedStore {
    characters: Vec<Character>,
}

impl SeedStore {
    pub fn load() -> Result<Self> {
        let mut characters: Vec<Character> =
            serde_json::from_str(SEEDS_JSON).context("parsing bundled data/seeds.json")?;
        for c in &mut characters {
            if c.source.is_none() {
                c.source = Some("seed".to_string());
            }
        }
        Ok(SeedStore { characters })
    }

    pub fn all(&self) -> &[Character] {
        &self.characters
    }

    /// Case- and diacritic-insensitive lookup: exact match first, then a
    /// substring match (so "Asuka" finds "Asuka Langley Soryu").
    pub fn find(&self, name: &str) -> Option<Character> {
        let q = normalize(name);
        self.characters
            .iter()
            .find(|c| normalize(&c.name) == q)
            .or_else(|| {
                self.characters
                    .iter()
                    .find(|c| normalize(&c.name).contains(&q))
            })
            .cloned()
    }
}

/// Lowercases and folds common Japanese long-vowel diacritics so ASCII queries
/// match macron'd names ("Soryu"/"Sōryū", "Tohsaka"/"Tōsaka").
fn normalize(s: &str) -> String {
    let mut t = s.to_lowercase();
    for (from, to) in [
        ("ō", "o"),
        ("ū", "u"),
        ("ā", "a"),
        ("ē", "e"),
        ("ī", "i"),
        ("ô", "o"),
    ] {
        t = t.replace(from, to);
    }
    t
}

// ── AniList GraphQL client ──────────────────────────────────────────────────

const ANILIST_URL: &str = "https://graphql.anilist.co";

const CHARACTER_QUERY: &str = r#"
query ($search: String) {
  Character(search: $search) {
    id
    name { full }
    gender
    age
    description(asHtml: false)
    favourites
    image { large }
    media(perPage: 1, sort: POPULARITY_DESC) {
      nodes { title { romaji } genres }
    }
  }
}"#;

pub struct AniListClient {
    http: reqwest::blocking::Client,
}

impl Default for AniListClient {
    fn default() -> Self {
        Self::new()
    }
}

impl AniListClient {
    pub fn new() -> Self {
        let http = reqwest::blocking::Client::builder()
            .user_agent("sauce-cli (https://github.com/TcgVanguardTroll/Sauce)")
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .expect("failed to build HTTP client");
        AniListClient { http }
    }

    /// Fetches a single character by name and maps it into our model.
    pub fn fetch(&self, name: &str) -> Result<Character> {
        let body = serde_json::json!({
            "query": CHARACTER_QUERY,
            "variables": { "search": name },
        });
        let resp: GqlResponse = self
            .http
            .post(ANILIST_URL)
            .json(&body)
            .send()
            .context("contacting AniList")?
            .error_for_status()
            .context("AniList returned an error status")?
            .json()
            .context("decoding AniList response")?;

        let node = resp
            .data
            .and_then(|d| d.character)
            .ok_or_else(|| anyhow!("no AniList character found for {:?}", name))?;
        Ok(node.into_character())
    }
}

// ── AniList response shapes ─────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct GqlResponse {
    data: Option<GqlData>,
}

#[derive(Debug, Deserialize)]
struct GqlData {
    #[serde(rename = "Character")]
    character: Option<CharacterNode>,
}

#[derive(Debug, Deserialize)]
struct CharacterNode {
    id: i64,
    name: NameNode,
    gender: Option<String>,
    age: Option<String>,
    description: Option<String>,
    favourites: Option<i64>,
    image: Option<ImageNode>,
    media: Option<MediaConnection>,
}

#[derive(Debug, Deserialize)]
struct NameNode {
    full: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ImageNode {
    large: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MediaConnection {
    nodes: Vec<MediaNode>,
}

#[derive(Debug, Deserialize)]
struct MediaNode {
    title: Option<TitleNode>,
    #[serde(default)]
    genres: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct TitleNode {
    romaji: Option<String>,
}

impl CharacterNode {
    fn into_character(self) -> Character {
        let name = self.name.full.unwrap_or_else(|| "Unknown".to_string());
        let mut c = Character::new(name);
        c.anilist_id = Some(self.id);
        c.source = Some("anilist".to_string());
        c.source_url = Some(format!("https://anilist.co/character/{}", self.id));
        c.gender = self.gender;
        c.age = self.age.as_deref().and_then(parse_leading_age);
        c.popularity = self.favourites;
        c.image_url = self.image.and_then(|i| i.large);

        if let Some(media) = self.media {
            if let Some(first) = media.nodes.into_iter().next() {
                c.series = first.title.and_then(|t| t.romaji);
                c.genres = first.genres;
            }
        }

        if let Some(desc) = self.description {
            c.hair_color = extract::hair_color(&desc);
            c.eye_color = extract::eye_color(&desc);
            c.archetype = extract::archetype(&desc).unwrap_or_default();
            c.description = Some(truncate(&desc, 280));
        }
        // Archetype is required downstream; default the unknown to a neutral
        // catch-all so the record is still usable in attribute search.
        if c.archetype.is_empty() {
            c.archetype = "Deredere".to_string();
        }
        c
    }
}

/// AniList ages are free-text ("17", "17-18", "Unknown"); take the leading int.
fn parse_leading_age(s: &str) -> Option<u32> {
    let digits: String = s
        .trim()
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok().filter(|n| *n > 0 && *n < 200)
}

fn truncate(s: &str, max: usize) -> String {
    let cleaned = s.replace("<br>", " ").replace('\n', " ");
    if cleaned.chars().count() <= max {
        cleaned
    } else {
        let truncated: String = cleaned.chars().take(max).collect();
        format!("{}…", truncated.trim_end())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_dataset_loads_and_is_valid() {
        let store = SeedStore::load().unwrap();
        assert!(store.all().len() >= 20, "expected a decent seed roster");
        for c in store.all() {
            assert!(!c.name.is_empty());
            assert!(!c.archetype.is_empty(), "{} has no archetype", c.name);
            assert_eq!(c.source.as_deref(), Some("seed"));
        }
    }

    #[test]
    fn seed_lookup_is_case_insensitive() {
        let store = SeedStore::load().unwrap();
        assert!(store.find("taiga aisaka").is_some());
    }

    #[test]
    fn parses_leading_age() {
        assert_eq!(parse_leading_age("17"), Some(17));
        assert_eq!(parse_leading_age("17-18"), Some(17));
        assert_eq!(parse_leading_age("Unknown"), None);
    }
}
