//! Index secondaire : (nom de gène, valeur) -> genomes. Accélère `find_by_gene`.
//!
//! La valeur est représentée par un hash 64 bits : l'index renvoie des *candidats* que
//! l'appelant doit vérifier (une collision de hash reste théoriquement possible).

use crate::types::{Genome, GenomeId, Value};
use crate::util::hash64;
use parking_lot::RwLock;
use std::collections::{HashMap, HashSet};

type Key = (String, u64);

fn value_hash(value: &Value) -> u64 {
    let mut buf = Vec::new();
    // Écrire dans un Vec ne peut pas échouer
    let _ = value.serialize(&mut buf);
    hash64(&buf)
}

#[derive(Default)]
struct Inner {
    forward: HashMap<Key, HashSet<GenomeId>>,
    reverse: HashMap<GenomeId, Vec<Key>>,
}

impl Inner {
    fn remove_genome(&mut self, genome_id: GenomeId) {
        if let Some(keys) = self.reverse.remove(&genome_id) {
            for key in keys {
                let now_empty = match self.forward.get_mut(&key) {
                    Some(set) => {
                        set.remove(&genome_id);
                        set.is_empty()
                    }
                    None => false,
                };
                if now_empty {
                    self.forward.remove(&key);
                }
            }
        }
    }
}

/// Index secondaire des gènes.
pub struct GeneIndex {
    inner: RwLock<Inner>,
}

impl GeneIndex {
    /// Crée un index vide.
    pub fn new() -> Self {
        Self { inner: RwLock::new(Inner::default()) }
    }

    /// Indexe (ou réindexe) tous les gènes d'un genome.
    pub fn insert(&self, genome: &Genome) {
        let mut keys: Vec<Key> = genome
            .genes
            .values()
            .map(|g| (g.name.clone(), value_hash(&g.value)))
            .collect();
        keys.sort();
        keys.dedup();

        let mut inner = self.inner.write();
        inner.remove_genome(genome.id);
        for key in &keys {
            inner.forward.entry(key.clone()).or_default().insert(genome.id);
        }
        inner.reverse.insert(genome.id, keys);
    }

    /// Retire un genome de l'index.
    pub fn remove(&self, genome_id: GenomeId) {
        self.inner.write().remove_genome(genome_id);
    }

    /// Genomes candidats (triés) ayant un gène `name` de valeur `value`.
    pub fn candidates(&self, name: &str, value: &Value) -> Vec<GenomeId> {
        let key = (name.to_string(), value_hash(value));
        let inner = self.inner.read();
        let mut ids: Vec<GenomeId> = match inner.forward.get(&key) {
            Some(set) => set.iter().copied().collect(),
            None => Vec::new(),
        };
        ids.sort_unstable();
        ids
    }

    /// Nombre de genomes indexés.
    pub fn len(&self) -> usize {
        self.inner.read().reverse.len()
    }

    /// Vrai si l'index est vide.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for GeneIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Chromosome, Gene};

    fn genome(id: u64, name: &str) -> Genome {
        let genes = vec![Gene::new(1, "name".to_string(), Value::from(name), 0.5)];
        let chromosome = Chromosome::new(1, "c".to_string(), vec![1], 0.0);
        Genome::try_new(id, vec![chromosome], genes).unwrap()
    }

    #[test]
    fn insert_reindex_and_remove() {
        let index = GeneIndex::new();
        index.insert(&genome(1, "Alice"));
        index.insert(&genome(2, "Alice"));
        index.insert(&genome(3, "Bob"));
        assert_eq!(index.candidates("name", &Value::from("Alice")), vec![1, 2]);

        // réindexer le genome 1 avec une autre valeur retire l'ancienne entrée
        index.insert(&genome(1, "Bob"));
        assert_eq!(index.candidates("name", &Value::from("Alice")), vec![2]);
        assert_eq!(index.candidates("name", &Value::from("Bob")), vec![1, 3]);

        index.remove(2);
        assert!(index.candidates("name", &Value::from("Alice")).is_empty());
        assert_eq!(index.len(), 2);
    }
}
