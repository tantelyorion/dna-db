//! Signature génétique (SimHash 64 bits) pour la recherche par similarité.
//!
//! Deux genomes de structure et de contenu proches ont des signatures proches
//! (distance de Hamming faible). La signature ne dépend ni des identifiants ni
//! de l'ordre d'insertion en base : uniquement des noms, valeurs et poids.

use crate::types::{Genome, Value};
use crate::util::hash64;

/// Signature génétique de 64 bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GeneticSignature(pub u64);

impl GeneticSignature {
    /// Nombre de bits qui diffèrent entre deux signatures.
    pub fn hamming_distance(self, other: GeneticSignature) -> u32 {
        (self.0 ^ other.0).count_ones()
    }

    /// Similarité entre 0.0 (opposées) et 1.0 (identiques).
    pub fn similarity(self, other: GeneticSignature) -> f32 {
        1.0 - self.hamming_distance(other) as f32 / 64.0
    }
}

/// Générateur de signatures.
pub struct SignatureGenerator;

impl SignatureGenerator {
    /// Crée un générateur.
    pub fn new() -> Self {
        Self
    }

    /// Calcule la signature d'un genome.
    pub fn signature(&self, genome: &Genome) -> GeneticSignature {
        let mut acc = [0f32; 64];

        let mut add = |feature: &[u8], weight: f32| {
            let h = hash64(feature);
            for (bit, slot) in acc.iter_mut().enumerate() {
                if (h >> bit) & 1 == 1 {
                    *slot += weight;
                } else {
                    *slot -= weight;
                }
            }
        };

        let mut listed = std::collections::HashSet::new();
        for chromosome in &genome.chromosomes {
            add(format!("C:{}", chromosome.name).as_bytes(), 1.0);
            for gene_id in &chromosome.genes {
                if let Some(gene) = genome.get_gene(*gene_id) {
                    listed.insert(gene.id);
                    add_gene(&mut add, &chromosome.name, gene.name.as_str(), &gene.value, gene.weight);
                }
            }
        }

        // Gènes orphelins (hors chromosome), dans l'ordre des identifiants
        let mut orphans: Vec<_> = genome.genes.values().filter(|g| !listed.contains(&g.id)).collect();
        orphans.sort_by_key(|g| g.id);
        for gene in orphans {
            add_gene(&mut add, "", gene.name.as_str(), &gene.value, gene.weight);
        }

        let mut sig = 0u64;
        for (bit, slot) in acc.iter().enumerate() {
            if *slot > 0.0 {
                sig |= 1u64 << bit;
            }
        }
        GeneticSignature(sig)
    }

    /// Similarité entre deux signatures (0.0 à 1.0).
    pub fn similarity(&self, a: GeneticSignature, b: GeneticSignature) -> f32 {
        a.similarity(b)
    }
}

fn add_gene(add: &mut impl FnMut(&[u8], f32), chromosome_name: &str, gene_name: &str, value: &Value, weight: f32) {
    add(format!("G:{}/{}", chromosome_name, gene_name).as_bytes(), 0.5 + weight);

    let mut feature = format!("V:{}/{}=", chromosome_name, gene_name).into_bytes();
    // Écrire dans un Vec ne peut pas échouer
    let _ = value.serialize(&mut feature);
    add(&feature, weight.max(0.1));
}

impl Default for SignatureGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Chromosome, Gene};

    fn genome(id: u64, age: i64) -> Genome {
        let genes = vec![
            Gene::new(1, "name".to_string(), Value::String("Alice".to_string()), 0.8),
            Gene::new(2, "age".to_string(), Value::Int(age), 0.6),
        ];
        let chromosome = Chromosome::new(1, "profile".to_string(), vec![1, 2], 0.01);
        Genome::try_new(id, vec![chromosome], genes).unwrap()
    }

    #[test]
    fn identical_content_gives_identical_signature_regardless_of_id() {
        let generator = SignatureGenerator::new();
        let a = generator.signature(&genome(1, 30));
        let b = generator.signature(&genome(2, 30));
        assert_eq!(a, b);
        assert_eq!(a.similarity(b), 1.0);
    }

    #[test]
    fn similarity_is_symmetric_and_bounded() {
        let generator = SignatureGenerator::new();
        let a = generator.signature(&genome(1, 30));
        let b = generator.signature(&genome(2, 31));
        let s = a.similarity(b);
        assert!((0.0..=1.0).contains(&s));
        assert_eq!(s, b.similarity(a));
    }
}
