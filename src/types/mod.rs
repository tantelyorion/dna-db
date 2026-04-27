//! Types fondamentaux de DNA DB

mod value;
mod gene;
mod chromosome;
mod genome;

pub use value::Value;
pub use gene::{Gene, GeneId};
pub use chromosome::{Chromosome, ChromosomeId};
pub use genome::{Genome, GenomeId, FitnessScore, MemoryStatus};