//! Moteur de mutations : transforme une demande de mutation en un nouveau genome + delta.

use super::delta::{Delta, DeltaOperation};
use crate::types::{Chromosome, ChromosomeId, Gene, GeneId, Genome, GenomeId, Value};
use crate::util::{
    invalid_data, invalid_input, not_found, read_opt_pos, read_u32, read_u64, read_u8, time_seed,
    write_count, write_opt_pos, SplitMix64,
};
use parking_lot::Mutex;
use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};

/// Mutation demandée sur un genome existant.
#[derive(Debug, Clone, PartialEq)]
pub enum Mutation {
    /// Substitution de la valeur d'un gène
    SubstituteGene {
        /// Genome parent
        genome_id: GenomeId,
        /// Chromosome contenant le gène
        chromosome_id: ChromosomeId,
        /// Gène modifié
        gene_id: GeneId,
        /// Nouvelle valeur
        new_value: Value,
    },
    /// Insertion d'un gène dans un chromosome
    InsertGene {
        /// Genome parent
        genome_id: GenomeId,
        /// Chromosome cible
        chromosome_id: ChromosomeId,
        /// Gène inséré
        gene: Gene,
        /// Position (fin si `None`)
        position: Option<usize>,
    },
    /// Suppression d'un gène
    DeleteGene {
        /// Genome parent
        genome_id: GenomeId,
        /// Chromosome contenant le gène
        chromosome_id: ChromosomeId,
        /// Gène supprimé
        gene_id: GeneId,
    },
    /// Insertion d'un chromosome complet
    InsertChromosome {
        /// Genome parent
        genome_id: GenomeId,
        /// Chromosome inséré
        chromosome: Chromosome,
        /// Gènes du chromosome
        genes: Vec<Gene>,
        /// Position (fin si `None`)
        position: Option<usize>,
    },
    /// Suppression d'un chromosome et de ses gènes
    DeleteChromosome {
        /// Genome parent
        genome_id: GenomeId,
        /// Chromosome supprimé
        chromosome_id: ChromosomeId,
    },
    /// Aucune mutation
    Noop,
}

impl Mutation {
    /// Genome visé (`None` pour `Noop`).
    pub fn genome_id(&self) -> Option<GenomeId> {
        match self {
            Mutation::SubstituteGene { genome_id, .. }
            | Mutation::InsertGene { genome_id, .. }
            | Mutation::DeleteGene { genome_id, .. }
            | Mutation::InsertChromosome { genome_id, .. }
            | Mutation::DeleteChromosome { genome_id, .. } => Some(*genome_id),
            Mutation::Noop => None,
        }
    }

    /// Vrai pour `Noop`.
    pub fn is_noop(&self) -> bool {
        matches!(self, Mutation::Noop)
    }

    /// Sérialise la mutation (utilisé par le WAL).
    pub fn serialize<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        match self {
            Mutation::SubstituteGene { genome_id, chromosome_id, gene_id, new_value } => {
                writer.write_all(&[0u8])?;
                writer.write_all(&genome_id.to_le_bytes())?;
                writer.write_all(&chromosome_id.to_le_bytes())?;
                writer.write_all(&gene_id.to_le_bytes())?;
                new_value.serialize(writer)?;
            }
            Mutation::InsertGene { genome_id, chromosome_id, gene, position } => {
                writer.write_all(&[1u8])?;
                writer.write_all(&genome_id.to_le_bytes())?;
                writer.write_all(&chromosome_id.to_le_bytes())?;
                write_opt_pos(writer, *position)?;
                gene.serialize(writer)?;
            }
            Mutation::DeleteGene { genome_id, chromosome_id, gene_id } => {
                writer.write_all(&[2u8])?;
                writer.write_all(&genome_id.to_le_bytes())?;
                writer.write_all(&chromosome_id.to_le_bytes())?;
                writer.write_all(&gene_id.to_le_bytes())?;
            }
            Mutation::InsertChromosome { genome_id, chromosome, genes, position } => {
                writer.write_all(&[3u8])?;
                writer.write_all(&genome_id.to_le_bytes())?;
                write_opt_pos(writer, *position)?;
                chromosome.serialize(writer)?;
                write_count(writer, genes.len())?;
                for gene in genes {
                    gene.serialize(writer)?;
                }
            }
            Mutation::DeleteChromosome { genome_id, chromosome_id } => {
                writer.write_all(&[4u8])?;
                writer.write_all(&genome_id.to_le_bytes())?;
                writer.write_all(&chromosome_id.to_le_bytes())?;
            }
            Mutation::Noop => writer.write_all(&[5u8])?,
        }
        Ok(())
    }

    /// Désérialise une mutation.
    pub fn deserialize<R: Read>(reader: &mut R) -> io::Result<Self> {
        match read_u8(reader)? {
            0 => {
                let genome_id = read_u64(reader)?;
                let chromosome_id = read_u64(reader)?;
                let gene_id = read_u64(reader)?;
                let new_value = Value::deserialize(reader)?;
                Ok(Mutation::SubstituteGene { genome_id, chromosome_id, gene_id, new_value })
            }
            1 => {
                let genome_id = read_u64(reader)?;
                let chromosome_id = read_u64(reader)?;
                let position = read_opt_pos(reader)?;
                let gene = Gene::deserialize(reader)?;
                Ok(Mutation::InsertGene { genome_id, chromosome_id, gene, position })
            }
            2 => {
                let genome_id = read_u64(reader)?;
                let chromosome_id = read_u64(reader)?;
                let gene_id = read_u64(reader)?;
                Ok(Mutation::DeleteGene { genome_id, chromosome_id, gene_id })
            }
            3 => {
                let genome_id = read_u64(reader)?;
                let position = read_opt_pos(reader)?;
                let chromosome = Chromosome::deserialize(reader)?;
                let count = read_u32(reader)? as usize;
                let mut genes = Vec::with_capacity(count.min(4096));
                for _ in 0..count {
                    genes.push(Gene::deserialize(reader)?);
                }
                Ok(Mutation::InsertChromosome { genome_id, chromosome, genes, position })
            }
            4 => {
                let genome_id = read_u64(reader)?;
                let chromosome_id = read_u64(reader)?;
                Ok(Mutation::DeleteChromosome { genome_id, chromosome_id })
            }
            5 => Ok(Mutation::Noop),
            _ => Err(invalid_data("Type de mutation inconnu")),
        }
    }
}

/// Moteur de mutations (thread-safe : toutes les méthodes prennent `&self`).
pub struct MutationEngine {
    next_id: AtomicU64,
    rng: Mutex<SplitMix64>,
}

impl MutationEngine {
    /// Crée un moteur dont les identifiants commencent à 1.
    pub fn new() -> Self {
        Self::with_start_id(1)
    }

    /// Crée un moteur dont le prochain identifiant sera `start` (au moins 1).
    pub fn with_start_id(start: u64) -> Self {
        Self {
            next_id: AtomicU64::new(start.max(1)),
            rng: Mutex::new(SplitMix64::new(time_seed())),
        }
    }

    /// Alloue un nouvel identifiant unique.
    pub fn next_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Garantit que les prochains identifiants seront strictement supérieurs à `used_id`.
    pub fn reserve_up_to(&self, used_id: u64) {
        self.next_id.fetch_max(used_id.saturating_add(1), Ordering::Relaxed);
    }

    /// Transforme une mutation en opérations de delta (sans toucher au genome).
    fn plan(&self, genome: &Genome, mutation: &Mutation) -> io::Result<Vec<DeltaOperation>> {
        if let Some(target) = mutation.genome_id() {
            if target != genome.id {
                return Err(invalid_input(format!(
                    "La mutation vise le genome {}, pas le {}",
                    target, genome.id
                )));
            }
        }
        let ops = match mutation {
            Mutation::SubstituteGene { chromosome_id, gene_id, new_value, .. } => {
                let old_value = genome
                    .get_gene(*gene_id)
                    .map(|g| g.value.clone())
                    .ok_or_else(|| not_found(format!("Gène {} introuvable", gene_id)))?;
                vec![DeltaOperation::SubstituteGene {
                    chromosome_id: *chromosome_id,
                    gene_id: *gene_id,
                    old_value,
                    new_value: new_value.clone(),
                }]
            }
            Mutation::InsertGene { chromosome_id, gene, position, .. } => {
                vec![DeltaOperation::InsertGene {
                    chromosome_id: *chromosome_id,
                    gene: gene.clone(),
                    position: *position,
                }]
            }
            Mutation::DeleteGene { chromosome_id, gene_id, .. } => {
                vec![DeltaOperation::DeleteGene {
                    chromosome_id: *chromosome_id,
                    gene_id: *gene_id,
                }]
            }
            Mutation::InsertChromosome { chromosome, genes, position, .. } => {
                vec![DeltaOperation::InsertChromosome {
                    chromosome: chromosome.clone(),
                    genes: genes.clone(),
                    position: *position,
                }]
            }
            Mutation::DeleteChromosome { chromosome_id, .. } => {
                vec![DeltaOperation::DeleteChromosome { chromosome_id: *chromosome_id }]
            }
            Mutation::Noop => Vec::new(),
        };
        Ok(ops)
    }

    /// Applique une mutation : retourne le genome enfant et le delta correspondant.
    /// Le genome parent n'est jamais modifié ; une mutation invalide retourne une erreur.
    pub fn apply_mutation(&self, genome: &Genome, mutation: &Mutation) -> io::Result<(Genome, Delta)> {
        self.apply_mutation_with_id(genome, mutation, self.next_id())
    }

    /// Comme [`MutationEngine::apply_mutation`] mais avec un identifiant imposé
    /// (utilisé pour rejouer le WAL).
    pub fn apply_mutation_with_id(
        &self,
        genome: &Genome,
        mutation: &Mutation,
        new_id: GenomeId,
    ) -> io::Result<(Genome, Delta)> {
        if new_id == 0 {
            return Err(invalid_input("L'identifiant de genome 0 est réservé"));
        }
        let ops = self.plan(genome, mutation)?;
        let mut child = genome.child(new_id);
        for op in &ops {
            op.apply(&mut child)?;
        }
        let mut delta = Delta::new(new_id, genome.id, child.generation);
        for op in ops {
            delta.add_operation(op);
        }
        Ok((child, delta))
    }

    /// Génère une mutation aléatoire valide sur un genome (évolution automatique).
    /// Retourne `Mutation::Noop` si le genome n'a aucun chromosome.
    pub fn random_mutation(&self, genome: &Genome) -> Mutation {
        if genome.chromosomes.is_empty() {
            return Mutation::Noop;
        }
        let mut rng = self.rng.lock();
        let chromosome = &genome.chromosomes[rng.below(genome.chromosomes.len())];
        let roll = rng.below(10);
        let gene = if chromosome.genes.is_empty() {
            None
        } else {
            let gene_id = chromosome.genes[rng.below(chromosome.genes.len())];
            genome.get_gene(gene_id)
        };

        match (roll, gene) {
            (0..=4, Some(gene)) => Mutation::SubstituteGene {
                genome_id: genome.id,
                chromosome_id: chromosome.id,
                gene_id: gene.id,
                new_value: mutate_value(&mut rng, &gene.value),
            },
            (8..=9, Some(gene)) => Mutation::DeleteGene {
                genome_id: genome.id,
                chromosome_id: chromosome.id,
                gene_id: gene.id,
            },
            _ => {
                let mut id = self.next_id();
                while genome.genes.contains_key(&id) {
                    id = self.next_id();
                }
                Mutation::InsertGene {
                    genome_id: genome.id,
                    chromosome_id: chromosome.id,
                    gene: Gene::new(id, format!("gene_{}", id), Value::Int(0), 0.5),
                    position: None,
                }
            }
        }
    }
}

/// Variation aléatoire d'une valeur en conservant son type.
fn mutate_value(rng: &mut SplitMix64, value: &Value) -> Value {
    match value {
        Value::Int(v) => Value::Int(v.saturating_add(rng.below(21) as i64 - 10)),
        Value::Float(v) => Value::Float(v + (rng.unit_f64() * 2.0 - 1.0)),
        Value::String(s) => {
            let mut chars: Vec<char> = s.chars().collect();
            if chars.is_empty() {
                Value::String("mutated".to_string())
            } else {
                let idx = rng.below(chars.len());
                chars[idx] = (b'a' + rng.below(26) as u8) as char;
                Value::String(chars.into_iter().collect())
            }
        }
        Value::Bool(b) => Value::Bool(!b),
        Value::Bytes(bytes) => {
            let mut bytes = bytes.clone();
            if bytes.is_empty() {
                bytes.push(rng.below(256) as u8);
            } else {
                let idx = rng.below(bytes.len());
                bytes[idx] ^= (rng.below(255) as u8) + 1;
            }
            Value::Bytes(bytes)
        }
        Value::Null => Value::Int(0),
    }
}

impl Default for MutationEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Genome {
        let genes = vec![
            Gene::new(1, "name".to_string(), Value::String("Alice".to_string()), 0.8),
            Gene::new(2, "age".to_string(), Value::Int(30), 0.6),
        ];
        let chromosome = Chromosome::new(1, "profile".to_string(), vec![1, 2], 0.01);
        Genome::try_new(1, vec![chromosome], genes).unwrap()
    }

    #[test]
    fn mutation_serialization_roundtrip() {
        let mutations = vec![
            Mutation::SubstituteGene { genome_id: 1, chromosome_id: 2, gene_id: 3, new_value: Value::Int(4) },
            Mutation::InsertGene {
                genome_id: 1,
                chromosome_id: 2,
                gene: Gene::new(9, "g".to_string(), Value::Null, 0.5),
                position: Some(1),
            },
            Mutation::DeleteGene { genome_id: 1, chromosome_id: 2, gene_id: 3 },
            Mutation::InsertChromosome {
                genome_id: 1,
                chromosome: Chromosome::new(7, "c".to_string(), vec![8], 0.1),
                genes: vec![Gene::new(8, "x".to_string(), Value::Bool(false), 0.2)],
                position: None,
            },
            Mutation::DeleteChromosome { genome_id: 1, chromosome_id: 7 },
            Mutation::Noop,
        ];
        for m in mutations {
            let mut buf = Vec::new();
            m.serialize(&mut buf).unwrap();
            assert_eq!(Mutation::deserialize(&mut buf.as_slice()).unwrap(), m);
        }
    }

    #[test]
    fn substitute_builds_child_and_delta_without_touching_parent() {
        let engine = MutationEngine::with_start_id(100);
        let parent = base();
        let m = Mutation::SubstituteGene {
            genome_id: 1,
            chromosome_id: 1,
            gene_id: 1,
            new_value: Value::String("Bob".to_string()),
        };
        let (child, delta) = engine.apply_mutation(&parent, &m).unwrap();
        assert_eq!(child.id, 100);
        assert_eq!(child.parent_id, Some(1));
        assert_eq!(child.get_gene(1).unwrap().value, Value::String("Bob".to_string()));
        assert_eq!(parent.get_gene(1).unwrap().value, Value::String("Alice".to_string()));
        assert_eq!(delta.operations.len(), 1);
    }

    #[test]
    fn invalid_mutations_are_errors() {
        let engine = MutationEngine::new();
        let parent = base();
        let unknown_gene = Mutation::SubstituteGene { genome_id: 1, chromosome_id: 1, gene_id: 99, new_value: Value::Null };
        assert!(engine.apply_mutation(&parent, &unknown_gene).is_err());
        let wrong_genome = Mutation::DeleteGene { genome_id: 2, chromosome_id: 1, gene_id: 1 };
        assert!(engine.apply_mutation(&parent, &wrong_genome).is_err());
    }

    #[test]
    fn random_mutations_always_apply() {
        let engine = MutationEngine::with_start_id(1000);
        let mut genome = base();
        for _ in 0..200 {
            let m = engine.random_mutation(&genome);
            let (child, _) = engine.apply_mutation(&genome, &m).unwrap();
            assert!(child.validate().is_ok());
            genome = child;
        }
    }

    #[test]
    fn reserve_up_to_skips_used_ids() {
        let engine = MutationEngine::new();
        engine.reserve_up_to(50);
        assert_eq!(engine.next_id(), 51);
    }
}
