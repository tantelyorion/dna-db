//! Chromosome : groupe logique de gènes.

use super::gene::GeneId;
use crate::util::{clamp01, read_f32, read_string, read_u32, read_u64, write_count, write_len_bytes};
use std::io::{self, Read, Write};

/// Identifiant de chromosome.
pub type ChromosomeId = u64;

/// Chromosome : liste ordonnée d'identifiants de gènes qui mutent ensemble.
#[derive(Debug, Clone, PartialEq)]
pub struct Chromosome {
    /// Identifiant unique (dans un genome)
    pub id: ChromosomeId,
    /// Nom du chromosome
    pub name: String,
    /// Identifiants des gènes, dans l'ordre
    pub genes: Vec<GeneId>,
    /// Taux de mutation propre à ce chromosome (0.0 à 1.0)
    pub mutation_rate: f32,
}

impl Chromosome {
    /// Crée un nouveau chromosome.
    pub fn new(id: ChromosomeId, name: String, genes: Vec<GeneId>, mutation_rate: f32) -> Self {
        Self {
            id,
            name,
            genes,
            mutation_rate: clamp01(mutation_rate),
        }
    }

    /// Crée un chromosome vide (taux de mutation 0.01).
    pub fn empty(id: ChromosomeId, name: String) -> Self {
        Self::new(id, name, Vec::new(), 0.01)
    }

    /// Ajoute un gène à la fin ; retourne `false` s'il était déjà présent.
    pub fn add_gene(&mut self, gene_id: GeneId) -> bool {
        if self.has_gene(gene_id) {
            false
        } else {
            self.genes.push(gene_id);
            true
        }
    }

    /// Insère un gène à une position (ramenée à la longueur si trop grande) ou à la fin.
    pub fn insert_gene_at(&mut self, position: Option<usize>, gene_id: GeneId) {
        match position {
            Some(p) => {
                let p = p.min(self.genes.len());
                self.genes.insert(p, gene_id);
            }
            None => self.genes.push(gene_id),
        }
    }

    /// Retire un gène ; retourne sa position s'il était présent.
    pub fn remove_gene(&mut self, gene_id: GeneId) -> Option<usize> {
        let pos = self.genes.iter().position(|&id| id == gene_id)?;
        self.genes.remove(pos);
        Some(pos)
    }

    /// Vérifie si un gène est présent.
    pub fn has_gene(&self, gene_id: GeneId) -> bool {
        self.genes.contains(&gene_id)
    }

    /// Taille exacte en octets une fois sérialisé.
    pub fn size_bytes(&self) -> usize {
        8 + 4 + self.name.len() + 4 + self.genes.len() * 8 + 4
    }

    /// Sérialise le chromosome (les gènes sont référencés par identifiant).
    pub fn serialize<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        writer.write_all(&self.id.to_le_bytes())?;
        write_len_bytes(writer, self.name.as_bytes())?;
        write_count(writer, self.genes.len())?;
        for gene_id in &self.genes {
            writer.write_all(&gene_id.to_le_bytes())?;
        }
        writer.write_all(&self.mutation_rate.to_le_bytes())
    }

    /// Désérialise un chromosome.
    pub fn deserialize<R: Read>(reader: &mut R) -> io::Result<Self> {
        let id = read_u64(reader)?;
        let name = read_string(reader)?;
        let count = read_u32(reader)? as usize;
        let mut genes = Vec::with_capacity(count.min(4096));
        for _ in 0..count {
            genes.push(read_u64(reader)?);
        }
        let mutation_rate = read_f32(reader)?;
        Ok(Self::new(id, name, genes, mutation_rate))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_size() {
        let c = Chromosome::new(3, "profil".to_string(), vec![1, 2, 3], 0.05);
        let mut buf = Vec::new();
        c.serialize(&mut buf).unwrap();
        assert_eq!(buf.len(), c.size_bytes());
        assert_eq!(Chromosome::deserialize(&mut buf.as_slice()).unwrap(), c);
    }

    #[test]
    fn gene_list_operations() {
        let mut c = Chromosome::empty(1, "c".to_string());
        assert!(c.add_gene(10));
        assert!(!c.add_gene(10));
        c.insert_gene_at(Some(99), 11);
        c.insert_gene_at(Some(0), 12);
        assert_eq!(c.genes, vec![12, 10, 11]);
        assert_eq!(c.remove_gene(10), Some(1));
        assert_eq!(c.remove_gene(10), None);
    }
}
