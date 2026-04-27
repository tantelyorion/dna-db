//! Stockage standard (sans mmap) pour les petits fichiers

use crate::types::*;
use crate::storage::file_format::{FileHeader, FileType, Delta};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write, BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use parking_lot::RwLock;

/// Stockage utilisant la lecture/écriture standard (copie mémoire)
pub struct StdStorage {
    base_path: PathBuf,
    checkpoints: RwLock<HashMap<GenomeId, PathBuf>>,
}

impl StdStorage {
    pub fn new(base_path: &Path) -> std::io::Result<Self> {
        std::fs::create_dir_all(base_path.join("checkpoints"))?;
        
        let mut storage = Self {
            base_path: base_path.to_path_buf(),
            checkpoints: RwLock::new(HashMap::new()),
        };
        
        storage.scan_existing_files()?;
        Ok(storage)
    }
    
    fn scan_existing_files(&self) -> std::io::Result<()> {
        if let Ok(entries) = std::fs::read_dir(self.base_path.join("checkpoints")) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    if let Ok(id) = name.parse::<u64>() {
                        self.checkpoints.write().insert(id, path);
                    }
                }
            }
        }
        Ok(())
    }
    
    fn checkpoint_path(&self, genome_id: GenomeId) -> PathBuf {
        self.base_path.join("checkpoints").join(format!("{}.bin", genome_id))
    }
    
    /// Sauvegarde un genome
    pub fn save_genome(&self, genome: &Genome) -> std::io::Result<()> {
        let path = self.checkpoint_path(genome.id);
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)?;
        
        let mut writer = BufWriter::new(file);
        let header = FileHeader::new(FileType::Checkpoint);
        header.write(&mut writer)?;
        genome.serialize(&mut writer)?;
        writer.flush()?;
        
        self.checkpoints.write().insert(genome.id, path);
        Ok(())
    }
    
    /// Charge un genome
    pub fn load_genome(&self, genome_id: GenomeId) -> std::io::Result<Genome> {
        let path = self.checkpoints.read()
            .get(&genome_id)
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "Genome not found"))?;
        
        let file = File::open(path)?;
        let mut reader = BufReader::new(file);
        let mut data = Vec::new();
        reader.read_to_end(&mut data)?;
        
        Genome::deserialize(&mut data.as_slice())
    }
    
    /// Vérifie si un genome existe
    pub fn exists(&self, genome_id: GenomeId) -> bool {
        self.checkpoints.read().contains_key(&genome_id)
    }
    
    /// Supprime un genome
    pub fn delete_genome(&self, genome_id: GenomeId) -> std::io::Result<()> {
        if let Some(path) = self.checkpoints.write().remove(&genome_id) {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }
}