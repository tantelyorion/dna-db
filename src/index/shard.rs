//! Shards : l'index est réparti pour limiter la contention entre threads.

use super::signature::GeneticSignature;
use crate::types::{FitnessScore, GenomeId, MemoryStatus};
use parking_lot::RwLock;
use std::collections::{BTreeSet, HashMap};

/// Nombre de shards.
pub const SHARD_COUNT: usize = 256;

/// Métadonnées légères d'un genome : l'index ne conserve plus de copie du genome entier.
#[derive(Debug, Clone, Copy)]
pub struct GenomeMeta {
    /// Identifiant
    pub id: GenomeId,
    /// Parent éventuel
    pub parent_id: Option<GenomeId>,
    /// Génération
    pub generation: u64,
    /// Fitness
    pub fitness: FitnessScore,
    /// Signature de similarité
    pub signature: GeneticSignature,
    /// Statut mémoire
    pub memory_status: MemoryStatus,
}

struct ShardInner {
    metas: HashMap<GenomeId, GenomeMeta>,
    by_fitness: BTreeSet<(FitnessScore, GenomeId)>,
}

/// Un shard : métadonnées + index trié par fitness, sous un seul verrou (toujours cohérents).
pub struct Shard {
    inner: RwLock<ShardInner>,
}

impl Shard {
    /// Crée un shard vide.
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(ShardInner {
                metas: HashMap::new(),
                by_fitness: BTreeSet::new(),
            }),
        }
    }

    /// Insère ou remplace les métadonnées d'un genome.
    pub fn upsert(&self, meta: GenomeMeta) {
        let mut inner = self.inner.write();
        if let Some(old) = inner.metas.insert(meta.id, meta) {
            inner.by_fitness.remove(&(old.fitness, old.id));
        }
        inner.by_fitness.insert((meta.fitness, meta.id));
    }

    /// Récupère les métadonnées d'un genome.
    pub fn get(&self, genome_id: GenomeId) -> Option<GenomeMeta> {
        self.inner.read().metas.get(&genome_id).copied()
    }

    /// Supprime un genome de l'index.
    pub fn remove(&self, genome_id: GenomeId) -> Option<GenomeMeta> {
        let mut inner = self.inner.write();
        let old = inner.metas.remove(&genome_id)?;
        inner.by_fitness.remove(&(old.fitness, old.id));
        Some(old)
    }

    /// Les `n` meilleurs genomes du shard, du plus au moins fit.
    pub fn top_fitness(&self, n: usize) -> Vec<(FitnessScore, GenomeId)> {
        self.inner.read().by_fitness.iter().rev().take(n).copied().collect()
    }

    /// Genomes dont la signature est au moins `threshold`-similaire à `target`.
    pub fn similar(&self, target: GeneticSignature, threshold: f32) -> Vec<(GenomeId, f32)> {
        self.inner
            .read()
            .metas
            .values()
            .filter_map(|m| {
                let s = target.similarity(m.signature);
                if s >= threshold {
                    Some((m.id, s))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Copie de toutes les métadonnées du shard.
    pub fn all_metas(&self) -> Vec<GenomeMeta> {
        self.inner.read().metas.values().copied().collect()
    }

    /// Nombre de genomes dans le shard.
    pub fn len(&self) -> usize {
        self.inner.read().metas.len()
    }

    /// Vrai si le shard est vide.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for Shard {
    fn default() -> Self {
        Self::new()
    }
}
