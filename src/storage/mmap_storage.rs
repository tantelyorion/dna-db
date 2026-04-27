//! Stockage avec memory-mapped files

use crate::types::*;
use crate::storage::file_format::{FileHeader, FileType, Delta, DeltaOperation};
use memmap2::{Mmap, MmapMut};
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use parking_lot::RwLock;
use std::collections::HashMap;

/// Stockage utilisant mmap pour les gros fichiers
pub struct MmapStorage {
    base_path: PathBuf,
    checkpoints: RwLock<HashMap<GenomeId, PathBuf>>,
    deltas: RwLock<HashMap<GenomeId, Vec<PathBuf>>>,
}

impl MmapStorage {
    pub fn new(base_path: &Path) -> std::io::Result<Self> {
        std::fs::create_dir_all(base_path.join("checkpoints"))?;
        std::fs::create_dir_all(base_path.join("deltas"))?;
        
        let mut storage = Self {
            base_path: base_path.to_path_buf(),
            checkpoints: RwLock::new(HashMap::new()),
            deltas: RwLock::new(HashMap::new()),
        };
        
        storage.scan_existing_files()?;
        Ok(storage)
    }
    
    fn scan_existing_files(&self) -> std::io::Result<()> {
        // Scan checkpoints
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
        
        // Scan deltas
        if let Ok(entries) = std::fs::read_dir(self.base_path.join("deltas")) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    if let Ok(id) = name.split('_').next().and_then(|s| s.parse::<u64>()) {
                        self.deltas.write().entry(id).or_insert_with(Vec::new).push(path);
                    }
                }
            }
        }
        
        Ok(())
    }
    
    fn checkpoint_path(&self, genome_id: GenomeId) -> PathBuf {
        self.base_path.join("checkpoints").join(format!("{}.bin", genome_id))
    }
    
    fn delta_path(&self, genome_id: GenomeId, seq: u32) -> PathBuf {
        self.base_path.join("deltas").join(format!("{}_{:08x}.delta", genome_id, seq))
    }
    
    /// Sauvegarde un genome en checkpoint
    pub fn save_checkpoint(&self, genome: &Genome) -> std::io::Result<()> {
        let path = self.checkpoint_path(genome.id);
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)?;
        
        let mut writer = std::io::BufWriter::new(file);
        let header = FileHeader::new(FileType::Checkpoint);
        header.write(&mut writer)?;
        genome.serialize(&mut writer)?;
        writer.flush()?;
        
        self.checkpoints.write().insert(genome.id, path);
        Ok(())
    }
    
    /// Sauvegarde un delta
    pub fn save_delta(&self, delta: &Delta, seq: u32) -> std::io::Result<()> {
        let path = self.delta_path(delta.genome_id, seq);
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)?;
        
        let mut writer = std::io::BufWriter::new(file);
        delta.serialize(&mut writer)?;
        writer.flush()?;
        
        self.deltas.write()
            .entry(delta.genome_id)
            .or_insert_with(Vec::new)
            .push(path);
        
        Ok(())
    }
    
    /// Charge un genome, en reconstruisant depuis checkpoint + deltas
    pub fn load_genome(&self, genome_id: GenomeId) -> std::io::Result<Genome> {
        // D'abord, chercher le checkpoint le plus récent
        let checkpoint_path = self.checkpoints.read().get(&genome_id).cloned();
        
        let mut genome = if let Some(path) = checkpoint_path {
            let file = File::open(path)?;
            let reader = std::io::BufReader::new(file);
            Genome::deserialize(&mut reader.bytes().map(|b| b.unwrap()).collect::<Vec<u8>>().as_slice())?
        } else {
            // Pas de checkpoint, chercher un parent
            return Err(std::io::Error::new(std::io::ErrorKind::NotFound, "Genome not found"));
        };
        
        // Appliquer les deltas
        let deltas = self.deltas.read().get(&genome_id).cloned();
        if let Some(delta_paths) = deltas {
            let mut sorted_deltas: Vec<_> = delta_paths.iter()
                .filter_map(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .and_then(|s| s.split('_').nth(1))
                        .and_then(|s| s.strip_suffix(".delta"))
                        .and_then(|s| u32::from_str_radix(s, 16).ok())
                        .map(|seq| (seq, p))
                })
                .collect();
            sorted_deltas.sort_by_key(|(seq, _)| *seq);
            
            for (_, path) in sorted_deltas {
                let file = File::open(path)?;
                let mut reader = std::io::BufReader::new(file);
                let mut data = Vec::new();
                reader.read_to_end(&mut data)?;
                let delta = Delta::deserialize(&mut data.as_slice())?;
                Self::apply_delta(&mut genome, &delta)?;
            }
        }
        
        Ok(genome)
    }
    
    fn apply_delta(genome: &mut Genome, delta: &Delta) -> std::io::Result<()> {
        for op in &delta.operations {
            match op {
                DeltaOperation::SubstituteGene { gene_id, new_value, .. } => {
                    genome.set_gene_value(*gene_id, new_value.clone());
                }
                DeltaOperation::InsertGene { chromosome_id, gene, position: _ } => {
                    genome.genes.insert(gene.id, gene.clone());
                    if let Some(chromo) = genome.chromosomes.iter_mut().find(|c| c.id == *chromosome_id) {
                        chromo.add_gene(gene.id);
                    }
                }
                DeltaOperation::DeleteGene { chromosome_id, gene_id } => {
                    genome.genes.remove(gene_id);
                    if let Some(chromo) = genome.chromosomes.iter_mut().find(|c| c.id == *chromosome_id) {
                        chromo.remove_gene(*gene_id);
                    }
                }
                _ => {} // Autres opérations
            }
        }
        Ok(())
    }
    
    /// Vérifie si un genome existe
    pub fn exists(&self, genome_id: GenomeId) -> bool {
        self.checkpoints.read().contains_key(&genome_id) || self.deltas.read().contains_key(&genome_id)
    }
    
    /// Supprime un genome
    pub fn delete_genome(&self, genome_id: GenomeId) -> std::io::Result<()> {
        if let Some(path) = self.checkpoints.write().remove(&genome_id) {
            let _ = std::fs::remove_file(path);
        }
        
        if let Some(delta_paths) = self.deltas.write().remove(&genome_id) {
            for path in delta_paths {
                let _ = std::fs::remove_file(path);
            }
        }
        
        Ok(())
    }
}