# Sauce

> A privacy-first CLI recommendation engine for discovering anime characters you'll love — built in Rust, powered by [AniList](https://anilist.co), with optional character-art visual similarity.

![Rust](https://img.shields.io/badge/Rust-stable-orange?logo=rust)
![License](https://img.shields.io/badge/license-MIT-blue)
![Local First](https://img.shields.io/badge/data-local--first-green)
![Tests](https://img.shields.io/badge/tests-24%20passing-brightgreen)

*"What's the sauce?"* — Sauce learns your taste from the characters you love and recommends new ones, using a preference tree, IDF-weighted scoring, k-NN similarity, k-means clustering, and (optionally) CLIP-style visual similarity over character art. Everything runs **locally**: no accounts, no telemetry, no cloud.

> 📐 See [DESIGN.md](DESIGN.md) for the architecture, data model, and algorithm diagrams.

It ships with a curated **catalog** of well-known characters baked into the binary, so it works fully offline out of the box — no API key required.

---

## Features

- **Preference tree** — builds an `archetype → hair → eye → age → role` tree from the characters you like, with counts and percentages at every branch.
- **Smart recommendations** — IDF-weighted scoring that emphasises what's *distinctive* about your taste, not just what's common; **archetype is a hard gate**.
- **Taste clusters** — k-means over your library finds your *multiple* types; `recommend --by-cluster` surfaces matches for each.
- **Vibe similarity** — k-NN over a weighted "personality" feature vector; `find --vibe-like "Levi Ackerman"` finds characters who feel the same.
- **Visual similarity (optional)** — CLIP-style embeddings of character art via a Python sidecar; `find --looks-like "X"` sorts by how the art actually *looks*.
- **Mix-and-match search** — `find --looks-like "A" --vibe-like "B"` combines one character's look with another's personality.
- **Plain-English queries** — `sauce query "blue-eyed tsunderes that look like Taiga"` (local rule-based parser, no LLM).
- **Series-aware "similar"** — characters from the same show rank as strongly related.
- **Local-first** — everything cached in SQLite in the OS-standard data dir.

---

## Why this project

It's a compact showcase of recommendation-systems engineering in Rust:

| Technique | Where |
|---|---|
| Decision-tree taste modelling | `src/recommender.rs` → `build_preference_tree` |
| TF-IDF–style attribute weighting | `compute_idf_weights` / `score_character_idf` |
| k-NN over weighted feature vectors | `feature_vector` / `FeatureVec::distance` |
| k-means clustering (deterministic init) | `kmeans` / `cluster_vector` |
| Embedding cosine similarity | `cosine_similarity` + `embed_image.py` |
| Jaccard set similarity | `genre_overlap` |
| Rule-based NL parsing | `src/query.rs` |
| Local-first persistence | `src/database.rs` (SQLite, bundled) |
| External REST/GraphQL integration | `src/source.rs` (AniList) |

All algorithms are pure functions over in-memory slices, covered by **24 tests**.

---

## Requirements

| Dependency | Purpose | Install |
|---|---|---|
| **Rust** (stable) | Build the binary | [rustup.rs](https://rustup.rs) |
| **Python 3.9+** + `open_clip_torch` | Visual similarity (optional) | `pip install open_clip_torch torch pillow` |

The core tool has **no runtime dependencies** beyond the binary itself — SQLite is compiled in, and the catalog is embedded. AniList is keyless. Visual similarity is the only optional extra.

---

## Installation

Cross-platform — builds and runs on **macOS, Windows, and Linux**. The database
lives in the OS-standard data dir, resolved at runtime (no hardcoded paths):
`~/Library/Application Support/sauce` on macOS, `%LOCALAPPDATA%\sauce` on
Windows, `~/.local/share/sauce` on Linux.

```bash
git clone https://github.com/TcgVanguardTroll/Sauce.git
cd Sauce
cargo build --release
# or install onto your PATH:
cargo install --path .
```

Binary: `target/release/sauce` (`sauce.exe` on Windows).

---

## Quick Start

```bash
# Add characters you love (resolved from the built-in catalog, or fetched from AniList)
sauce add "Taiga Aisaka" "Rin Tohsaka" "Kurisu Makise" "Rei Ayanami"

# See your taste profile
sauce profile

# Get recommendations
sauce recommend

# Find a stoic with Levi's energy
sauce find --vibe-like "Levi Ackerman"
```

```
Your Taste Profile
══════════════════════════════════════════
  Based on 4 favourites

  ├── Tsundere 3/4  75%
  │   ├── Red 2/3  67%
  │   │   └── Blue 2/2  100%
  │   └── Brown 1/3  33%
  └── Kuudere 1/4  25%

  Your type: Tsundere → Red → Blue
```

---

## Commands

### Managing your library

```bash
sauce add "Name" ["Name2" ...]   # add from the catalog (offline) or AniList
sauce list                        # list your library
sauce view "Name"                 # show a stored character's profile
sauce remove "Name"               # remove a character
sauce catalog [--limit N]         # browse the built-in candidate catalog
sauce stats                       # library/catalog/db info
```

### Taste profile

```bash
sauce profile            # ASCII preference tree + "your type"
sauce profile --mermaid  # emit a Mermaid flowchart for docs
```

The tree drills through **archetype → hair → eye → age → role**. Each level shows
counts and percentages. The more characters you add, the more specific it gets.

### Recommendations

```bash
sauce recommend [--limit 10]   # score the catalog against your taste tree
sauce recommend --by-cluster   # separate recommendations per taste cluster
sauce similar "Levi Ackerman"  # characters most like one specific character
```

`recommend` scores every catalog candidate against your tree. **Archetype is a
hard exclusion gate** — the wrong personality type is excluded entirely. Rare
distinctive traits (via IDF) count for more than ones every favourite shares.

### Clusters

```bash
sauce clusters [--k 3]
```

k-means over your library detects the distinct "types" in your taste and labels
each by its dominant archetype/role/hair.

### Advanced search — `find`

```bash
# Personality match (k-NN over the "vibe" vector)
sauce find --vibe-like "Levi Ackerman"

# Visual match (needs embeddings — see below)
sauce find --looks-like "Rin Tohsaka"

# Mix: one character's look, another's vibe
sauce find --looks-like "Rin Tohsaka" --vibe-like "Levi Ackerman"

# Manual attribute filters
sauce find --archetype Tsundere --hair Blonde --limit 5
```

| Flag | Values | Notes |
|------|--------|-------|
| `--looks-like` | character name (repeatable) | ranks by art similarity when embeddings exist |
| `--vibe-like` | character name (repeatable) | ranks by k-NN personality similarity |
| `--archetype` | `Tsundere`, `Kuudere`, `Yandere`, `Dandere`, `Deredere`, `Genki`, `Hot-Blooded`, `Stoic`, `Mastermind`, `Villain` | hard filter |
| `--role` | `Protagonist`, `Antagonist`, `Supporting`, `Rival` | hard filter |
| `--hair` / `--eye` | colour | hard filter |
| `--gender` | `Female`, `Male`, `Non-binary` | hard filter |
| `--limit` | `10` | number of results |

### Plain-English queries

```bash
sauce query "blue-eyed tsunderes that look like Taiga Aisaka"
sauce query "pink haired girls with the vibe of Yuno Gasai"
sauce query "characters that look like Rin Tohsaka with the vibe of Levi Ackerman"
```

A local rule-based parser (no LLM) extracts appearance/archetype attributes and
splits `"<trait> like <name>"` / `"<trait> of <name>"` clauses into look vs vibe
references, then hands off to `find`.

### Visual similarity (optional ML)

```bash
pip install open_clip_torch torch pillow   # one-time
sauce embed                                 # embed your library's character art
sauce find --looks-like "Rin Tohsaka"       # now ranks by visual similarity
```

`sauce embed` shells out to `embed_image.py`, which loads a CLIP image encoder
and returns an L2-normalised vector per character image (from AniList). Sauce
stores it in SQLite and ranks `--looks-like` by **cosine similarity**. If Python
or the model isn't installed, the tool transparently falls back to attribute-only
ranking — nothing else breaks.

### Settings

```bash
sauce config                    # show settings
sauce config gender female      # restrict to a gender (female/male/non-binary/any)
```

---

## Data sources

- **Bundled catalog** (`data/seeds.json`) — a curated, hand-labelled roster
  compiled into the binary, so the tool works fully offline.
- **AniList GraphQL API** (`https://graphql.anilist.co`) — keyless. Factual
  fields (name, gender, age, series, genres, popularity, image) come straight
  from AniList; hair/eye/archetype are enriched from the description by a
  rule-based extractor (`src/extract.rs`).

## Data & privacy

| What | Where |
|------|-------|
| Character database | `<data-dir>/sauce/sauce.db` |
| Visual embeddings | stored inside the same SQLite DB |
| Settings | `<data-dir>/sauce/config.json` |

Nothing leaves your machine except outbound calls to AniList when you explicitly
`add` a character that isn't already in the catalog.

---

## Development

```bash
cargo test       # 24 unit + integration tests
cargo clippy --all-targets -- -D warnings
cargo run -- profile
```

## License

MIT — see [LICENSE](LICENSE).
