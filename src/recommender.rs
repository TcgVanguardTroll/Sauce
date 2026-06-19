//! The recommendation engine.
//!
//! Four complementary algorithms, all operating purely over in-memory
//! [`Character`] slices (no I/O, fully unit-testable):
//!
//! 1. **Preference tree** — a decision tree over `archetype → hair → eye → age →
//!    role`, with counts and percentages at every branch.
//! 2. **IDF-weighted scoring** — borrows TF-IDF from search: attributes that
//!    *all* your favourites share are uninformative and down-weighted; rare,
//!    distinctive traits are up-weighted. Archetype is a hard gate.
//! 3. **k-NN feature vectors** — each character is encoded as a weighted numeric
//!    "vibe" vector; Euclidean distance ranks similarity.
//! 4. **k-means clustering** — finds the *multiple* types in your library.

use crate::models::{Character, PreferenceAnalysis};
use std::collections::HashMap;

// ── Preference tree ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PreferenceNode {
    pub label: String,
    pub attribute: String,
    pub count: usize,
    pub parent_count: usize,
    pub children: Vec<PreferenceNode>,
}

impl PreferenceNode {
    pub fn pct(&self) -> f64 {
        if self.parent_count == 0 {
            0.0
        } else {
            self.count as f64 / self.parent_count as f64 * 100.0
        }
    }
}

/// Buckets a numeric age into a coarse band. Anime skews young, so the lower
/// bands are finer-grained than the upper ones.
pub fn age_bucket(age: u32) -> &'static str {
    match age {
        0..=14 => "0-14",
        15..=17 => "15-17",
        18..=24 => "18-24",
        25..=34 => "25-34",
        _ => "35+",
    }
}

/// The tree drills, in order: archetype → hair → eye → age → role.
fn attribute_label(c: &Character, depth: usize) -> String {
    match depth {
        0 => c.archetype.clone(),
        1 => c.hair_color.as_deref().unwrap_or("Unknown").to_string(),
        2 => c.eye_color.as_deref().unwrap_or("Unknown").to_string(),
        3 => c.age.map(age_bucket).unwrap_or("Unknown").to_string(),
        4 => c.role.as_deref().unwrap_or("Unknown").to_string(),
        _ => unreachable!(),
    }
}

fn attribute_name(depth: usize) -> &'static str {
    match depth {
        0 => "archetype",
        1 => "hair_color",
        2 => "eye_color",
        3 => "age_range",
        4 => "role",
        _ => "unknown",
    }
}

pub fn build_preference_tree(characters: &[Character]) -> Vec<PreferenceNode> {
    build_level(characters, characters.len(), 0)
}

fn build_level(characters: &[Character], parent_count: usize, depth: usize) -> Vec<PreferenceNode> {
    if depth >= 5 || characters.is_empty() {
        return vec![];
    }
    let mut groups = group_by_attribute(characters, depth);
    let mut nodes: Vec<PreferenceNode> = groups
        .drain()
        .map(|(label, group)| {
            let count = group.len();
            PreferenceNode {
                label,
                attribute: attribute_name(depth).to_string(),
                count,
                parent_count,
                children: build_level(&group, count, depth + 1),
            }
        })
        .collect();
    // Stable order: most common first, then alphabetical to break ties
    // deterministically (HashMap iteration order is otherwise random).
    nodes.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.label.cmp(&b.label)));
    nodes
}

fn group_by_attribute(characters: &[Character], depth: usize) -> HashMap<String, Vec<Character>> {
    let mut map: HashMap<String, Vec<Character>> = HashMap::new();
    for c in characters {
        map.entry(attribute_label(c, depth))
            .or_default()
            .push(c.clone());
    }
    map
}

pub fn print_tree(nodes: &[PreferenceNode], prefix: &str, total: usize) {
    for (i, node) in nodes.iter().enumerate() {
        let is_last = i == nodes.len() - 1;
        let connector = if is_last { "└──" } else { "├──" };
        let child_prefix = if is_last { "    " } else { "│   " };
        println!(
            "{}{} {} {}/{}  {:.0}%",
            prefix,
            connector,
            node.label,
            node.count,
            total,
            node.pct()
        );
        if !node.children.is_empty() {
            print_tree(
                &node.children,
                &format!("{}{}", prefix, child_prefix),
                node.count,
            );
        }
    }
}

/// Renders the tree as a Mermaid flowchart for docs / READMEs.
pub fn to_mermaid(nodes: &[PreferenceNode], total: usize) -> String {
    let mut out = String::from("```mermaid\nflowchart TD\n");
    out.push_str(&format!("  root[\"You · {} favourites\"]\n", total));
    let mut counter = 0usize;
    for n in nodes {
        emit_mermaid(n, "root", &mut counter, &mut out);
    }
    out.push_str("```\n");
    out
}

fn emit_mermaid(node: &PreferenceNode, parent: &str, counter: &mut usize, out: &mut String) {
    let id = format!("n{}", *counter);
    *counter += 1;
    let label = format!(
        "{} · {}/{} · {:.0}%",
        node.label.replace('"', ""),
        node.count,
        node.parent_count,
        node.pct()
    );
    out.push_str(&format!("  {}[\"{}\"]\n", id, label));
    out.push_str(&format!("  {} --> {}\n", parent, id));
    for c in &node.children {
        emit_mermaid(c, &id, counter, out);
    }
}

/// The dominant path down the tree: the highest-count child at each level,
/// stopping when confidence drops below 50% (after the first hop).
pub fn dominant_query_path(nodes: &[PreferenceNode]) -> Vec<String> {
    let mut path = vec![];
    let mut current = nodes;
    while let Some(best) = current.first() {
        if best.pct() < 50.0 && !path.is_empty() {
            break;
        }
        path.push(best.label.clone());
        current = &best.children;
    }
    path
}

// ── IDF-weighted scoring ────────────────────────────────────────────────────
//
// idf(v) = ln(N / df(v)) + 1, where N = favourites and df(v) = favourites with
// that value. A trait every favourite shares earns idf = ln(1)+1 = 1.0 (no
// discriminating power); a rare trait earns a larger weight.

#[derive(Debug, Clone)]
pub struct IdfWeights {
    pub archetype: HashMap<String, f64>,
    pub hair_color: HashMap<String, f64>,
    pub eye_color: HashMap<String, f64>,
    pub age_bucket: HashMap<String, f64>,
    pub role: HashMap<String, f64>,
}

pub fn compute_idf_weights(characters: &[Character]) -> IdfWeights {
    let n = characters.len() as f64;
    let idf_map = |vals: Vec<Option<String>>| -> HashMap<String, f64> {
        let mut counts: HashMap<String, usize> = HashMap::new();
        for v in vals.into_iter().flatten() {
            *counts.entry(v).or_insert(0) += 1;
        }
        counts
            .into_iter()
            .map(|(k, df)| (k, (n / df as f64).ln() + 1.0))
            .collect()
    };

    IdfWeights {
        archetype: idf_map(
            characters
                .iter()
                .map(|c| Some(c.archetype.clone()))
                .collect(),
        ),
        hair_color: idf_map(characters.iter().map(|c| c.hair_color.clone()).collect()),
        eye_color: idf_map(characters.iter().map(|c| c.eye_color.clone()).collect()),
        age_bucket: idf_map(
            characters
                .iter()
                .map(|c| c.age.map(|a| age_bucket(a).to_string()))
                .collect(),
        ),
        role: idf_map(characters.iter().map(|c| c.role.clone()).collect()),
    }
}

/// Scores a candidate against the preference tree using IDF weights. Archetype
/// is a hard gate: a candidate whose archetype isn't in your tree scores 0.
pub fn score_character_idf(c: &Character, tree: &[PreferenceNode], idf: &IdfWeights) -> f64 {
    let Some(arch_node) = tree.iter().find(|n| n.label == c.archetype) else {
        return 0.0;
    };

    let arch_idf = idf.archetype.get(&c.archetype).copied().unwrap_or(1.0);
    let mut score = arch_node.pct() / 100.0 * arch_idf * 3.0;

    let hair = c.hair_color.as_deref().unwrap_or("Unknown");
    if let Some(hair_node) = arch_node.children.iter().find(|n| n.label == hair) {
        let hair_idf = idf.hair_color.get(hair).copied().unwrap_or(1.0);
        score += hair_node.pct() / 100.0 * hair_idf * 2.0;

        // Age sits two levels under hair (hair → eye → age).
        if let Some(age) = c.age {
            let bucket = age_bucket(age);
            let age_idf = idf.age_bucket.get(bucket).copied().unwrap_or(1.0);
            if let Some(age_node) = hair_node
                .children
                .iter()
                .flat_map(|e| e.children.iter())
                .find(|n| n.label == bucket)
            {
                score += age_node.pct() / 100.0 * age_idf * 1.5;
            }
        }

        let eye = c.eye_color.as_deref().unwrap_or("Unknown");
        if let Some(eye_node) = hair_node.children.iter().find(|n| n.label == eye) {
            let eye_idf = idf.eye_color.get(eye).copied().unwrap_or(1.0);
            score += eye_node.pct() / 100.0 * eye_idf * 0.5;

            // Role sits two levels under eye (eye → age → role).
            let role = c.role.as_deref().unwrap_or("Unknown");
            let role_idf = idf.role.get(role).copied().unwrap_or(1.0);
            if let Some(role_node) = eye_node
                .children
                .iter()
                .flat_map(|n| n.children.iter())
                .find(|n| n.label == role)
            {
                score += role_node.pct() / 100.0 * role_idf * 0.3;
            }
        }
    }

    score
}

// ── k-NN "vibe" feature vectors ─────────────────────────────────────────────
//
// Each character → a normalised, weighted numeric vector. Archetype and role
// dominate; gender, age and popularity are finer signals; hair/eye are light
// appearance hints. Euclidean distance = taste similarity.

const ARCHETYPES: &[&str] = &[
    "Dandere",
    "Deredere",
    "Genki",
    "Hot-Blooded",
    "Kuudere",
    "Mastermind",
    "Stoic",
    "Tsundere",
    "Villain",
    "Yandere",
];
const ROLES: &[&str] = &["Antagonist", "Protagonist", "Rival", "Supporting"];
const HAIRS: &[&str] = &[
    "Black", "Blonde", "Blue", "Brown", "Green", "Grey", "Pink", "Purple", "Red", "Silver", "White",
];
const EYES: &[&str] = &[
    "Amber", "Aqua", "Black", "Blue", "Brown", "Gold", "Green", "Grey", "Lavender", "Pink",
    "Purple", "Red",
];
const GENDERS: &[&str] = &["Female", "Male", "Non-binary"];

fn str_to_id(val: Option<&str>, options: &[&str]) -> f64 {
    let v = val.unwrap_or("Unknown");
    options
        .iter()
        .position(|&o| o.eq_ignore_ascii_case(v))
        .map(|i| i as f64 / (options.len() - 1).max(1) as f64)
        .unwrap_or(0.5)
}

fn archetype_id(c: &Character) -> f64 {
    str_to_id(Some(&c.archetype), ARCHETYPES)
}

fn age_f64(c: &Character) -> f64 {
    // Normalise over a plausible 10–60 anime span; neutral 0.5 when unknown.
    c.age
        .map(|a| ((a as f64 - 10.0) / 50.0).clamp(0.0, 1.0))
        .unwrap_or(0.5)
}

/// Log-normalised popularity (0–1); neutral 0.5 when unknown. Log because
/// favourites counts are heavy-tailed (a handful of characters dwarf the rest).
fn popularity_f64(c: &Character) -> f64 {
    c.popularity
        .map(|p| {
            let l = (p.max(0) as f64 + 1.0).ln();
            (l / 12.0).clamp(0.0, 1.0) // ln(~160k) ≈ 12
        })
        .unwrap_or(0.5)
}

/// Weighted feature vector for k-means clustering. Always returns a vector
/// (neutral defaults for missing data) so every character can be clustered.
pub fn cluster_vector(c: &Character) -> Vec<f32> {
    vec![
        (archetype_id(c) * 3.0) as f32, // primary split
        (str_to_id(c.role.as_deref(), ROLES) * 2.0) as f32,
        (str_to_id(c.gender.as_deref(), GENDERS) * 1.5) as f32,
        (age_f64(c) * 1.0) as f32,
        (popularity_f64(c) * 1.0) as f32,
        (str_to_id(c.hair_color.as_deref(), HAIRS) * 0.5) as f32,
        (str_to_id(c.eye_color.as_deref(), EYES) * 0.3) as f32,
    ]
}

fn dist2(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| (x - y).powi(2)).sum()
}

/// k-means clustering with deterministic farthest-point init (reproducible, no
/// RNG). Returns a cluster id per input point.
pub fn kmeans(points: &[Vec<f32>], k: usize) -> Vec<usize> {
    let n = points.len();
    if n == 0 || k == 0 {
        return vec![0; n];
    }
    let k = k.min(n);
    let dims = points[0].len();

    let mut centroids: Vec<Vec<f32>> = vec![points[0].clone()];
    while centroids.len() < k {
        let mut best = 0usize;
        let mut best_d = -1.0_f32;
        for (i, p) in points.iter().enumerate() {
            let d = centroids
                .iter()
                .map(|c| dist2(p, c))
                .fold(f32::MAX, f32::min);
            if d > best_d {
                best_d = d;
                best = i;
            }
        }
        centroids.push(points[best].clone());
    }

    let mut assign = vec![0usize; n];
    for _ in 0..50 {
        let mut changed = false;
        for (i, p) in points.iter().enumerate() {
            let mut bj = 0usize;
            let mut bd = f32::MAX;
            for (j, c) in centroids.iter().enumerate() {
                let d = dist2(p, c);
                if d < bd {
                    bd = d;
                    bj = j;
                }
            }
            if assign[i] != bj {
                assign[i] = bj;
                changed = true;
            }
        }
        let mut sums = vec![vec![0.0_f32; dims]; k];
        let mut counts = vec![0usize; k];
        for (i, p) in points.iter().enumerate() {
            counts[assign[i]] += 1;
            for d in 0..dims {
                sums[assign[i]][d] += p[d];
            }
        }
        for j in 0..k {
            if counts[j] > 0 {
                for d in 0..dims {
                    centroids[j][d] = sums[j][d] / counts[j] as f32;
                }
            }
        }
        if !changed {
            break;
        }
    }
    assign
}

#[derive(Debug, Clone)]
pub struct FeatureVec {
    pub name: String,
    pub values: Vec<f64>,
}

/// Encodes a character as a normalised k-NN feature vector ("vibe" space).
pub fn feature_vector(c: &Character) -> FeatureVec {
    FeatureVec {
        name: c.name.clone(),
        values: vec![
            archetype_id(c) * 3.0,                           // archetype × 3
            str_to_id(c.role.as_deref(), ROLES) * 2.0,       // role × 2
            str_to_id(c.gender.as_deref(), GENDERS) * 1.5,   // gender × 1.5
            age_f64(c) * 1.0,                                // age
            popularity_f64(c) * 1.0,                         // popularity
            str_to_id(c.hair_color.as_deref(), HAIRS) * 0.5, // hair (light)
            str_to_id(c.eye_color.as_deref(), EYES) * 0.3,   // eye (light)
        ],
    }
}

impl FeatureVec {
    pub fn distance(&self, other: &FeatureVec) -> f64 {
        self.values
            .iter()
            .zip(other.values.iter())
            .map(|(a, b)| (a - b).powi(2))
            .sum::<f64>()
            .sqrt()
    }

    /// Max possible distance given the weight vector (order matches
    /// [`feature_vector`]); used to turn a distance into a 0–100% similarity.
    fn max_distance() -> f64 {
        let max_vals = [3.0_f64, 2.0, 1.5, 1.0, 1.0, 0.5, 0.3];
        max_vals.iter().map(|v| v.powi(2)).sum::<f64>().sqrt()
    }

    /// Averages several vectors into one ("vibe like the blend of these").
    pub fn average(vecs: &[FeatureVec]) -> Option<FeatureVec> {
        let first = vecs.first()?;
        let dims = first.values.len();
        let mut sum = vec![0.0_f64; dims];
        let mut count = 0usize;
        for v in vecs {
            if v.values.len() != dims {
                continue;
            }
            for (s, x) in sum.iter_mut().zip(v.values.iter()) {
                *s += *x;
            }
            count += 1;
        }
        if count == 0 {
            return None;
        }
        for s in sum.iter_mut() {
            *s /= count as f64;
        }
        Some(FeatureVec {
            name: "<blend>".to_string(),
            values: sum,
        })
    }

    pub fn similarity_pct(&self, other: &FeatureVec) -> f64 {
        let d = self.distance(other);
        let sim = 1.0 - (d / Self::max_distance()).clamp(0.0, 1.0);
        (sim * 100.0).round()
    }
}

// ── Reference similarity ("vibe like X") ────────────────────────────────────
//
// Compares a candidate to a single reference, dimension by dimension. A
// dimension only contributes to the denominator when the *reference* has that
// data, so missing fields never penalise a match and an identical character
// scores a clean 100%.

pub fn score_against(candidate: &Character, reference: &Character) -> f64 {
    let mut score = 0.0;
    let mut max = 0.0;

    // Archetype — always present, dominant weight.
    max += 5.0;
    if candidate.archetype == reference.archetype {
        score += 5.0;
    }

    if reference.role.is_some() {
        max += 3.0;
        if candidate.role == reference.role {
            score += 3.0;
        }
    }

    // Genre overlap (Jaccard) — shared themes/setting.
    if !reference.genres.is_empty() {
        max += 2.0;
        score += genre_overlap(&reference.genres, &candidate.genres) * 2.0;
    }

    if reference.gender.is_some() {
        max += 1.0;
        if candidate.gender == reference.gender {
            score += 1.0;
        }
    }

    // Age band — a soft signal in anime (a show's cast spans a wide range).
    if reference.age.is_some() {
        max += 1.0;
        if let (Some(ca), Some(ra)) = (candidate.age, reference.age) {
            if age_bucket(ca) == age_bucket(ra) {
                score += 1.0;
            }
        }
    }

    if reference.hair_color.is_some() {
        max += 1.0;
        if candidate.hair_color == reference.hair_color {
            score += 1.0;
        }
    }

    if reference.eye_color.is_some() {
        max += 0.5;
        if candidate.eye_color == reference.eye_color {
            score += 0.5;
        }
    }

    // Same series → strong relatedness bonus (fans of one cast member tend to
    // like another), weighted above incidental demographic overlap.
    if reference.series.is_some() {
        max += 2.0;
        if candidate.series == reference.series {
            score += 2.0;
        }
    }

    if max == 0.0 {
        return 0.0;
    }
    (score / max * 100.0_f64).round()
}

/// Jaccard similarity (0–1) between two genre sets (case-insensitive).
pub fn genre_overlap(a: &[String], b: &[String]) -> f64 {
    let norm = |g: &[String]| -> Vec<String> {
        g.iter()
            .map(|x| x.trim().to_lowercase())
            .filter(|x| !x.is_empty())
            .collect()
    };
    let sa = norm(a);
    let sb = norm(b);
    if sa.is_empty() || sb.is_empty() {
        return 0.0;
    }
    let intersection = sa.iter().filter(|x| sb.contains(x)).count();
    let union = sa.len() + sb.len() - intersection;
    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

/// True if a character's series carries a genre containing the keyword.
pub fn has_genre(c: &Character, keyword: &str) -> bool {
    let kw = keyword.to_lowercase();
    c.genres.iter().any(|g| g.to_lowercase().contains(&kw))
}

// ── Visual embedding similarity ─────────────────────────────────────────────

/// Cosine similarity between two embedding vectors (0–1 for the non-negative
/// region; clamped). Used to rank `find --looks-like` by character art when
/// embeddings have been generated (see [`crate::embedder`]).
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f64 = a.iter().zip(b).map(|(x, y)| *x as f64 * *y as f64).sum();
    let na: f64 = a.iter().map(|x| (*x as f64).powi(2)).sum::<f64>().sqrt();
    let nb: f64 = b.iter().map(|x| (*x as f64).powi(2)).sum::<f64>().sqrt();
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        (dot / (na * nb)).clamp(-1.0, 1.0)
    }
}

// ── Flat analysis ───────────────────────────────────────────────────────────

pub fn analyze_preferences(characters: &[Character]) -> PreferenceAnalysis {
    let mut archetypes: HashMap<String, usize> = HashMap::new();
    let mut roles: HashMap<String, usize> = HashMap::new();
    let mut hair_colors: HashMap<String, usize> = HashMap::new();
    let mut genres: HashMap<String, usize> = HashMap::new();
    let mut ages: Vec<u32> = Vec::new();
    for c in characters {
        if !c.archetype.is_empty() {
            *archetypes.entry(c.archetype.clone()).or_insert(0) += 1;
        }
        if let Some(r) = &c.role {
            *roles.entry(r.clone()).or_insert(0) += 1;
        }
        if let Some(h) = &c.hair_color {
            *hair_colors.entry(h.clone()).or_insert(0) += 1;
        }
        for g in &c.genres {
            *genres.entry(g.clone()).or_insert(0) += 1;
        }
        if let Some(a) = c.age {
            ages.push(a);
        }
    }
    let sort_map = |map: HashMap<String, usize>| -> Vec<(String, usize)> {
        let mut v: Vec<_> = map.into_iter().collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        v
    };
    let age_range = if ages.is_empty() {
        (0, 0)
    } else {
        (*ages.iter().min().unwrap(), *ages.iter().max().unwrap())
    };
    let average_age = if ages.is_empty() {
        0.0
    } else {
        ages.iter().sum::<u32>() as f64 / ages.len() as f64
    };
    PreferenceAnalysis {
        common_archetypes: sort_map(archetypes),
        common_roles: sort_map(roles),
        common_hair_colors: sort_map(hair_colors),
        common_genres: sort_map(genres),
        age_range,
        average_age,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::too_many_arguments)]
    fn ch(
        name: &str,
        archetype: &str,
        role: &str,
        hair: &str,
        eye: &str,
        age: u32,
        series: &str,
        genres: &[&str],
    ) -> Character {
        let mut c = Character::new(name);
        c.archetype = archetype.to_string();
        c.role = Some(role.to_string());
        c.hair_color = Some(hair.to_string());
        c.eye_color = Some(eye.to_string());
        c.age = Some(age);
        c.series = Some(series.to_string());
        c.genres = genres.iter().map(|g| g.to_string()).collect();
        c
    }

    #[test]
    fn age_buckets() {
        assert_eq!(age_bucket(12), "0-14");
        assert_eq!(age_bucket(16), "15-17");
        assert_eq!(age_bucket(20), "18-24");
        assert_eq!(age_bucket(30), "25-34");
        assert_eq!(age_bucket(40), "35+");
    }

    #[test]
    fn idf_downweights_universal_attributes() {
        let people = vec![
            ch(
                "a",
                "Tsundere",
                "Protagonist",
                "Red",
                "Blue",
                17,
                "X",
                &["Romance"],
            ),
            ch(
                "b",
                "Tsundere",
                "Supporting",
                "Black",
                "Aqua",
                17,
                "Y",
                &["Action"],
            ),
            ch(
                "c",
                "Tsundere",
                "Protagonist",
                "Brown",
                "Brown",
                18,
                "Z",
                &["Comedy"],
            ),
        ];
        let idf = compute_idf_weights(&people);
        // Tsundere in all 3 → idf = ln(3/3)+1 = 1.0
        assert!((idf.archetype["Tsundere"] - 1.0).abs() < 1e-9);
        // Supporting in 1/3 → higher idf than Protagonist (2/3)
        assert!(idf.role["Supporting"] > idf.role["Protagonist"]);
    }

    #[test]
    fn preference_tree_dominant_path() {
        let people = vec![
            ch(
                "a",
                "Tsundere",
                "Protagonist",
                "Red",
                "Blue",
                17,
                "X",
                &["Romance"],
            ),
            ch(
                "b",
                "Tsundere",
                "Supporting",
                "Red",
                "Blue",
                16,
                "Y",
                &["Action"],
            ),
            ch(
                "c",
                "Tsundere",
                "Protagonist",
                "Red",
                "Green",
                18,
                "Z",
                &["Comedy"],
            ),
        ];
        let tree = build_preference_tree(&people);
        let path = dominant_query_path(&tree);
        assert_eq!(path[0], "Tsundere");
        assert_eq!(path[1], "Red");
    }

    #[test]
    fn idf_score_gates_on_archetype() {
        let people = vec![
            ch(
                "a",
                "Tsundere",
                "Protagonist",
                "Red",
                "Blue",
                17,
                "X",
                &["Romance"],
            ),
            ch(
                "b",
                "Tsundere",
                "Supporting",
                "Red",
                "Blue",
                16,
                "Y",
                &["Action"],
            ),
        ];
        let tree = build_preference_tree(&people);
        let idf = compute_idf_weights(&people);
        let matching = ch(
            "m",
            "Tsundere",
            "Protagonist",
            "Red",
            "Blue",
            17,
            "Q",
            &["Romance"],
        );
        let off_type = ch(
            "o",
            "Villain",
            "Antagonist",
            "Red",
            "Blue",
            17,
            "Q",
            &["Romance"],
        );
        assert!(score_character_idf(&matching, &tree, &idf) > 0.0);
        // Wrong archetype is excluded entirely.
        assert_eq!(score_character_idf(&off_type, &tree, &idf), 0.0);
    }

    #[test]
    fn kmeans_separates_two_groups() {
        let people = [
            ch(
                "a",
                "Tsundere",
                "Protagonist",
                "Red",
                "Blue",
                17,
                "X",
                &["Romance"],
            ),
            ch(
                "b",
                "Tsundere",
                "Protagonist",
                "Red",
                "Blue",
                18,
                "X",
                &["Romance"],
            ),
            ch(
                "c",
                "Villain",
                "Antagonist",
                "White",
                "Red",
                30,
                "Y",
                &["Action"],
            ),
            ch(
                "d",
                "Villain",
                "Antagonist",
                "White",
                "Red",
                32,
                "Y",
                &["Action"],
            ),
        ];
        let vecs: Vec<Vec<f32>> = people.iter().map(cluster_vector).collect();
        let assign = kmeans(&vecs, 2);
        assert_eq!(assign[0], assign[1]);
        assert_eq!(assign[2], assign[3]);
        assert_ne!(assign[0], assign[2]);
    }

    #[test]
    fn vibe_similarity_orders_correctly() {
        let taiga = ch(
            "Taiga",
            "Tsundere",
            "Protagonist",
            "Brown",
            "Brown",
            18,
            "Toradora",
            &["Romance", "Comedy"],
        );
        let near = ch(
            "Rin",
            "Tsundere",
            "Supporting",
            "Black",
            "Aqua",
            17,
            "Fate",
            &["Romance", "Action"],
        );
        let far = ch(
            "Dio",
            "Villain",
            "Antagonist",
            "Blonde",
            "Red",
            20,
            "JoJo",
            &["Action", "Supernatural"],
        );
        assert!(score_against(&near, &taiga) > score_against(&far, &taiga));
        assert_eq!(score_against(&taiga, &taiga), 100.0);
    }

    #[test]
    fn feature_vector_distance_orders_by_vibe() {
        let taiga = ch(
            "Taiga",
            "Tsundere",
            "Protagonist",
            "Brown",
            "Brown",
            18,
            "Toradora",
            &["Romance"],
        );
        let near = ch(
            "Chitoge",
            "Tsundere",
            "Protagonist",
            "Blonde",
            "Blue",
            16,
            "Nisekoi",
            &["Romance"],
        );
        let far = ch(
            "Levi",
            "Stoic",
            "Supporting",
            "Black",
            "Grey",
            30,
            "AoT",
            &["Action"],
        );
        let (vt, vn, vf) = (
            feature_vector(&taiga),
            feature_vector(&near),
            feature_vector(&far),
        );
        assert!(vt.distance(&vn) < vt.distance(&vf));
        assert!(vt.similarity_pct(&vn) > vt.similarity_pct(&vf));
    }

    #[test]
    fn genre_overlap_jaccard() {
        let a = vec!["Action".to_string(), "Romance".to_string()];
        let b = vec!["romance".to_string(), "comedy".to_string()];
        // shared: romance → 1 of 3 union = 0.333
        assert!((genre_overlap(&a, &b) - 0.3333).abs() < 0.01);
        assert_eq!(genre_overlap(&[], &b), 0.0);
    }

    #[test]
    fn cosine_basic() {
        assert!((cosine_similarity(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 1e-9);
        assert!(cosine_similarity(&[1.0, 0.0], &[0.0, 1.0]).abs() < 1e-9);
        assert_eq!(cosine_similarity(&[1.0], &[1.0, 2.0]), 0.0); // mismatched dims
    }
}
