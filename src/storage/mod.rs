//! Système de stockage de DNA DB

mod file_format;
mod mmap_storage;
mod std_storage;
mod wal;

pub use file_format::{FileHeader, FileType, Delta, DeltaOperation};
pub use mmap_storage::MmapStorage;
pub use std_storage::StdStorage;
pub use wal::{WriteAheadLog, WalEntry};

use crate::types::Genome;
use std::path::Path;

/// Trait de stockage générique
pub trait StorageBackend {
    fn save_genome(&self, genome: &Genome) -> std::io::Result<()>;
    fn load_genome(&self, genome_id: u64) -> std::io::Result<Genome>;
    fn exists(&self, genome_id: u64) -> bool;
    fn delete_genome(&self, genome_id: u64) -> std::io::Result<()>;
}

/// Sélection intelligente du backend selon la taille
pub struct SmartStorage {
    mmap: MmapStorage,
    std: StdStorage,
    threshold_bytes: u64,
}

impl SmartStorage {
    pub fn new(base_path: &Path, mmap_threshold_bytes: u64) -> std::io::Result<Self> {
        Ok(Self {
            mmap: MmapStorage::new(base_path)?,
            std: StdStorage::new(base_path)?,
            threshold_bytes: mmap_threshold_bytes,
        })
    }
    
    pub fn save_genome(&self, genome: &Genome) -> std::io::Result<()> {
        if genome.size_bytes() as u64 > self.threshold_bytes {
            self.mmap.save_checkpoint(genome)
        } else {
            self.std.save_genome(genome)
        }
    }
    
    pub fn load_genome(&self, genome_id: u64) -> std::io::Result<Genome> {
        if self.mmap.exists(genome_id) {
            self.mmap.load_genome(genome_id)
        } else {
            self.std.load_genome(genome_id)
        }
    }
    
    pub fn exists(&self, genome_id: u64) -> bool {
        self.mmap.exists(genome_id) || self.std.exists(genome_id)
    }
    
    pub fn delete_genome(&self, genome_id: u64) -> std::io::Result<()> {
        let _ = self.mmap.delete_genome(genome_id);
        let _ = self.std.delete_genome(genome_id);
        Ok(())
    }
}