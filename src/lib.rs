//! DNA DB - Base de données évolutive inspirée des mécanismes biologiques
//! 
//! Une base de données où les données sont des structures vivantes capables
//! d'évoluer dans le temps via des mutations, avec versionnement complet,
//! sélection naturelle basée sur le fitness, et recherche par similarité.

#![deny(unsafe_code)]
#![warn(missing_docs)]

pub mod types;
pub mod storage;
pub mod mutation;
pub mod fitness;
pub mod index;
pub mod cache;
pub mod db;

// Ré-exports principaux
pub use types::*;
pub use db::{DNADatabase, DNADatabaseBuilder};
pub use mutation::{Mutation, MutationEngine};
pub use fitness::{FitnessCalculator, FitnessConfig};