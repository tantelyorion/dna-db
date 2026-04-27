//! Système d'indexation pour recherche rapide

mod shard;
mod signature;

pub use shard::{Shard, SHARD_COUNT};
pub use signature::{GeneticSignature, SignatureGenerator};

use crate::types::GenomeId;
use std::sync::Arc;

/// Index global réparti sur plusieurs shards
pub struct GlobalIndex {
    shards: Vec<Arc<Shard>>,
    signature_gen: SignatureGenerator,
}

impl GlobalIndex {
    pub fn new() -> Self {
        let mut shards = Vec::with_capacity(SHARD_COUNT);
        for _ in 0..SHARD_COUNT {
            shards.push(Arc::new(Shard::new()));
        }
        
        Self {
            shards,
            signature_gen: SignatureGenerator::new(),
        }
    }
    
    /// Détermine le shard pour un genome_id
    fn shard_for(&self, genome_id: GenomeId) -> &Arc<Shard> {
        &self.shards[(genome_id as usize) % SHARD_COUNT]
    }
    
    /// Insère un genome
    pub fn insert(&self, genome: crate::types::Genome) {
        self.shard_for(genome.id).insert(genome);
    }
    
    /// Récupère un genome
    pub fn get(&self, genome_id: GenomeId) -> Option<crate::types::Genome> {
        self.shard_for(genome_id).get(genome_id)
    }
    
    /// Met à jour un genome
    pub fn update(&self, genome: crate::types::Genome) {
        self.shard_for(genome.id).update(genome);
    }
    
    /// Supprime un genome
    pub fn remove(&self, genome_id: GenomeId) -> Option<crate::types::Genome> {
        self.shard_for(genome_id).remove(genome_id)
    }
    
    /// Retourne les N meilleurs genomes
    pub fn top_fitness(&self, n: usize) -> Vec<GenomeId> {
        let mut all_results = Vec::new();
        
        for shard in &self.shards {
            all_results.extend(shard.top_fitness(n));
        }
        
        all_results.sort_by_key(|id| *id);
        all_results.truncate(n);
        all_results
    }
    
    /// Retourne le générateur de signatures
    pub fn signature_generator(&self) -> &SignatureGenerator {
        &self.signature_gen
    }
}

impl Default for GlobalIndex {
    fn default() -> Self {
        Self::new()
    }
}