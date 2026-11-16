//! Indexation en mémoire : fitness, métadonnées et similarité génétique.

mod gene_index;
mod shard;
mod signature;

pub use gene_index::GeneIndex;
pub use shard::{GenomeMeta, Shard, SHARD_COUNT};
pub use signature::{GeneticSignature, SignatureGenerator};

use crate::types::{Genome, GenomeId, Value};
use std::cmp::Ordering;

/// Index global réparti sur plusieurs shards.
pub struct GlobalIndex {
    shards: Vec<Shard>,
    signature_gen: SignatureGenerator,
    genes: GeneIndex,
}

impl GlobalIndex {
    /// Crée un index vide.
    pub fn new() -> Self {
        Self {
            shards: (0..SHARD_COUNT).map(|_| Shard::new()).collect(),
            signature_gen: SignatureGenerator::new(),
            genes: GeneIndex::new(),
        }
    }

    fn shard_for(&self, genome_id: GenomeId) -> &Shard {
        &self.shards[(genome_id % SHARD_COUNT as u64) as usize]
    }

    /// Insère ou met à jour les métadonnées d'un genome.
    pub fn upsert(&self, meta: GenomeMeta) {
        self.shard_for(meta.id).upsert(meta);
    }

    /// Métadonnées d'un genome.
    pub fn get_meta(&self, genome_id: GenomeId) -> Option<GenomeMeta> {
        self.shard_for(genome_id).get(genome_id)
    }

    /// Retire un genome de l'index.
    pub fn remove(&self, genome_id: GenomeId) -> Option<GenomeMeta> {
        self.genes.remove(genome_id);
        self.shard_for(genome_id).remove(genome_id)
    }

    /// Indexe (ou réindexe) les gènes d'un genome dans l'index secondaire.
    pub fn index_genes(&self, genome: &Genome) {
        self.genes.insert(genome);
    }

    /// Genomes candidats ayant un gène `name` de valeur `value` (à vérifier par l'appelant).
    pub fn find_gene_candidates(&self, name: &str, value: &Value) -> Vec<GenomeId> {
        self.genes.candidates(name, value)
    }

    /// Les `n` genomes avec le meilleur fitness (du plus au moins fit).
    pub fn top_fitness(&self, n: usize) -> Vec<GenomeId> {
        let mut candidates = Vec::new();
        for shard in &self.shards {
            candidates.extend(shard.top_fitness(n));
        }
        // fitness décroissant, puis identifiant croissant pour un ordre stable
        candidates.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        candidates.truncate(n);
        candidates.into_iter().map(|(_, id)| id).collect()
    }

    /// Genomes similaires à une signature, triés par similarité décroissante.
    pub fn search_similar(
        &self,
        target: GeneticSignature,
        threshold: f32,
        exclude: Option<GenomeId>,
    ) -> Vec<(GenomeId, f32)> {
        let mut results = Vec::new();
        for shard in &self.shards {
            results.extend(shard.similar(target, threshold));
        }
        results.retain(|(id, _)| Some(*id) != exclude);
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal).then(a.0.cmp(&b.0)));
        results
    }

    /// Copie de toutes les métadonnées.
    pub fn all_metas(&self) -> Vec<GenomeMeta> {
        self.shards.iter().flat_map(|s| s.all_metas()).collect()
    }

    /// Nombre total de genomes indexés.
    pub fn len(&self) -> usize {
        self.shards.iter().map(|s| s.len()).sum()
    }

    /// Vrai si l'index est vide.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Générateur de signatures.
    pub fn signature_generator(&self) -> &SignatureGenerator {
        &self.signature_gen
    }
}

impl Default for GlobalIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{FitnessScore, MemoryStatus};

    fn meta(id: u64, fitness: f32, sig: u64) -> GenomeMeta {
        GenomeMeta {
            id,
            parent_id: None,
            generation: 1,
            fitness: FitnessScore::new(fitness),
            signature: GeneticSignature(sig),
            memory_status: MemoryStatus::from_fitness(fitness),
        }
    }

    #[test]
    fn top_fitness_sorts_by_fitness_across_shards() {
        let index = GlobalIndex::new();
        index.upsert(meta(1, 0.2, 0));
        index.upsert(meta(2, 0.9, 0));
        index.upsert(meta(300, 0.5, 0)); // même shard que 44 (300 % 256)
        index.upsert(meta(44, 0.7, 0));
        assert_eq!(index.top_fitness(3), vec![2, 44, 300]);
    }

    #[test]
    fn upsert_replaces_old_fitness_entry() {
        let index = GlobalIndex::new();
        index.upsert(meta(1, 0.2, 0));
        index.upsert(meta(1, 0.8, 0));
        assert_eq!(index.len(), 1);
        assert_eq!(index.top_fitness(5), vec![1]);
        assert!(index.remove(1).is_some());
        assert!(index.top_fitness(5).is_empty());
    }

    #[test]
    fn similarity_search_filters_and_excludes() {
        let index = GlobalIndex::new();
        index.upsert(meta(1, 0.5, 0b0000));
        index.upsert(meta(2, 0.5, 0b0001)); // 1 bit d'écart
        index.upsert(meta(3, 0.5, u64::MAX)); // très éloigné
        let found = index.search_similar(GeneticSignature(0), 0.9, Some(1));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, 2);
    }
}
