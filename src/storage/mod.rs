//! Système de stockage de DNA DB : fichiers atomiques avec checksum + journal WAL.

pub mod file_format;
mod file_storage;
mod wal;

pub use file_format::{FileHeader, FileType};
pub use file_storage::FileStorage;
pub use wal::{WalEntry, WriteAheadLog};

use crate::types::{Genome, GenomeId};
use std::io;

/// Contrat minimal d'un backend de stockage de genomes.
pub trait StorageBackend {
    /// Sauvegarde un genome (remplace la version précédente).
    fn save_genome(&self, genome: &Genome) -> io::Result<()>;
    /// Charge un genome.
    fn load_genome(&self, genome_id: GenomeId) -> io::Result<Genome>;
    /// Vrai si le genome existe.
    fn exists(&self, genome_id: GenomeId) -> bool;
    /// Supprime un genome.
    fn delete_genome(&self, genome_id: GenomeId) -> io::Result<()>;
}
