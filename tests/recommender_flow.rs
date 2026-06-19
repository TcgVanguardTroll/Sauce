//! End-to-end flow over the public library API: load the catalog, build a small
//! library, persist it, and check that the recommender behaves sensibly.

use sauce::database::Database;
use sauce::recommender as rec;
use sauce::source::SeedStore;

#[test]
fn catalog_loads_and_persists_into_db() {
    let catalog = SeedStore::load().unwrap();
    let db = Database::in_memory().unwrap();

    // Build a tsundere-heavy "favourites" library from the catalog.
    for name in ["Taiga Aisaka", "Rin Tohsaka", "Kurisu Makise"] {
        let c = catalog
            .find(name)
            .unwrap_or_else(|| panic!("{name} missing from catalog"));
        db.upsert(&c).unwrap();
    }
    assert_eq!(db.count().unwrap(), 3);

    let lib = db.all().unwrap();
    let tree = rec::build_preference_tree(&lib);
    let path = rec::dominant_query_path(&tree);
    assert_eq!(path.first().map(String::as_str), Some("Tsundere"));
}

#[test]
fn recommendations_respect_the_archetype_gate() {
    let catalog = SeedStore::load().unwrap();
    let liked: Vec<_> = ["Taiga Aisaka", "Chitoge Kirisaki", "Erina Nakiri"]
        .iter()
        .map(|n| catalog.find(n).unwrap())
        .collect();

    let tree = rec::build_preference_tree(&liked);
    let idf = rec::compute_idf_weights(&liked);

    let liked_names: std::collections::HashSet<String> =
        liked.iter().map(|c| c.name.clone()).collect();

    let mut scored: Vec<(f64, String, String)> = catalog
        .all()
        .iter()
        .filter(|c| !liked_names.contains(&c.name))
        .map(|c| {
            (
                rec::score_character_idf(c, &tree, &idf),
                c.name.clone(),
                c.archetype.clone(),
            )
        })
        .filter(|(s, _, _)| *s > 0.0)
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());

    assert!(!scored.is_empty(), "expected some recommendations");
    // Every scored recommendation must be a Tsundere (the hard gate) since the
    // entire liked set is Tsundere.
    for (_, name, archetype) in &scored {
        assert_eq!(
            archetype, "Tsundere",
            "{name} slipped past the archetype gate"
        );
    }
}

#[test]
fn similar_ranks_same_series_and_archetype_higher() {
    let catalog = SeedStore::load().unwrap();
    let levi = catalog.find("Levi Ackerman").unwrap();

    let mut scored: Vec<(f64, String)> = catalog
        .all()
        .iter()
        .filter(|c| c.name != levi.name)
        .map(|c| (rec::score_against(c, &levi), c.name.clone()))
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());

    // Mikasa: same series (AoT) AND same archetype (Stoic) → should top the list.
    assert_eq!(
        scored.first().map(|(_, n)| n.as_str()),
        Some("Mikasa Ackerman")
    );
}
