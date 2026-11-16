//! Système de mutations : cœur de l'évolution.

mod delta;
mod engine;

pub use delta::{Delta, DeltaBuilder, DeltaOperation};
pub use engine::{Mutation, MutationEngine};
