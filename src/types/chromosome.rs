//! Chromosome - groupe logique de gènes

use super::gene::{Gene, GeneId};
use std::io::{Read, Write};

/// Identifiant de chromosome
pub type ChromosomeId = u64;

/// Chromosome - groupe de gènes qui mute ensemble
#[derive(Debug, Clone)]
pub struct Chromosome {
    /// Identifiant unique
    pub id: ChromosomeId,
    /// Nom du chromosome
    pub name: String,
    /// Liste des IDs de gènes
    pub genes: Vec<GeneId>,
    /// Taux de mutation spécifique à ce chromosome
    pub mutation_rate: f32,
}

impl Chromosome {
    /// Crée un nouveau chromosome
    pub fn new(id: ChromosomeId, name: String, genes: Vec<GeneId>, mutation_rate: f32) -> Self {
        Self {
            id,
            name,
            genes,
            mutation_rate: mutation_rate.clamp(0.0, 1.0),
        }
    }

    /// Crée un chromosome vide
    pub fn empty(id: ChromosomeId, name: String) -> Self {
        Self {
            id,
            name,
            genes: Vec::new(),
            mutation_rate: 0.01,
        }
    }

    /// Ajoute un gène au chromosome
    pub fn add_gene(&mut self, gene_id: GeneId) {
        self.genes.push(gene_id);
    }

    /// Retire un gène du chromosome
    pub fn remove_gene(&mut self, gene_id: GeneId) -> Option<usize> {
        self.genes.iter().position(|&id| id == gene_id).map(|pos| {
            self.genes.remove(pos);
            pos
        })
    }

    /// Vérifie si un gène est présent
    pub fn has_gene(&self, gene_id: GeneId) -> bool {
        self.genes.contains(&gene_id)
    }

    /// Retourne la taille approximative en bytes
    pub fn size_bytes(&self, gene_count: usize) -> usize {
        8 + // id
        self.name.len() + 4 + // name
        4 + (self.genes.len() * 8) + // gene list
        4 // mutation_rate
    }

    /// Sérialise le chromosome
    pub fn serialize<W: Write>(&self, writer: &mut W, genes: &[Gene]) -> std::io::Result<()> {
        writer.write_all(&self.id.to_le_bytes())?;
        
        let name_len = self.name.len() as u32;
        writer.write_all(&name_len.to_le_bytes())?;
        writer.write_all(self.name.as_bytes())?;
        
        let gene_count = self.genes.len() as u32;
        writer.write_all(&gene_count.to_le_bytes())?;
        
        for gene_id in &self.genes {
            writer.write_all(&gene_id.to_le_bytes())?;
        }
        
        writer.write_all(&self.mutation_rate.to_le_bytes())?;
        
        Ok(())
    }

    /// Désérialise un chromosome
    pub fn deserialize<R: Read>(reader: &mut R) -> std::io::Result<Self> {
        let mut id_bytes = [0u8; 8];
        reader.read_exact(&mut id_bytes)?;
        let id = u64::from_le_bytes(id_bytes);
        
        let mut name_len_bytes = [0u8; 4];
        reader.read_exact(&mut name_len_bytes)?;
        let name_len = u32::from_le_bytes(name_len_bytes) as usize;
        
        let mut name_bytes = vec![0u8; name_len];
        reader.read_exact(&mut name_bytes)?;
        let name = String::from_utf8(name_bytes).map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid name UTF-8"))?;
        
        let mut gene_count_bytes = [0u8; 4];
        reader.read_exact(&mut gene_count_bytes)?;
        let gene_count = u32::from_le_bytes(gene_count_bytes) as usize;
        
        let mut genes = Vec::with_capacity(gene_count);
        for _ in 0..gene_count {
            let mut gene_id_bytes = [0u8; 8];
            reader.read_exact(&mut gene_id_bytes)?;
            genes.push(u64::from_le_bytes(gene_id_bytes));
        }
        
        let mut rate_bytes = [0u8; 4];
        reader.read_exact(&mut rate_bytes)?;
        let mutation_rate = f32::from_le_bytes(rate_bytes);
        
        Ok(Self::new(id, name, genes, mutation_rate))
    }
}