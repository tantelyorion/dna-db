//! Genome : une donnée vivante complète (chromosomes + gènes + métadonnées d'évolution).

use super::chromosome::{Chromosome, ChromosomeId};
use super::gene::{Gene, GeneId};
use super::value::Value;
use crate::util::{clamp01, invalid_data, invalid_input, now_secs, read_f32, read_u32, read_u64, write_count};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::io::{self, Read, Write};

/// Identifiant de genome (0 est réservé : « pas de parent »).
pub type GenomeId = u64;

/// Statut mémoire, dérivé du fitness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryStatus {
    /// Données utilisées fréquemment, accès rapide
    Active = 0,
    /// Données historiques, rarement utilisées
    Passive = 1,
    /// Données d'archive, candidates au nettoyage
    Fossil = 2,
}

impl MemoryStatus {
    /// Déduit le statut depuis un fitness (> 0.7 actif, > 0.3 passif, sinon fossile).
    pub fn from_fitness(fitness: f32) -> Self {
        if fitness > 0.7 {
            MemoryStatus::Active
        } else if fitness > 0.3 {
            MemoryStatus::Passive
        } else {
            MemoryStatus::Fossil
        }
    }
}

/// Score de fitness, toujours dans [0, 1] (jamais NaN, jamais -0.0).
///
/// Le champ est privé pour garantir ces invariants, ce qui permet un ordre total
/// (`Ord`) utilisable comme clé d'index.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FitnessScore(f32);

impl FitnessScore {
    /// Crée un score en ramenant la valeur dans [0, 1].
    pub fn new(value: f32) -> Self {
        let v = clamp01(value);
        Self(if v == 0.0 { 0.0 } else { v })
    }

    /// Valeur du score.
    pub fn value(&self) -> f32 {
        self.0
    }
}

impl Eq for FitnessScore {}

impl PartialOrd for FitnessScore {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for FitnessScore {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.total_cmp(&other.0)
    }
}

/// Genome : entité versionnée. Chaque mutation produit un nouvel enfant.
#[derive(Debug, Clone)]
pub struct Genome {
    /// Identifiant unique
    pub id: GenomeId,
    /// Parent (génération précédente)
    pub parent_id: Option<GenomeId>,
    /// Numéro de génération (1 pour un genome initial)
    pub generation: u64,
    /// Chromosomes, dans l'ordre
    pub chromosomes: Vec<Chromosome>,
    /// Tous les gènes, indexés par identifiant
    pub genes: HashMap<GeneId, Gene>,
    /// Score de fitness
    pub fitness: FitnessScore,
    /// Statut mémoire (toujours cohérent avec le fitness)
    pub memory_status: MemoryStatus,
    /// Horodatage de création (secondes Unix)
    pub created_at: u64,
    /// Dernier accès (secondes Unix)
    pub last_access: u64,
    /// Nombre d'accès
    pub access_count: u64,
}

impl Genome {
    /// Crée un genome de première génération sans validation (voir [`Genome::try_new`]).
    pub fn new(id: GenomeId, chromosomes: Vec<Chromosome>, genes: Vec<Gene>) -> Self {
        let now = now_secs();
        let fitness = FitnessScore::new(0.5);
        Self {
            id,
            parent_id: None,
            generation: 1,
            chromosomes,
            genes: genes.into_iter().map(|g| (g.id, g)).collect(),
            fitness,
            memory_status: MemoryStatus::from_fitness(fitness.value()),
            created_at: now,
            last_access: now,
            access_count: 0,
        }
    }

    /// Crée un genome en validant sa cohérence (id non nul, ids de gènes et de
    /// chromosomes uniques, gènes référencés existants).
    pub fn try_new(id: GenomeId, chromosomes: Vec<Chromosome>, genes: Vec<Gene>) -> io::Result<Self> {
        if id == 0 {
            return Err(invalid_input("L'identifiant de genome 0 est réservé"));
        }
        let mut map = HashMap::with_capacity(genes.len());
        for gene in genes {
            let gene_id = gene.id;
            if map.insert(gene_id, gene).is_some() {
                return Err(invalid_input(format!("Gène {} dupliqué", gene_id)));
            }
        }
        let mut genome = Self::new(id, chromosomes, Vec::new());
        genome.genes = map;
        genome.validate()?;
        Ok(genome)
    }

    /// Vérifie la cohérence interne : chromosomes uniques, gènes référencés existants
    /// et appartenant à un seul chromosome.
    pub fn validate(&self) -> io::Result<()> {
        let mut chromosome_ids = HashSet::new();
        let mut used_genes = HashSet::new();
        for chromosome in &self.chromosomes {
            if !chromosome_ids.insert(chromosome.id) {
                return Err(invalid_input(format!("Chromosome {} dupliqué", chromosome.id)));
            }
            for gene_id in &chromosome.genes {
                if !self.genes.contains_key(gene_id) {
                    return Err(invalid_input(format!(
                        "Gène {} introuvable (référencé par le chromosome {})",
                        gene_id, chromosome.id
                    )));
                }
                if !used_genes.insert(*gene_id) {
                    return Err(invalid_input(format!(
                        "Gène {} référencé plusieurs fois",
                        gene_id
                    )));
                }
            }
        }
        Ok(())
    }

    /// Crée un enfant (copie) pour une mutation.
    pub fn child(&self, new_id: GenomeId) -> Self {
        let now = now_secs();
        let fitness = FitnessScore::new(0.5);
        Self {
            id: new_id,
            parent_id: Some(self.id),
            generation: self.generation.saturating_add(1),
            chromosomes: self.chromosomes.clone(),
            genes: self.genes.clone(),
            fitness,
            memory_status: MemoryStatus::from_fitness(fitness.value()),
            created_at: now,
            last_access: now,
            access_count: 0,
        }
    }

    /// Enregistre un accès.
    pub fn record_access(&mut self) {
        self.access_count = self.access_count.saturating_add(1);
        self.last_access = now_secs();
    }

    /// Met à jour le statut mémoire selon le fitness.
    pub fn update_memory_status(&mut self) {
        self.memory_status = MemoryStatus::from_fitness(self.fitness.value());
    }

    /// Récupère un gène par son identifiant.
    pub fn get_gene(&self, gene_id: GeneId) -> Option<&Gene> {
        self.genes.get(&gene_id)
    }

    /// Récupère un gène par son nom : le premier dans l'ordre des chromosomes,
    /// puis, à défaut, le gène orphelin de plus petit identifiant.
    pub fn get_gene_by_name(&self, name: &str) -> Option<&Gene> {
        for chromosome in &self.chromosomes {
            for gene_id in &chromosome.genes {
                if let Some(gene) = self.genes.get(gene_id) {
                    if gene.name == name {
                        return Some(gene);
                    }
                }
            }
        }
        self.genes
            .values()
            .filter(|g| g.name == name)
            .min_by_key(|g| g.id)
    }

    /// Récupère un chromosome par son identifiant.
    pub fn get_chromosome(&self, chromosome_id: ChromosomeId) -> Option<&Chromosome> {
        self.chromosomes.iter().find(|c| c.id == chromosome_id)
    }

    /// Modifie la valeur d'un gène ; retourne `false` s'il n'existe pas.
    pub fn set_gene_value(&mut self, gene_id: GeneId, new_value: Value) -> bool {
        match self.genes.get_mut(&gene_id) {
            Some(gene) => {
                gene.value = new_value;
                true
            }
            None => false,
        }
    }

    /// Taille exacte en octets une fois sérialisé.
    pub fn size_bytes(&self) -> usize {
        // id, parent, génération, fitness, created_at, last_access, access_count
        let mut size = 8 + 8 + 8 + 4 + 8 + 8 + 8;
        size += 4;
        size += self.chromosomes.iter().map(|c| c.size_bytes()).sum::<usize>();
        size += 4;
        size += self.genes.values().map(|g| g.size_bytes()).sum::<usize>();
        size
    }

    /// Sérialise le genome (sans en-tête de fichier : voir `storage::file_format`).
    ///
    /// Tous les gènes sont écrits, triés par identifiant, que le chromosome les
    /// référence ou non : le format est déterministe.
    pub fn serialize<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        writer.write_all(&self.id.to_le_bytes())?;
        writer.write_all(&self.parent_id.unwrap_or(0).to_le_bytes())?;
        writer.write_all(&self.generation.to_le_bytes())?;
        writer.write_all(&self.fitness.value().to_le_bytes())?;
        writer.write_all(&self.created_at.to_le_bytes())?;
        writer.write_all(&self.last_access.to_le_bytes())?;
        writer.write_all(&self.access_count.to_le_bytes())?;

        write_count(writer, self.chromosomes.len())?;
        for chromosome in &self.chromosomes {
            chromosome.serialize(writer)?;
        }

        let mut ids: Vec<GeneId> = self.genes.keys().copied().collect();
        ids.sort_unstable();
        write_count(writer, ids.len())?;
        for id in ids {
            if let Some(gene) = self.genes.get(&id) {
                gene.serialize(writer)?;
            }
        }
        Ok(())
    }

    /// Désérialise un genome et vérifie sa cohérence.
    pub fn deserialize<R: Read>(reader: &mut R) -> io::Result<Self> {
        let id = read_u64(reader)?;
        let parent_raw = read_u64(reader)?;
        let parent_id = if parent_raw == 0 { None } else { Some(parent_raw) };
        let generation = read_u64(reader)?;
        let fitness = FitnessScore::new(read_f32(reader)?);
        let created_at = read_u64(reader)?;
        let last_access = read_u64(reader)?;
        let access_count = read_u64(reader)?;

        let chromosome_count = read_u32(reader)? as usize;
        let mut chromosomes = Vec::with_capacity(chromosome_count.min(1024));
        for _ in 0..chromosome_count {
            chromosomes.push(Chromosome::deserialize(reader)?);
        }

        let gene_count = read_u32(reader)? as usize;
        let mut genes = HashMap::with_capacity(gene_count.min(4096));
        for _ in 0..gene_count {
            let gene = Gene::deserialize(reader)?;
            if genes.insert(gene.id, gene).is_some() {
                return Err(invalid_data("Gène dupliqué dans le fichier"));
            }
        }

        let genome = Self {
            id,
            parent_id,
            generation,
            chromosomes,
            genes,
            fitness,
            memory_status: MemoryStatus::from_fitness(fitness.value()),
            created_at,
            last_access,
            access_count,
        };
        genome
            .validate()
            .map_err(|e| invalid_data(format!("Genome incohérent : {}", e)))?;
        Ok(genome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Genome {
        let genes = vec![
            Gene::new(1, "name".to_string(), Value::String("Alice".to_string()), 0.8),
            Gene::new(2, "age".to_string(), Value::Int(30), 0.6),
            Gene::new(3, "orphan".to_string(), Value::Bool(true), 0.4),
        ];
        let chromosome = Chromosome::new(1, "profile".to_string(), vec![1, 2], 0.01);
        Genome::try_new(5, vec![chromosome], genes).unwrap()
    }

    #[test]
    fn roundtrip_keeps_every_gene() {
        // Régression : les gènes rattachés à un chromosome étaient perdus à la relecture.
        let genome = sample();
        let mut buf = Vec::new();
        genome.serialize(&mut buf).unwrap();
        assert_eq!(buf.len(), genome.size_bytes());

        let back = Genome::deserialize(&mut buf.as_slice()).unwrap();
        assert_eq!(back.id, 5);
        assert_eq!(back.genes.len(), 3);
        assert_eq!(back.chromosomes, genome.chromosomes);
        assert_eq!(back.get_gene(1), genome.get_gene(1));
        assert_eq!(back.get_gene_by_name("orphan").map(|g| g.id), Some(3));
    }

    #[test]
    fn validation_rejects_dangling_and_duplicate_references() {
        let g = Gene::new(1, "a".to_string(), Value::Null, 0.5);
        let dangling = Chromosome::new(1, "c".to_string(), vec![1, 2], 0.0);
        assert!(Genome::try_new(1, vec![dangling], vec![g.clone()]).is_err());

        let c1 = Chromosome::new(1, "c1".to_string(), vec![1], 0.0);
        let c2 = Chromosome::new(2, "c2".to_string(), vec![1], 0.0);
        assert!(Genome::try_new(1, vec![c1, c2], vec![g]).is_err());
        assert!(Genome::try_new(0, vec![], vec![]).is_err());
    }

    #[test]
    fn child_increments_generation() {
        let parent = sample();
        let child = parent.child(9);
        assert_eq!(child.parent_id, Some(5));
        assert_eq!(child.generation, 2);
    }

    #[test]
    fn fitness_score_is_totally_ordered() {
        assert!(FitnessScore::new(0.2) < FitnessScore::new(0.9));
        assert_eq!(FitnessScore::new(-3.0), FitnessScore::new(0.0));
        assert_eq!(FitnessScore::new(f32::NAN).value(), 0.0);
    }
}
