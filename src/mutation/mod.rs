//! Système de mutations - cœur de l'évolution

mod engine;
mod delta;

pub use engine::{Mutation, MutationEngine};
pub use delta::DeltaBuilder;