//! Command-line interface — a thin shell over the `sauce` library crate.

use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use colored::*;
use std::collections::HashSet;

use sauce::config::Config;
use sauce::database::Database;
use sauce::models::Character;
use sauce::recommender as rec;
use sauce::source::{AniListClient, SeedStore};
use sauce::{embedder, query};

#[derive(Parser)]
#[command(
    name = "sauce",
    version,
    about = "Discover anime characters you'll love — a privacy-first CLI recommender."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Add character(s) to your library (from the bundled catalog or AniList).
    Add {
        #[arg(required = true)]
        names: Vec<String>,
    },
    /// List the characters in your library.
    List,
    /// Show a stored character's full profile.
    View { name: String },
    /// Remove a character from your library.
    Remove { name: String },
    /// Library + cache statistics.
    Stats,
    /// Browse the bundled candidate catalog.
    Catalog {
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    /// Show your taste profile as a preference tree.
    Profile {
        /// Emit a Mermaid flowchart instead of an ASCII tree.
        #[arg(long)]
        mermaid: bool,
    },
    /// Recommend new characters based on your taste.
    Recommend {
        #[arg(long, default_value_t = 10)]
        limit: usize,
        /// Recommend separately for each of your taste clusters.
        #[arg(long)]
        by_cluster: bool,
    },
    /// Detect and label your taste clusters (k-means).
    Clusters {
        #[arg(long)]
        k: Option<usize>,
    },
    /// Mix-and-match search across attributes and references.
    Find {
        /// Visual reference — rank by character-art similarity (needs `embed`).
        #[arg(long)]
        looks_like: Vec<String>,
        /// Personality reference — rank by "vibe" (k-NN) similarity.
        #[arg(long)]
        vibe_like: Vec<String>,
        #[arg(long)]
        archetype: Option<String>,
        #[arg(long)]
        role: Option<String>,
        #[arg(long)]
        hair: Option<String>,
        #[arg(long)]
        eye: Option<String>,
        #[arg(long)]
        gender: Option<String>,
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// Characters similar to one specific character.
    Similar {
        name: String,
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// Plain-English search, e.g. "blue-eyed tsunderes that look like Taiga".
    Query {
        #[arg(required = true)]
        text: Vec<String>,
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// Generate visual embeddings for your library (optional Python sidecar).
    Embed {
        #[arg(long)]
        force: bool,
    },
    /// Show or change settings (e.g. `config gender female`).
    Config {
        key: Option<String>,
        value: Option<String>,
    },
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    let db = Database::open(&sauce::config::db_path()?)?;
    let cfg = Config::load()?;

    match cli.command {
        Commands::Add { names } => add(&db, &names),
        Commands::List => list(&db),
        Commands::View { name } => view(&db, &name),
        Commands::Remove { name } => remove(&db, &name),
        Commands::Stats => stats(&db),
        Commands::Catalog { limit } => catalog(limit),
        Commands::Profile { mermaid } => profile(&db, mermaid),
        Commands::Recommend { limit, by_cluster } => recommend(&db, &cfg, limit, by_cluster),
        Commands::Clusters { k } => clusters(&db, k),
        Commands::Find {
            looks_like,
            vibe_like,
            archetype,
            role,
            hair,
            eye,
            gender,
            limit,
        } => find(
            &db, &cfg, looks_like, vibe_like, archetype, role, hair, eye, gender, limit,
        ),
        Commands::Similar { name, limit } => similar(&db, &cfg, &name, limit),
        Commands::Query { text, limit } => nl_query(&db, &cfg, &text.join(" "), limit),
        Commands::Embed { force } => embed(&db, force),
        Commands::Config { key, value } => configure(key, value),
    }
}

// ── Library management ──────────────────────────────────────────────────────

fn add(db: &Database, names: &[String]) -> Result<()> {
    let catalog = SeedStore::load()?;
    let anilist = AniListClient::new();
    for name in names {
        match resolve_source(&catalog, &anilist, name) {
            Ok(c) => {
                db.upsert(&c)?;
                println!(
                    "{} {}  {}",
                    "+".green().bold(),
                    c.name.bold(),
                    c.summary().bright_black()
                );
            }
            Err(e) => eprintln!("{} {}: {}", "✗".red(), name, e),
        }
    }
    Ok(())
}

/// Catalog first (works offline), then the live AniList API.
fn resolve_source(catalog: &SeedStore, anilist: &AniListClient, name: &str) -> Result<Character> {
    if let Some(c) = catalog.find(name) {
        return Ok(c);
    }
    anilist
        .fetch(name)
        .map_err(|e| anyhow!("not in catalog and AniList lookup failed ({e})"))
}

fn list(db: &Database) -> Result<()> {
    let all = db.all()?;
    if all.is_empty() {
        println!(
            "{}",
            "Your library is empty. Try: sauce add \"Taiga Aisaka\"".bright_black()
        );
        return Ok(());
    }
    println!("{}", format!("Your library ({})", all.len()).bold());
    for c in &all {
        println!("  {}  {}", c.name.bold(), c.summary().bright_black());
    }
    Ok(())
}

fn view(db: &Database, name: &str) -> Result<()> {
    let Some(c) = db
        .get(name)?
        .or_else(|| SeedStore::load().ok().and_then(|s| s.find(name)))
    else {
        return Err(anyhow!(
            "'{}' not found in your library or the catalog",
            name
        ));
    };
    println!("{}", c.name.bold().underline());
    field("Archetype", Some(&c.archetype));
    field("Role", c.role.as_deref());
    field("Hair", c.hair_color.as_deref());
    field("Eyes", c.eye_color.as_deref());
    field("Gender", c.gender.as_deref());
    field("Age", c.age.map(|a| a.to_string()).as_deref());
    field("Series", c.series.as_deref());
    if !c.genres.is_empty() {
        field("Genres", Some(&c.genres.join(", ")));
    }
    field("Popularity", c.popularity.map(|p| p.to_string()).as_deref());
    field("Source", c.source.as_deref());
    field("Link", c.source_url.as_deref());
    if let Some(d) = &c.description {
        println!("\n{}", d.bright_black());
    }
    Ok(())
}

fn field(label: &str, value: Option<&str>) {
    if let Some(v) = value {
        if !v.is_empty() {
            println!("  {:<11} {}", format!("{}:", label).bright_black(), v);
        }
    }
}

fn remove(db: &Database, name: &str) -> Result<()> {
    if db.remove(name)? {
        println!("{} removed {}", "−".red().bold(), name.bold());
    } else {
        println!("{} '{}' was not in your library", "?".yellow(), name);
    }
    Ok(())
}

fn stats(db: &Database) -> Result<()> {
    let n = db.count()?;
    let catalog = SeedStore::load()?.all().len();
    println!("{}", "Sauce".bold());
    println!("  {:<18} {}", "Library:".bright_black(), n);
    println!("  {:<18} {}", "Catalog:".bright_black(), catalog);
    println!(
        "  {:<18} {}",
        "Database:".bright_black(),
        sauce::config::db_path()?.display()
    );
    println!(
        "  {:<18} {}",
        "Visual sidecar:".bright_black(),
        if embedder::available() {
            "available".green()
        } else {
            "not installed".bright_black()
        }
    );
    Ok(())
}

fn catalog(limit: usize) -> Result<()> {
    let store = SeedStore::load()?;
    let mut all: Vec<&Character> = store.all().iter().collect();
    all.sort_by_key(|c| std::cmp::Reverse(c.popularity.unwrap_or(0)));
    println!(
        "{}",
        format!("Candidate catalog ({})", store.all().len()).bold()
    );
    for c in all.into_iter().take(limit) {
        println!("  {}  {}", c.name.bold(), c.summary().bright_black());
    }
    Ok(())
}

// ── Taste profile ───────────────────────────────────────────────────────────

fn profile(db: &Database, mermaid: bool) -> Result<()> {
    let lib = db.all()?;
    if lib.is_empty() {
        return Err(anyhow!(
            "add some favourites first: sauce add \"Taiga Aisaka\""
        ));
    }
    let tree = rec::build_preference_tree(&lib);
    if mermaid {
        print!("{}", rec::to_mermaid(&tree, lib.len()));
        return Ok(());
    }
    println!("{}", "Your Taste Profile".bold());
    println!(
        "{}",
        "══════════════════════════════════════════".bright_black()
    );
    println!("  Based on {} favourites\n", lib.len());
    rec::print_tree(&tree, "  ", lib.len());
    let path = rec::dominant_query_path(&tree);
    if !path.is_empty() {
        println!(
            "\n  {} {}",
            "Your type:".bright_black(),
            path.join(" → ").bright_cyan().bold()
        );
    }
    Ok(())
}

// ── Recommendations ─────────────────────────────────────────────────────────

fn recommend(db: &Database, cfg: &Config, limit: usize, by_cluster: bool) -> Result<()> {
    let lib = db.all()?;
    if lib.is_empty() {
        return Err(anyhow!(
            "add some favourites first: sauce add \"Taiga Aisaka\""
        ));
    }
    let known: HashSet<String> = lib.iter().map(|c| c.name.to_lowercase()).collect();
    let pool = candidate_pool(cfg, &known)?;

    if by_cluster {
        let vecs: Vec<Vec<f32>> = lib.iter().map(rec::cluster_vector).collect();
        let k = auto_k(lib.len());
        let assign = rec::kmeans(&vecs, k);
        for cid in 0..k {
            let members: Vec<Character> = lib
                .iter()
                .zip(&assign)
                .filter(|(_, &a)| a == cid)
                .map(|(c, _)| c.clone())
                .collect();
            if members.is_empty() {
                continue;
            }
            println!(
                "\n{} {}",
                "Cluster:".bold(),
                cluster_label(&members).bright_cyan()
            );
            let scored = score_pool(&members, &pool);
            print_recs(&scored, limit.min(5));
        }
        return Ok(());
    }

    let scored = score_pool(&lib, &pool);
    if scored.is_empty() {
        println!(
            "{}",
            "No new recommendations — your library already covers the catalog.".bright_black()
        );
        return Ok(());
    }
    println!("{}", "Recommended for you".bold());
    print_recs(&scored, limit);
    Ok(())
}

/// Scores a candidate pool against a liked set using the preference tree + IDF.
fn score_pool(liked: &[Character], pool: &[Character]) -> Vec<(f64, Character)> {
    let tree = rec::build_preference_tree(liked);
    let idf = rec::compute_idf_weights(liked);
    let mut scored: Vec<(f64, Character)> = pool
        .iter()
        .map(|c| (rec::score_character_idf(c, &tree, &idf), c.clone()))
        .filter(|(s, _)| *s > 0.0)
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored
}

fn print_recs(scored: &[(f64, Character)], limit: usize) {
    // Normalise display: the top raw score maps to ~100%.
    let top = scored.first().map(|(s, _)| *s).unwrap_or(1.0).max(1e-9);
    for (i, (score, c)) in scored.iter().take(limit).enumerate() {
        println!(
            "{}. {}  {}  {}",
            (i + 1).to_string().bright_black(),
            c.name.bold(),
            format!("({})", c.summary()).bright_black(),
            format!("match {:.0}%", (score / top * 100.0)).bright_cyan()
        );
    }
}

// ── Clustering ──────────────────────────────────────────────────────────────

fn clusters(db: &Database, k: Option<usize>) -> Result<()> {
    let lib = db.all()?;
    if lib.len() < 2 {
        return Err(anyhow!("need at least 2 favourites to cluster"));
    }
    let k = k.unwrap_or_else(|| auto_k(lib.len()));
    let vecs: Vec<Vec<f32>> = lib.iter().map(rec::cluster_vector).collect();
    let assign = rec::kmeans(&vecs, k);
    println!("{}", format!("Your taste clusters (k={})", k).bold());
    for cid in 0..k {
        let members: Vec<Character> = lib
            .iter()
            .zip(&assign)
            .filter(|(_, &a)| a == cid)
            .map(|(c, _)| c.clone())
            .collect();
        if members.is_empty() {
            continue;
        }
        println!(
            "\n  {} {}",
            "▸".bright_cyan(),
            cluster_label(&members).bright_cyan().bold()
        );
        for m in &members {
            println!("      {}  {}", m.name, m.summary().bright_black());
        }
    }
    Ok(())
}

// ── Find ────────────────────────────────────────────────────────────────────

fn find(
    db: &Database,
    cfg: &Config,
    looks_like: Vec<String>,
    vibe_like: Vec<String>,
    archetype: Option<String>,
    role: Option<String>,
    hair: Option<String>,
    eye: Option<String>,
    gender: Option<String>,
    limit: usize,
) -> Result<()> {
    let catalog = SeedStore::load()?;
    let resolve = |name: &str| db.get(name).ok().flatten().or_else(|| catalog.find(name));

    let look_refs: Vec<Character> = looks_like.iter().filter_map(|n| resolve(n)).collect();
    let vibe_refs: Vec<Character> = vibe_like.iter().filter_map(|n| resolve(n)).collect();
    let ref_names: HashSet<String> = look_refs
        .iter()
        .chain(&vibe_refs)
        .map(|c| c.name.to_lowercase())
        .collect();

    // Candidate pool = catalog ∪ library, minus the references themselves.
    let mut pool: Vec<Character> = catalog.all().to_vec();
    for c in db.all()? {
        if !pool.iter().any(|p| p.name.eq_ignore_ascii_case(&c.name)) {
            pool.push(c);
        }
    }
    pool.retain(|c| !ref_names.contains(&c.name.to_lowercase()) && cfg.allows(c.gender.as_deref()));

    // Hard attribute filters.
    let eqf = |a: &Option<String>, b: &Option<String>| match b {
        Some(want) => a
            .as_deref()
            .map(|v| v.eq_ignore_ascii_case(want))
            .unwrap_or(false),
        None => true,
    };
    // hair/eye from look refs become soft targets, not hard filters.
    pool.retain(|c| {
        (archetype
            .as_ref()
            .map(|a| c.archetype.eq_ignore_ascii_case(a))
            .unwrap_or(true))
            && eqf(&c.role, &role)
            && eqf(&c.hair_color, &hair)
            && eqf(&c.eye_color, &eye)
            && eqf(&c.gender, &gender)
    });

    if pool.is_empty() {
        println!("{}", "No matches for those filters.".bright_black());
        return Ok(());
    }

    // Ranking: vibe similarity (k-NN) when a vibe reference is given, else
    // attribute match against a synthetic reference built from the flags and
    // any look reference's appearance.
    let vibe_blend = if vibe_refs.is_empty() {
        None
    } else {
        rec::FeatureVec::average(
            &vibe_refs
                .iter()
                .map(rec::feature_vector)
                .collect::<Vec<_>>(),
        )
    };

    let mut synth = Character::new("<query>");
    synth.archetype = archetype.clone().unwrap_or_default();
    synth.role = role.clone();
    synth.gender = gender.clone();
    synth.hair_color = hair
        .clone()
        .or_else(|| look_refs.first().and_then(|r| r.hair_color.clone()));
    synth.eye_color = eye
        .clone()
        .or_else(|| look_refs.first().and_then(|r| r.eye_color.clone()));

    // Visual cosine when embeddings exist for the look reference(s).
    let look_embedding = look_refs
        .iter()
        .filter_map(|r| db.embedding(&r.name).ok().flatten())
        .next();

    let mut scored: Vec<(f64, String, Character)> = pool
        .into_iter()
        .map(|c| {
            let (key, badge) = if let (Some(le), Some(ce)) =
                (&look_embedding, db.embedding(&c.name).ok().flatten())
            {
                let s = rec::cosine_similarity(le, &ce) * 100.0;
                (s, format!("look {:.0}%", s))
            } else if let Some(blend) = &vibe_blend {
                let s = blend.similarity_pct(&rec::feature_vector(&c));
                (s, format!("vibe {:.0}%", s))
            } else {
                let s = rec::score_against(&c, &synth);
                (s, format!("match {:.0}%", s))
            };
            (key, badge, c)
        })
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    for (i, (_, badge, c)) in scored.iter().take(limit).enumerate() {
        println!(
            "{}. {}  {}  {}",
            (i + 1).to_string().bright_black(),
            c.name.bold(),
            format!("({})", c.summary()).bright_black(),
            badge.bright_cyan()
        );
    }
    Ok(())
}

// ── Similar ─────────────────────────────────────────────────────────────────

fn similar(db: &Database, cfg: &Config, name: &str, limit: usize) -> Result<()> {
    let catalog = SeedStore::load()?;
    let reference = db
        .get(name)?
        .or_else(|| catalog.find(name))
        .ok_or_else(|| anyhow!("'{}' not found", name))?;

    let mut scored: Vec<(f64, Character)> = catalog
        .all()
        .iter()
        .filter(|c| {
            !c.name.eq_ignore_ascii_case(&reference.name) && cfg.allows(c.gender.as_deref())
        })
        .map(|c| (rec::score_against(c, &reference), c.clone()))
        .filter(|(s, _)| *s > 0.0)
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    println!("{} {}", "Similar to".bold(), reference.name.bold());
    for (i, (s, c)) in scored.iter().take(limit).enumerate() {
        println!(
            "{}. {}  {}  {}",
            (i + 1).to_string().bright_black(),
            c.name.bold(),
            format!("({})", c.summary()).bright_black(),
            format!("{:.0}%", s).bright_cyan()
        );
    }
    Ok(())
}

// ── Natural-language query ──────────────────────────────────────────────────

fn nl_query(db: &Database, cfg: &Config, text: &str, limit: usize) -> Result<()> {
    let q = query::parse(text);
    println!(
        "{} {}",
        "Parsed:".bright_black(),
        format!("{:?}", q).bright_black()
    );
    find(
        db,
        cfg,
        q.looks_like,
        q.vibe_like,
        q.archetype,
        None,
        q.hair,
        q.eye,
        None,
        limit,
    )
}

// ── Embeddings ──────────────────────────────────────────────────────────────

fn embed(db: &Database, force: bool) -> Result<()> {
    if !embedder::available() {
        return Err(anyhow!(
            "embed_image.py not found. See the README — install it to enable visual similarity."
        ));
    }
    let targets: Vec<String> = if force {
        db.all()?.into_iter().map(|c| c.name).collect()
    } else {
        db.names_missing_embedding()?
    };
    if targets.is_empty() {
        println!("{}", "All characters already embedded.".bright_black());
        return Ok(());
    }
    for name in targets {
        let Some(c) = db.get(&name)? else { continue };
        let Some(url) = c.image_url.as_deref() else {
            eprintln!(
                "{} {}: no image_url (added from AniList?)",
                "·".bright_black(),
                name
            );
            continue;
        };
        match embedder::embed(url) {
            Ok(v) => {
                db.set_embedding(&name, &v)?;
                println!("{} {}", "✓".green(), name);
            }
            Err(e) => eprintln!("{} {}: {}", "✗".red(), name, e),
        }
    }
    Ok(())
}

// ── Config ──────────────────────────────────────────────────────────────────

fn configure(key: Option<String>, value: Option<String>) -> Result<()> {
    let mut cfg = Config::load()?;
    match (key.as_deref(), value) {
        (None, _) => {
            println!("{}", "Settings".bold());
            println!("  gender = {}", cfg.gender_filter);
        }
        (Some("gender"), Some(v)) => {
            cfg.gender_filter = v.to_lowercase();
            cfg.save()?;
            println!("gender = {}", cfg.gender_filter);
        }
        (Some(k), _) => return Err(anyhow!("unknown setting '{}'", k)),
    }
    Ok(())
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// The candidate pool: the bundled catalog, minus what's already in the library,
/// filtered by the gender setting.
fn candidate_pool(cfg: &Config, known: &HashSet<String>) -> Result<Vec<Character>> {
    Ok(SeedStore::load()?
        .all()
        .iter()
        .filter(|c| !known.contains(&c.name.to_lowercase()) && cfg.allows(c.gender.as_deref()))
        .cloned()
        .collect())
}

/// A short label for a cluster from its members' dominant traits.
fn cluster_label(members: &[Character]) -> String {
    let mode = |vals: Vec<Option<String>>| -> Option<String> {
        let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for v in vals.into_iter().flatten() {
            *counts.entry(v).or_insert(0) += 1;
        }
        counts.into_iter().max_by_key(|(_, c)| *c).map(|(k, _)| k)
    };
    let arch = mode(members.iter().map(|c| Some(c.archetype.clone())).collect());
    let role = mode(members.iter().map(|c| c.role.clone()).collect());
    let hair = mode(members.iter().map(|c| c.hair_color.clone()).collect());
    [arch, role, hair]
        .into_iter()
        .flatten()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
}

fn auto_k(n: usize) -> usize {
    (n / 4).clamp(2, 5)
}
