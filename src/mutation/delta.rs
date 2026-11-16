//! Deltas : description compacte et rejouable de la différence entre un genome et son parent.

use crate::types::{Chromosome, ChromosomeId, Gene, GeneId, Genome, GenomeId, Value};
use crate::util::{invalid_data, invalid_input, not_found, read_opt_pos, read_u32, read_u64, read_u8, write_count, write_opt_pos};
use std::collections::HashSet;
use std::io::{self, Read, Write};

/// Opération élémentaire d'un delta.
#[derive(Debug, Clone, PartialEq)]
pub enum DeltaOperation {
    /// Remplace la valeur d'un gène (l'ancienne valeur est conservée pour l'historique)
    SubstituteGene {
        /// Chromosome contenant le gène
        chromosome_id: ChromosomeId,
        /// Gène modifié
        gene_id: GeneId,
        /// Valeur avant mutation
        old_value: Value,
        /// Valeur après mutation
        new_value: Value,
    },
    /// Insère un gène dans un chromosome
    InsertGene {
        /// Chromosome cible
        chromosome_id: ChromosomeId,
        /// Gène inséré
        gene: Gene,
        /// Position (fin si `None`)
        position: Option<usize>,
    },
    /// Supprime un gène d'un chromosome
    DeleteGene {
        /// Chromosome contenant le gène
        chromosome_id: ChromosomeId,
        /// Gène supprimé
        gene_id: GeneId,
    },
    /// Insère un chromosome complet avec ses gènes
    InsertChromosome {
        /// Chromosome inséré (ses `genes` doivent tous figurer dans `genes`)
        chromosome: Chromosome,
        /// Gènes du chromosome
        genes: Vec<Gene>,
        /// Position (fin si `None`)
        position: Option<usize>,
    },
    /// Supprime un chromosome et tous ses gènes
    DeleteChromosome {
        /// Chromosome supprimé
        chromosome_id: ChromosomeId,
    },
}

impl DeltaOperation {
    /// Applique l'opération à un genome en validant sa cohérence.
    ///
    /// En cas d'erreur le genome peut avoir été partiellement modifié : appliquez
    /// toujours une opération sur une copie (c'est ce que fait `MutationEngine`).
    pub fn apply(&self, genome: &mut Genome) -> io::Result<()> {
        match self {
            DeltaOperation::SubstituteGene { chromosome_id, gene_id, new_value, .. } => {
                let idx = chromosome_index(genome, *chromosome_id)?;
                if !genome.chromosomes[idx].has_gene(*gene_id) {
                    return Err(not_found(format!(
                        "Gène {} absent du chromosome {}",
                        gene_id, chromosome_id
                    )));
                }
                if !genome.set_gene_value(*gene_id, new_value.clone()) {
                    return Err(not_found(format!("Gène {} introuvable", gene_id)));
                }
                Ok(())
            }
            DeltaOperation::InsertGene { chromosome_id, gene, position } => {
                let idx = chromosome_index(genome, *chromosome_id)?;
                if genome.genes.contains_key(&gene.id) {
                    return Err(invalid_input(format!("Gène {} déjà présent", gene.id)));
                }
                genome.genes.insert(gene.id, gene.clone());
                genome.chromosomes[idx].insert_gene_at(*position, gene.id);
                Ok(())
            }
            DeltaOperation::DeleteGene { chromosome_id, gene_id } => {
                let idx = chromosome_index(genome, *chromosome_id)?;
                if genome.chromosomes[idx].remove_gene(*gene_id).is_none() {
                    return Err(not_found(format!(
                        "Gène {} absent du chromosome {}",
                        gene_id, chromosome_id
                    )));
                }
                genome.genes.remove(gene_id);
                Ok(())
            }
            DeltaOperation::InsertChromosome { chromosome, genes, position } => {
                if genome.chromosomes.iter().any(|c| c.id == chromosome.id) {
                    return Err(invalid_input(format!("Chromosome {} déjà présent", chromosome.id)));
                }
                let mut seen = HashSet::new();
                for gene_id in &chromosome.genes {
                    if !seen.insert(*gene_id) {
                        return Err(invalid_input(format!("Gène {} listé deux fois", gene_id)));
                    }
                    if genome.genes.contains_key(gene_id) {
                        return Err(invalid_input(format!("Gène {} déjà présent", gene_id)));
                    }
                    if !genes.iter().any(|g| g.id == *gene_id) {
                        return Err(invalid_input(format!("Gène {} non fourni", gene_id)));
                    }
                }
                for gene in genes {
                    if seen.contains(&gene.id) {
                        genome.genes.insert(gene.id, gene.clone());
                    }
                }
                let pos = match position {
                    Some(p) => (*p).min(genome.chromosomes.len()),
                    None => genome.chromosomes.len(),
                };
                genome.chromosomes.insert(pos, chromosome.clone());
                Ok(())
            }
            DeltaOperation::DeleteChromosome { chromosome_id } => {
                let idx = chromosome_index(genome, *chromosome_id)?;
                let removed = genome.chromosomes.remove(idx);
                for gene_id in &removed.genes {
                    genome.genes.remove(gene_id);
                }
                Ok(())
            }
        }
    }

    /// Sérialise l'opération.
    pub fn serialize<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        match self {
            DeltaOperation::SubstituteGene { chromosome_id, gene_id, old_value, new_value } => {
                writer.write_all(&[0u8])?;
                writer.write_all(&chromosome_id.to_le_bytes())?;
                writer.write_all(&gene_id.to_le_bytes())?;
                old_value.serialize(writer)?;
                new_value.serialize(writer)?;
            }
            DeltaOperation::InsertGene { chromosome_id, gene, position } => {
                writer.write_all(&[1u8])?;
                writer.write_all(&chromosome_id.to_le_bytes())?;
                write_opt_pos(writer, *position)?;
                gene.serialize(writer)?;
            }
            DeltaOperation::DeleteGene { chromosome_id, gene_id } => {
                writer.write_all(&[2u8])?;
                writer.write_all(&chromosome_id.to_le_bytes())?;
                writer.write_all(&gene_id.to_le_bytes())?;
            }
            DeltaOperation::InsertChromosome { chromosome, genes, position } => {
                writer.write_all(&[3u8])?;
                write_opt_pos(writer, *position)?;
                chromosome.serialize(writer)?;
                write_count(writer, genes.len())?;
                for gene in genes {
                    gene.serialize(writer)?;
                }
            }
            DeltaOperation::DeleteChromosome { chromosome_id } => {
                writer.write_all(&[4u8])?;
                writer.write_all(&chromosome_id.to_le_bytes())?;
            }
        }
        Ok(())
    }

    /// Désérialise une opération.
    pub fn deserialize<R: Read>(reader: &mut R) -> io::Result<Self> {
        match read_u8(reader)? {
            0 => {
                let chromosome_id = read_u64(reader)?;
                let gene_id = read_u64(reader)?;
                let old_value = Value::deserialize(reader)?;
                let new_value = Value::deserialize(reader)?;
                Ok(DeltaOperation::SubstituteGene { chromosome_id, gene_id, old_value, new_value })
            }
            1 => {
                let chromosome_id = read_u64(reader)?;
                let position = read_opt_pos(reader)?;
                let gene = Gene::deserialize(reader)?;
                Ok(DeltaOperation::InsertGene { chromosome_id, gene, position })
            }
            2 => {
                let chromosome_id = read_u64(reader)?;
                let gene_id = read_u64(reader)?;
                Ok(DeltaOperation::DeleteGene { chromosome_id, gene_id })
            }
            3 => {
                let position = read_opt_pos(reader)?;
                let chromosome = Chromosome::deserialize(reader)?;
                let count = read_u32(reader)? as usize;
                let mut genes = Vec::with_capacity(count.min(4096));
                for _ in 0..count {
                    genes.push(Gene::deserialize(reader)?);
                }
                Ok(DeltaOperation::InsertChromosome { chromosome, genes, position })
            }
            4 => {
                let chromosome_id = read_u64(reader)?;
                Ok(DeltaOperation::DeleteChromosome { chromosome_id })
            }
            _ => Err(invalid_data("Type d'opération de delta inconnu")),
        }
    }
}

fn chromosome_index(genome: &Genome, chromosome_id: ChromosomeId) -> io::Result<usize> {
    genome
        .chromosomes
        .iter()
        .position(|c| c.id == chromosome_id)
        .ok_or_else(|| not_found(format!("Chromosome {} introuvable", chromosome_id)))
}

/// Delta : suite d'opérations menant d'un parent à son enfant.
#[derive(Debug, Clone, PartialEq)]
pub struct Delta {
    /// Genome produit
    pub genome_id: GenomeId,
    /// Genome parent
    pub parent_id: GenomeId,
    /// Génération du genome produit
    pub generation: u64,
    /// Opérations, dans l'ordre d'application
    pub operations: Vec<DeltaOperation>,
}

impl Delta {
    /// Crée un delta vide.
    pub fn new(genome_id: GenomeId, parent_id: GenomeId, generation: u64) -> Self {
        Self { genome_id, parent_id, generation, operations: Vec::new() }
    }

    /// Ajoute une opération.
    pub fn add_operation(&mut self, op: DeltaOperation) {
        self.operations.push(op);
    }

    /// Vrai si le delta ne contient aucune opération.
    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }

    /// Rejoue le delta sur le genome parent et retourne l'enfant.
    pub fn apply_to(&self, parent: &Genome) -> io::Result<Genome> {
        if parent.id != self.parent_id {
            return Err(invalid_input(format!(
                "Ce delta part du genome {}, pas du {}",
                self.parent_id, parent.id
            )));
        }
        let mut child = parent.child(self.genome_id);
        for op in &self.operations {
            op.apply(&mut child)?;
        }
        Ok(child)
    }

    /// Sérialise le delta (sans en-tête de fichier).
    pub fn serialize<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        writer.write_all(&self.genome_id.to_le_bytes())?;
        writer.write_all(&self.parent_id.to_le_bytes())?;
        writer.write_all(&self.generation.to_le_bytes())?;
        write_count(writer, self.operations.len())?;
        for op in &self.operations {
            op.serialize(writer)?;
        }
        Ok(())
    }

    /// Désérialise un delta.
    pub fn deserialize<R: Read>(reader: &mut R) -> io::Result<Self> {
        let genome_id = read_u64(reader)?;
        let parent_id = read_u64(reader)?;
        let generation = read_u64(reader)?;
        let count = read_u32(reader)? as usize;
        let mut operations = Vec::with_capacity(count.min(4096));
        for _ in 0..count {
            operations.push(DeltaOperation::deserialize(reader)?);
        }
        Ok(Self { genome_id, parent_id, generation, operations })
    }
}

/// Builder pour créer des deltas.
pub struct DeltaBuilder {
    delta: Delta,
}

impl DeltaBuilder {
    /// Démarre un delta.
    pub fn new(genome_id: GenomeId, parent_id: GenomeId, generation: u64) -> Self {
        Self { delta: Delta::new(genome_id, parent_id, generation) }
    }

    /// Ajoute une substitution de gène.
    pub fn substitute_gene(&mut self, chromosome_id: ChromosomeId, gene_id: GeneId, old_value: Value, new_value: Value) -> &mut Self {
        self.delta.add_operation(DeltaOperation::SubstituteGene { chromosome_id, gene_id, old_value, new_value });
        self
    }

    /// Ajoute une insertion de gène.
    pub fn insert_gene(&mut self, chromosome_id: ChromosomeId, gene: Gene, position: Option<usize>) -> &mut Self {
        self.delta.add_operation(DeltaOperation::InsertGene { chromosome_id, gene, position });
        self
    }

    /// Ajoute une suppression de gène.
    pub fn delete_gene(&mut self, chromosome_id: ChromosomeId, gene_id: GeneId) -> &mut Self {
        self.delta.add_operation(DeltaOperation::DeleteGene { chromosome_id, gene_id });
        self
    }

    /// Ajoute une insertion de chromosome.
    pub fn insert_chromosome(&mut self, chromosome: Chromosome, genes: Vec<Gene>, position: Option<usize>) -> &mut Self {
        self.delta.add_operation(DeltaOperation::InsertChromosome { chromosome, genes, position });
        self
    }

    /// Ajoute une suppression de chromosome.
    pub fn delete_chromosome(&mut self, chromosome_id: ChromosomeId) -> &mut Self {
        self.delta.add_operation(DeltaOperation::DeleteChromosome { chromosome_id });
        self
    }

    /// Termine la construction.
    pub fn build(self) -> Delta {
        self.delta
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Genome {
        let genes = vec![
            Gene::new(1, "a".to_string(), Value::Int(1), 0.5),
            Gene::new(2, "b".to_string(), Value::Int(2), 0.5),
        ];
        let chromosome = Chromosome::new(1, "c".to_string(), vec![1, 2], 0.0);
        Genome::try_new(1, vec![chromosome], genes).unwrap()
    }

    #[test]
    fn all_operations_roundtrip() {
        let mut b = DeltaBuilder::new(2, 1, 2);
        b.substitute_gene(1, 1, Value::Int(1), Value::Int(9))
            .insert_gene(1, Gene::new(10, "n".to_string(), Value::Null, 0.1), Some(0))
            .delete_gene(1, 2)
            .insert_chromosome(
                Chromosome::new(2, "d".to_string(), vec![20], 0.0),
                vec![Gene::new(20, "x".to_string(), Value::Bool(true), 0.3)],
                None,
            )
            .delete_chromosome(2);
        let delta = b.build();

        let mut buf = Vec::new();
        delta.serialize(&mut buf).unwrap();
        assert_eq!(Delta::deserialize(&mut buf.as_slice()).unwrap(), delta);
    }

    #[test]
    fn delta_replays_on_parent() {
        let parent = base();
        let mut b = DeltaBuilder::new(2, 1, 2);
        b.substitute_gene(1, 1, Value::Int(1), Value::Int(9))
            .insert_gene(1, Gene::new(10, "n".to_string(), Value::Null, 0.1), Some(0))
            .delete_gene(1, 2);
        let child = b.build().apply_to(&parent).unwrap();

        assert_eq!(child.get_gene(1).unwrap().value, Value::Int(9));
        assert_eq!(child.chromosomes[0].genes, vec![10, 1]);
        assert!(child.get_gene(2).is_none());
        // le parent reste intact
        assert_eq!(parent.get_gene(1).unwrap().value, Value::Int(1));
    }

    #[test]
    fn invalid_operations_are_rejected() {
        let mut g = base();
        let missing_chromosome = DeltaOperation::DeleteGene { chromosome_id: 99, gene_id: 1 };
        assert!(missing_chromosome.apply(&mut g).is_err());
        let duplicate = DeltaOperation::InsertGene {
            chromosome_id: 1,
            gene: Gene::new(1, "dup".to_string(), Value::Null, 0.5),
            position: None,
        };
        assert!(duplicate.apply(&mut g).is_err());
        let wrong_parent = Delta::new(5, 42, 2);
        assert!(wrong_parent.apply_to(&base()).is_err());
    }
}
