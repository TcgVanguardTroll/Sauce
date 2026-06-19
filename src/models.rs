use serde::{Deserialize, Serialize};

/// A single anime character — the unit the recommendation engine reasons over.
///
/// Mirrors the shape of a row in the `characters` table. Fields that the data
/// sources don't always provide are `Option`; `archetype` is required because it
/// is the primary split of the preference tree and the hard gate for
/// recommendations (see [`crate::recommender`]).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Character {
    pub id: Option<i64>,
    pub name: String,
    /// Primary personality archetype — `Tsundere`, `Kuudere`, `Yandere`,
    /// `Dandere`, `Deredere`, `Genki`, `Hot-Blooded`, `Stoic`, `Mastermind`,
    /// `Villain`. This is the hard gate for recommendations.
    pub archetype: String,
    /// Narrative role — `Protagonist`, `Antagonist`, `Supporting`, `Rival`.
    pub role: Option<String>,
    pub hair_color: Option<String>,
    pub eye_color: Option<String>,
    pub gender: Option<String>,
    pub age: Option<u32>,
    /// Series the character is most associated with.
    pub series: Option<String>,
    /// Genres/tags of that series — used for "vibe" overlap scoring.
    pub genres: Vec<String>,
    /// AniList "favourites" count — a popularity signal.
    pub popularity: Option<i64>,
    pub image_url: Option<String>,
    pub description: Option<String>,
    // Provenance
    pub source: Option<String>,
    pub source_url: Option<String>,
    pub anilist_id: Option<i64>,
    // Computed at query time, never persisted.
    #[serde(skip)]
    pub match_score: f64,
}

impl Character {
    pub fn new(name: impl Into<String>) -> Self {
        Character {
            id: None,
            name: name.into(),
            archetype: String::new(),
            role: None,
            hair_color: None,
            eye_color: None,
            gender: None,
            age: None,
            series: None,
            genres: Vec::new(),
            popularity: None,
            image_url: None,
            description: None,
            source: None,
            source_url: None,
            anilist_id: None,
            match_score: 0.0,
        }
    }

    /// A compact one-line trait summary used in listings.
    pub fn summary(&self) -> String {
        let mut parts: Vec<String> = vec![self.archetype.clone()];
        if let Some(r) = &self.role {
            parts.push(r.clone());
        }
        if let Some(h) = &self.hair_color {
            parts.push(format!("{} hair", h));
        }
        if let Some(e) = &self.eye_color {
            parts.push(format!("{} eyes", e));
        }
        if let Some(s) = &self.series {
            parts.push(s.clone());
        }
        parts.retain(|p| !p.is_empty());
        parts.join(", ")
    }
}

/// Flat counts produced by [`crate::recommender::analyze_preferences`].
#[derive(Debug, Default)]
pub struct PreferenceAnalysis {
    pub common_archetypes: Vec<(String, usize)>,
    pub common_roles: Vec<(String, usize)>,
    pub common_hair_colors: Vec<(String, usize)>,
    pub common_genres: Vec<(String, usize)>,
    pub age_range: (u32, u32),
    pub average_age: f64,
}
