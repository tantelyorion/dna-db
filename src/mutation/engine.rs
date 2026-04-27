//! Moteur de mutations

use crate::types::*;
use crate::mutation::delta::DeltaBuilder;
use crate::storage::Delta;
use rand::Rng;
use std::sync::atomic::{AtomicU64, Ordering};

/// Mutation possible
#[derive(Debug, Clone)]
pub enum Mutation {
    /// Substitution d'un gène
    SubstituteGene {
        genome_id: GenomeId,
        chromosome_id: ChromosomeId,
        gene_id: GeneId,
        new_value: Value,
    },
    /// Insertion d'un gène
    InsertGene {
        genome_id: GenomeId,
        chromosome_id: ChromosomeId,
        gene: Gene,
        position: Option<usize>,
    },
    /// Suppression d'un gène
    DeleteGene {
        genome_id: GenomeId,
        chromosome_id: ChromosomeId,
        gene_id: GeneId,
    },
    /// Pas de mutation (no-op)
    Noop,
}

impl Mutation {
    pub fn genome_id(&self) -> Option<GenomeId> {
        match self {
            Mutation::SubstituteGene { genome_id, .. } => Some(*genome_id),
            Mutation::InsertGene { genome_id, .. } => Some(*genome_id),
            Mutation::DeleteGene { genome_id, .. } => Some(*genome_id),
            Mutation::Noop => None,
        }
    }
}

/// Moteur de mutations
pub struct MutationEngine {
    next_id: AtomicU64,
    rng: rand::rngs::ThreadRng,
}

impl MutationEngine {
    pub fn new() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            rng: rand::thread_rng(),
        }
    }
    
    pub fn next_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }
    
    /// Applique une mutation à un genome, retourne le nouveau genome et le delta
    pub fn apply_mutation(&mut self, genome: &Genome, mutation: &Mutation) -> (Genome, Delta) {
        let new_id = self.next_id();
        let mut child = genome.child(new_id);
        let mut delta = DeltaBuilder::new(child.id, genome.id, child.generation);
        
        match mutation {
            Mutation::SubstituteGene { chromosome_id, gene_id, new_value, .. } => {
                let old_value = child.get_gene(*gene_id).map(|g| g.value.clone());
                if let Some(old_val) = old_value {
                    if child.set_gene_value(*gene_id, new_value.clone()) {
                        delta.substitute_gene(*chromosome_id, *gene_id, old_val, new_value.clone());
                    }
                }
            }
            Mutation::InsertGene { chromosome_id, gene, position, .. } => {
                child.genes.insert(gene.id, gene.clone());
                if let Some(chromo) = child.chromosomes.iter_mut().find(|c| c.id == *chromosome_id) {
                    if let Some(pos) = position {
                        chromo.genes.insert(*pos, gene.id);
                    } else {
                        chromo.add_gene(gene.id);
                    }
                }
                delta.insert_gene(*chromosome_id, gene.clone(), *position);
            }
            Mutation::DeleteGene { chromosome_id, gene_id, .. } => {
                child.genes.remove(gene_id);
                if let Some(chromo) = child.chromosomes.iter_mut().find(|c| c.id == *chromosome_id) {
                    chromo.remove_gene(*gene_id);
                }
                delta.delete_gene(*chromosome_id, *gene_id);
            }
            Mutation::Noop => {}
        }
        
        (child, delta.build())
    }
    
    /// Génère une mutation aléatoire sur un genome (pour évolution automatique)
    pub fn random_mutation(&mut self, genome: &Genome) -> Mutation {
        if genome.genes.is_empty() {
            return Mutation::Noop;
        }
        
        let mutation_type = self.rng.gen_range(0..3);
        
        // Sélection aléatoire d'un chromosome et d'un gène
        if let Some(chromosome) = genome.chromosomes.first() {
            if let Some(&gene_id) = chromosome.genes.first() {
                if let Some(gene) = genome.get_gene(gene_id) {
                    match mutation_type {
                        0 => {
                            // Substitution
                            let new_value = match &gene.value {
                                Value::Int(v) => Value::Int(v.saturating_add(self.rng.gen_range(-10..10))),
                                Value::Float(v) => Value::Float(v + self.rng.gen_range(-1.0..1.0)),
                                Value::String(s) => {
                                    let mut chars: Vec<char> = s.chars().collect();
                                    if !chars.is_empty() {
                                        let idx = self.rng.gen_range(0..chars.len());
                                        chars[idx] = self.rng.gen_range('a'..='z');
                                        Value::String(chars.into_iter().collect())
                                    } else {
                                        Value::String("mutated".to_string())
                                    }
                                }
                                Value::Bool(b) => Value::Bool(!b),
                                _ => gene.value.clone(),
                            };
                            Mutation::SubstituteGene {
                                genome_id: genome.id,
                                chromosome_id: chromosome.id,
                                gene_id,
                                new_value,
                            }
                        }
                        1 => {
                            // Insertion (simplifiée)
                            let new_gene = Gene::new(
                                self.next_id(),
                                format!("new_gene_{}", self.next_id()),
                                Value::Int(0),
                                0.5,
                            );
                            Mutation::InsertGene {
                                genome_id: genome.id,
                                chromosome_id: chromosome.id,
                                gene: new_gene,
                                position: None,
                            }
                        }
                        _ => {
                            // Suppression
                            Mutation::DeleteGene {
                                genome_id: genome.id,
                                chromosome_id: chromosome.id,
                                gene_id,
                            }
                        }
                    }
                } else {
                    Mutation::Noop
                }
            } else {
                Mutation::Noop
            }
        } else {
            Mutation::Noop
        }
    }
}

impl Default for MutationEngine {
    fn default() -> Self {
        Self::new()
    }
}