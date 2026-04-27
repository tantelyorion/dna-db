//! Sharding pour le multi-threading

use crate::types::*;
use crate::fitness::FitnessScore;
use parking_lot::RwLock;
use std::collections::{HashMap, BTreeMap};

/// Nombre de shards (puissance de 2 pour modulo rapide)
pub const SHARD_COUNT: usize = 256;

/// Un shard contenant une partie des genomes
pub struct Shard {
    /// Maps genome_id → genome (peut être None si pas en mémoire)
    genomes: RwLock<HashMap<GenomeId, Option<Genome>>>,
    /// Index fitness trié
    fitness_index: RwLock<BTreeMap<FitnessScore, Vec<GenomeId>>>,
    /// Index générationnel
    generation_index: RwLock<Vec<GenomeId>>,
}

impl Shard {
    pub fn new() -> Self {
        Self {
            genomes: RwLock::new(HashMap::new()),
            fitness_index: RwLock::new(BTreeMap::new()),
            generation_index: RwLock::new(Vec::new()),
        }
    }
    
    /// Insère un genome dans le shard
    pub fn insert(&self, genome: Genome) {
        let fitness = genome.fitness;
        let genome_id = genome.id;
        let generation = genome.generation as usize;
        
        self.genomes.write().insert(genome_id, Some(genome));
        self.fitness_index.write()
            .entry(fitness)
            .or_insert_with(Vec::new)
            .push(genome_id);
        
        let mut gen_idx = self.generation_index.write();
        if generation >= gen_idx.len() {
            gen_idx.resize(generation + 1, 0);
        }
        gen_idx[generation] = genome_id;
    }
    
    /// Récupère un genome
    pub fn get(&self, genome_id: GenomeId) -> Option<Genome> {
        self.genomes.read()
            .get(&genome_id)
            .and_then(|opt| opt.clone())
    }
    
    /// Met à jour un genome
    pub fn update(&self, genome: Genome) {
        let genome_id = genome.id;
        let fitness = genome.fitness;
        
        // Supprimer l'ancienne entrée dans l'index fitness
        if let Some(old) = self.genomes.write().insert(genome_id, Some(genome)) {
            if let Some(old_genome) = old {
                if let Some(vec) = self.fitness_index.write().get_mut(&old_genome.fitness) {
                    vec.retain(|&id| id != genome_id);
                }
            }
        }
        
        // Ajouter la nouvelle entrée
        self.fitness_index.write()
            .entry(fitness)
            .or_insert_with(Vec::new)
            .push(genome_id);
    }
    
    /// Supprime un genome
    pub fn remove(&self, genome_id: GenomeId) -> Option<Genome> {
        if let Some(genome_opt) = self.genomes.write().remove(&genome_id) {
            if let Some(genome) = genome_opt {
                if let Some(vec) = self.fitness_index.write().get_mut(&genome.fitness) {
                    vec.retain(|&id| id != genome_id);
                }
                return Some(genome);
            }
        }
        None
    }
    
    /// Retourne les genomes avec le plus haut fitness
    pub fn top_fitness(&self, n: usize) -> Vec<GenomeId> {
        let mut result = Vec::new();
        let fitness_idx = self.fitness_index.read();
        
        for (_fitness, ids) in fitness_idx.iter().rev() {
            for &id in ids {
                result.push(id);
                if result.len() >= n {
                    return result;
                }
            }
        }
        
        result
    }
    
    /// Nombre de genomes dans le shard
    pub fn len(&self) -> usize {
        self.genomes.read().len()
    }
}

impl Default for Shard {
    fn default() -> Self {
        Self::new()
    }
}