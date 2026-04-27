//! Interface principale de DNA DB

use crate::types::*;
use crate::mutation::{Mutation, MutationEngine};
use crate::fitness::{FitnessCalculator, FitnessConfig};
use crate::storage::{SmartStorage, WriteAheadLog, WalEntry};
use crate::cache::LRUCache;
use crate::index::{GlobalIndex, GeneticSignature};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use parking_lot::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// Configuration de la base de données
pub struct DNADatabaseConfig {
    pub data_dir: String,
    pub cache_size: usize,
    pub mmap_threshold_bytes: u64,
    pub wal_max_file_size: u64,
    pub auto_checkpoint_delta_count: u32,
    pub fitness_config: FitnessConfig,
}

impl Default for DNADatabaseConfig {
    fn default() -> Self {
        Self {
            data_dir: "dna_data".to_string(),
            cache_size: 1000,
            mmap_threshold_bytes: 1024 * 1024, // 1 MB
            wal_max_file_size: 1024 * 1024 * 10, // 10 MB
            auto_checkpoint_delta_count: 90,
            fitness_config: FitnessConfig::default(),
        }
    }
}

/// Base de données DNA DB
pub struct DNADatabase {
    config: DNADatabaseConfig,
    storage: SmartStorage,
    wal: WriteAheadLog,
    cache: LRUCache<GenomeId, Genome>,
    index: GlobalIndex,
    mutation_engine: RwLock<MutationEngine>,
    fitness_calculator: FitnessCalculator,
    wal_sequence: AtomicU64,
}

impl DNADatabase {
    /// Crée une nouvelle base de données
    pub fn new(config: DNADatabaseConfig) -> std::io::Result<Self> {
        let base_path = Path::new(&config.data_dir);
        std::fs::create_dir_all(base_path)?;
        
        let wal_path = base_path.join("wal");
        std::fs::create_dir_all(&wal_path)?;
        
        let storage = SmartStorage::new(base_path, config.mmap_threshold_bytes)?;
        let wal = WriteAheadLog::new(&wal_path, config.wal_max_file_size)?;
        
        let db = Self {
            config,
            storage,
            wal,
            cache: LRUCache::new(1000),
            index: GlobalIndex::new(),
            mutation_engine: RwLock::new(MutationEngine::new()),
            fitness_calculator: FitnessCalculator::with_defaults(),
            wal_sequence: AtomicU64::new(0),
        };
        
        Ok(db)
    }
    
    /// Builder pattern
    pub fn builder() -> DNADatabaseBuilder {
        DNADatabaseBuilder::new()
    }
    
    /// Crée un nouveau genome
    pub fn create_genome(&self, chromosomes: Vec<Chromosome>, genes: Vec<Gene>) -> std::io::Result<GenomeId> {
        let id = self.mutation_engine.write().next_id();
        let genome = Genome::new(id, chromosomes, genes);
        
        self.storage.save_genome(&genome)?;
        self.index.insert(genome.clone());
        self.cache.insert(id, genome);
        
        Ok(id)
    }
    
    /// Applique une mutation
    pub fn mutate(&self, mutation: Mutation) -> std::io::Result<GenomeId> {
        let genome_id = mutation.genome_id()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "Mutation has no genome_id"))?;
        
        let parent = self.get_genome(genome_id)?
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "Parent genome not found"))?;
        
        let (child, delta) = self.mutation_engine.write().apply_mutation(&parent, &mutation);
        
        // Sauvegarder le delta
        let delta_seq = self.mutation_engine.read().next_id() as u32;
        self.storage.save_genome(&child)?;
        
        // Enregistrer dans le WAL
        let entry = WalEntry {
            sequence: self.wal_sequence.fetch_add(1, Ordering::Relaxed),
            timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
            mutation,
            result_id: child.id,
        };
        self.wal.append(entry)?;
        
        // Mettre à jour les index et cache
        self.index.insert(child.clone());
        self.cache.insert(child.id, child);
        
        // Vérifier checkpoint automatique
        // (simplifié pour l'instant)
        
        Ok(child.id)
    }
    
    /// Récupère un genome
    pub fn get_genome(&self, genome_id: GenomeId) -> std::io::Result<Option<Genome>> {
        // Vérifier le cache
        if let Some(cached) = self.cache.get(&genome_id) {
            let mut genome = (*cached).clone();
            genome.record_access();
            return Ok(Some(genome));
        }
        
        // Charger depuis le stockage
        if self.storage.exists(genome_id) {
            let mut genome = self.storage.load_genome(genome_id)?;
            genome.record_access();
            
            // Mettre à jour le fitness
            self.fitness_calculator.update(&mut genome, None);
            
            // Mettre en cache
            self.cache.insert(genome_id, genome.clone());
            self.index.update(genome.clone());
            
            Ok(Some(genome))
        } else {
            Ok(None)
        }
    }
    
    /// Recherche par similarité génétique
    pub fn search_similar(&self, target_id: GenomeId, threshold: f32) -> std::io::Result<Vec<GenomeId>> {
        let target = match self.get_genome(target_id)? {
            Some(g) => g,
            None => return Ok(Vec::new()),
        };
        
        let target_sig = self.index.signature_generator().signature(&target);
        let mut results = Vec::new();
        
        // Parcourir tous les shards (simplifié, à optimiser avec index)
        // Pour un vrai système, on utiliserait un index LSH
        
        Ok(results)
    }
    
    /// Met à jour le fitness d'un genome
    pub fn update_fitness(&self, genome_id: GenomeId, query_match_score: Option<f32>) -> std::io::Result<()> {
        if let Some(mut genome) = self.get_genome(genome_id)? {
            self.fitness_calculator.update(&mut genome, query_match_score);
            self.storage.save_genome(&genome)?;
            self.index.update(genome);
            self.cache.insert(genome_id, genome);
        }
        Ok(())
    }
    
    /// Retourne les genomes avec le meilleur fitness
    pub fn top_genomes(&self, n: usize) -> Vec<GenomeId> {
        self.index.top_fitness(n)
    }
    
    /// Supprime un genome
    pub fn delete_genome(&self, genome_id: GenomeId) -> std::io::Result<()> {
        self.storage.delete_genome(genome_id)?;
        self.index.remove(genome_id);
        self.cache.remove(&genome_id);
        Ok(())
    }
    
    /// Nettoie les fossiles (fitness < 0.3)
    pub fn gc(&self) -> std::io::Result<usize> {
        let mut removed = 0;
        let top = self.index.top_fitness(10000);
        
        // Cette implémentation est simplifiée
        // Dans un vrai système, on parcourrait tous les genomes
        // et on supprimerait ceux avec fitness < 0.3
        
        Ok(removed)
    }
}

/// Builder pour DNADatabase
pub struct DNADatabaseBuilder {
    config: DNADatabaseConfig,
}

impl DNADatabaseBuilder {
    pub fn new() -> Self {
        Self {
            config: DNADatabaseConfig::default(),
        }
    }
    
    pub fn data_dir(mut self, dir: &str) -> Self {
        self.config.data_dir = dir.to_string();
        self
    }
    
    pub fn cache_size(mut self, size: usize) -> Self {
        self.config.cache_size = size;
        self
    }
    
    pub fn mmap_threshold(mut self, bytes: u64) -> Self {
        self.config.mmap_threshold_bytes = bytes;
        self
    }
    
    pub fn auto_checkpoint_delta_count(mut self, count: u32) -> Self {
        self.config.auto_checkpoint_delta_count = count;
        self
    }
    
    pub fn build(self) -> std::io::Result<DNADatabase> {
        DNADatabase::new(self.config)
    }
}

impl Default for DNADatabaseBuilder {
    fn default() -> Self {
        Self::new()
    }
}