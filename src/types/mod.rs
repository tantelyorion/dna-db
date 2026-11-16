//! Types fondamentaux de DNA DB : valeurs, gènes, chromosomes et genomes.

mod chromosome;
mod gene;
mod genome;
mod value;

pub use chromosome::{Chromosome, ChromosomeId};
pub use gene::{Gene, GeneId};
pub use genome::{FitnessScore, Genome, GenomeId, MemoryStatus};
pub use value::Value;
