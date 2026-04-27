//! Construction et gestion des deltas

use crate::types::*;
use crate::storage::{Delta, DeltaOperation};
use crate::storage::file_format::DeltaOperation as FileDeltaOperation;

/// Builder pour créer des deltas
pub struct DeltaBuilder {
    genome_id: GenomeId,
    parent_id: GenomeId,
    generation: u64,
    operations: Vec<DeltaOperation>,
}

impl DeltaBuilder {
    pub fn new(genome_id: GenomeId, parent_id: GenomeId, generation: u64) -> Self {
        Self {
            genome_id,
            parent_id,
            generation,
            operations: Vec::new(),
        }
    }
    
    pub fn substitute_gene(&mut self, chromosome_id: ChromosomeId, gene_id: GeneId, old_value: Value, new_value: Value) {
        self.operations.push(DeltaOperation::SubstituteGene {
            chromosome_id,
            gene_id,
            old_value,
            new_value,
        });
    }
    
    pub fn insert_gene(&mut self, chromosome_id: ChromosomeId, gene: Gene, position: Option<usize>) {
        self.operations.push(DeltaOperation::InsertGene {
            chromosome_id,
            gene,
            position,
        });
    }
    
    pub fn delete_gene(&mut self, chromosome_id: ChromosomeId, gene_id: GeneId) {
        self.operations.push(DeltaOperation::DeleteGene {
            chromosome_id,
            gene_id,
        });
    }
    
    pub fn build(self) -> Delta {
        let mut delta = Delta::new(self.genome_id, self.parent_id, self.generation);
        for op in self.operations {
            delta.add_operation(op);
        }
        delta
    }
}