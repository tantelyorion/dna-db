//! DNA DB : base de données évolutive inspirée des mécanismes biologiques.
//!
//! Les données sont des *genomes* (chromosomes de gènes) immuables : chaque mutation
//! produit un nouvel enfant, ce qui donne un versionnement complet avec lignée
//! (`parent_id`, génération) et deltas rejouables. Un score de *fitness* (accès,
//! récence, pertinence, structure) classe les genomes ; une signature génétique
//! (SimHash) permet de retrouver des genomes similaires ; un WAL protège les mutations
//! contre les crashs.

#![deny(unsafe_code)]
#![warn(missing_docs)]

pub mod cache;
pub mod db;
pub mod fitness;
pub mod index;
pub mod mutation;
pub mod storage;
pub mod types;
mod util;

// Ré-exports principaux
pub use db::{DNADatabase, DNADatabaseBuilder, DNADatabaseConfig};
pub use fitness::{FitnessCalculator, FitnessConfig};
pub use mutation::{Delta, DeltaBuilder, DeltaOperation, Mutation, MutationEngine};
pub use types::*;
