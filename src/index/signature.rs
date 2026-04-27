//! Signature génétique pour recherche par similarité

use crate::types::*;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Signature génétique (16 bits pour commencer)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeneticSignature(pub u16);

/// Générateur de signatures
pub struct SignatureGenerator {
    seeds: [u64; 16],
}

impl SignatureGenerator {
    pub fn new() -> Self {
        let mut seeds = [0u64; 16];
        for (i, seed) in seeds.iter_mut().enumerate() {
            *seed = i as u64 * 0x9e3779b97f4a7c15;
        }
        Self { seeds }
    }
    
    /// Génère une signature pour un genome
    pub fn signature(&self, genome: &Genome) -> GeneticSignature {
        let mut sig = 0u16;
        
        for (i, seed) in self.seeds.iter().enumerate() {
            let hash = self.hash_genome_with_seed(genome, *seed);
            if hash & 1 == 1 {
                sig |= 1 << i;
            }
        }
        
        GeneticSignature(sig)
    }
    
    fn hash_genome_with_seed(&self, genome: &Genome, seed: u64) -> u64 {
        let mut hasher = DefaultHasher::new();
        seed.hash(&mut hasher);
        
        for chromosome in &genome.chromosomes {
            chromosome.id.hash(&mut hasher);
            chromosome.name.hash(&mut hasher);
            for gene_id in &chromosome.genes {
                gene_id.hash(&mut hasher);
                if let Some(gene) = genome.get_gene(*gene_id) {
                    gene.name.hash(&mut hasher);
                    // Hash de la valeur simplifié
                    match &gene.value {
                        Value::Int(v) => v.hash(&mut hasher),
                        Value::Float(v) => v.to_bits().hash(&mut hasher),
                        Value::String(s) => s.hash(&mut hasher),
                        Value::Bool(b) => b.hash(&mut hasher),
                        _ => {}
                    }
                }
            }
        }
        
        hasher.finish()
    }
    
    /// Calcule la similarité entre deux signatures (0.0 à 1.0)
    pub fn similarity(&self, a: GeneticSignature, b: GeneticSignature) -> f32 {
        let xor = a.0 ^ b.0;
        let same_bits = 16 - xor.count_ones();
        same_bits as f32 / 16.0
    }
}

impl Default for SignatureGenerator {
    fn default() -> Self {
        Self::new()
    }
}