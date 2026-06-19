//! Sauce — a privacy-first CLI recommendation engine for discovering anime
//! characters you'll love.
//!
//! The core logic lives in this library crate so it can be unit-tested in
//! isolation; the binary (`src/main.rs`) is a thin CLI over it.

pub mod config;
pub mod database;
pub mod embedder;
pub mod extract;
pub mod models;
pub mod query;
pub mod recommender;
pub mod source;

pub use models::Character;
