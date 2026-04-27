//! Formats binaires pour le stockage sur disque

use crate::types::*;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write, BufReader, BufWriter};
use std::path::{Path, PathBuf};

/// Type de fichier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FileType {
    Checkpoint = 0,
    Delta = 1,
    Index = 2,
    Wal = 3,
}

/// En-tête de fichier
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FileHeader {
    magic: [u8; 4],  // "DNAD"
    version: u8,
    file_type: u8,
    reserved: [u8; 2],
}

impl FileHeader {
    pub const MAGIC: [u8; 4] = *b"DNAD";
    pub const VERSION: u8 = 1;
    
    pub fn new(file_type: FileType) -> Self {
        Self {
            magic: Self::MAGIC,
            version: Self::VERSION,
            file_type: file_type as u8,
            reserved: [0, 0],
        }
    }
    
    pub fn validate(&self) -> bool {
        self.magic == Self::MAGIC && self.version == Self::VERSION
    }
    
    pub fn write<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writer.write_all(&self.magic)?;
        writer.write_all(&[self.version])?;
        writer.write_all(&[self.file_type])?;
        writer.write_all(&self.reserved)?;
        Ok(())
    }
    
    pub fn read<R: Read>(reader: &mut R) -> std::io::Result<Self> {
        let mut magic = [0u8; 4];
        reader.read_exact(&mut magic)?;
        
        let mut version = [0u8; 1];
        reader.read_exact(&mut version)?;
        
        let mut file_type = [0u8; 1];
        reader.read_exact(&mut file_type)?;
        
        let mut reserved = [0u8; 2];
        reader.read_exact(&mut reserved)?;
        
        Ok(Self {
            magic,
            version: version[0],
            file_type: file_type[0],
            reserved,
        })
    }
}

/// Delta de mutation
#[derive(Debug, Clone)]
pub struct Delta {
    pub genome_id: GenomeId,
    pub parent_id: GenomeId,
    pub generation: u64,
    pub operations: Vec<DeltaOperation>,
}

#[derive(Debug, Clone)]
pub enum DeltaOperation {
    SubstituteGene {
        chromosome_id: ChromosomeId,
        gene_id: GeneId,
        old_value: Value,
        new_value: Value,
    },
    InsertGene {
        chromosome_id: ChromosomeId,
        gene: Gene,
        position: Option<usize>,
    },
    DeleteGene {
        chromosome_id: ChromosomeId,
        gene_id: GeneId,
    },
    InsertChromosome {
        chromosome: Chromosome,
        position: Option<usize>,
    },
    DeleteChromosome {
        chromosome_id: ChromosomeId,
    },
}

impl Delta {
    pub fn new(genome_id: GenomeId, parent_id: GenomeId, generation: u64) -> Self {
        Self {
            genome_id,
            parent_id,
            generation,
            operations: Vec::new(),
        }
    }
    
    pub fn add_operation(&mut self, op: DeltaOperation) {
        self.operations.push(op);
    }
    
    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }
    
    pub fn serialize<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        let header = FileHeader::new(FileType::Delta);
        header.write(writer)?;
        
        writer.write_all(&self.genome_id.to_le_bytes())?;
        writer.write_all(&self.parent_id.to_le_bytes())?;
        writer.write_all(&self.generation.to_le_bytes())?;
        
        let op_count = self.operations.len() as u32;
        writer.write_all(&op_count.to_le_bytes())?;
        
        for op in &self.operations {
            op.serialize(writer)?;
        }
        
        Ok(())
    }
    
    pub fn deserialize<R: Read>(reader: &mut R) -> std::io::Result<Self> {
        let header = FileHeader::read(reader)?;
        if !header.validate() {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid delta header"));
        }
        
        let mut id_bytes = [0u8; 8];
        reader.read_exact(&mut id_bytes)?;
        let genome_id = u64::from_le_bytes(id_bytes);
        
        let mut parent_bytes = [0u8; 8];
        reader.read_exact(&mut parent_bytes)?;
        let parent_id = u64::from_le_bytes(parent_bytes);
        
        let mut gen_bytes = [0u8; 8];
        reader.read_exact(&mut gen_bytes)?;
        let generation = u64::from_le_bytes(gen_bytes);
        
        let mut op_count_bytes = [0u8; 4];
        reader.read_exact(&mut op_count_bytes)?;
        let op_count = u32::from_le_bytes(op_count_bytes) as usize;
        
        let mut operations = Vec::with_capacity(op_count);
        for _ in 0..op_count {
            operations.push(DeltaOperation::deserialize(reader)?);
        }
        
        Ok(Self {
            genome_id,
            parent_id,
            generation,
            operations,
        })
    }
}

impl DeltaOperation {
    pub fn serialize<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
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
                let pos = position.unwrap_or(usize::MAX) as u64;
                writer.write_all(&pos.to_le_bytes())?;
                gene.serialize(writer)?;
            }
            DeltaOperation::DeleteGene { chromosome_id, gene_id } => {
                writer.write_all(&[2u8])?;
                writer.write_all(&chromosome_id.to_le_bytes())?;
                writer.write_all(&gene_id.to_le_bytes())?;
            }
            DeltaOperation::InsertChromosome { chromosome, position } => {
                writer.write_all(&[3u8])?;
                let pos = position.unwrap_or(usize::MAX) as u64;
                writer.write_all(&pos.to_le_bytes())?;
                // Temporaire: simplifié
                writer.write_all(&chromosome.id.to_le_bytes())?;
                writer.write_all(&[0u8; 8])?; // placeholder
            }
            DeltaOperation::DeleteChromosome { chromosome_id } => {
                writer.write_all(&[4u8])?;
                writer.write_all(&chromosome_id.to_le_bytes())?;
            }
        }
        Ok(())
    }
    
    pub fn deserialize<R: Read>(reader: &mut R) -> std::io::Result<Self> {
        let mut op_type = [0u8; 1];
        reader.read_exact(&mut op_type)?;
        
        match op_type[0] {
            0 => {
                let mut chromo_bytes = [0u8; 8];
                reader.read_exact(&mut chromo_bytes)?;
                let chromosome_id = u64::from_le_bytes(chromo_bytes);
                
                let mut gene_bytes = [0u8; 8];
                reader.read_exact(&mut gene_bytes)?;
                let gene_id = u64::from_le_bytes(gene_bytes);
                
                let old_value = Value::deserialize(reader)?;
                let new_value = Value::deserialize(reader)?;
                
                Ok(DeltaOperation::SubstituteGene {
                    chromosome_id,
                    gene_id,
                    old_value,
                    new_value,
                })
            }
            1 => {
                let mut chromo_bytes = [0u8; 8];
                reader.read_exact(&mut chromo_bytes)?;
                let chromosome_id = u64::from_le_bytes(chromo_bytes);
                
                let mut pos_bytes = [0u8; 8];
                reader.read_exact(&mut pos_bytes)?;
                let pos_val = u64::from_le_bytes(pos_bytes);
                let position = if pos_val == usize::MAX as u64 { None } else { Some(pos_val as usize) };
                
                let gene = Gene::deserialize(reader)?;
                
                Ok(DeltaOperation::InsertGene {
                    chromosome_id,
                    gene,
                    position,
                })
            }
            2 => {
                let mut chromo_bytes = [0u8; 8];
                reader.read_exact(&mut chromo_bytes)?;
                let chromosome_id = u64::from_le_bytes(chromo_bytes);
                
                let mut gene_bytes = [0u8; 8];
                reader.read_exact(&mut gene_bytes)?;
                let gene_id = u64::from_le_bytes(gene_bytes);
                
                Ok(DeltaOperation::DeleteGene {
                    chromosome_id,
                    gene_id,
                })
            }
            _ => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Unknown operation type")),
        }
    }
}