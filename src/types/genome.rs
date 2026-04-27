//! Genome - entité complète (l'équivalent d'une "donnée vivante")

use super::chromosome::{Chromosome, ChromosomeId};
use super::gene::{Gene, GeneId};
use super::value::Value;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::time::{SystemTime, UNIX_EPOCH};

/// Identifiant de genome
pub type GenomeId = u64;

/// Statut de mémoire
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryStatus {
    /// Données utilisées fréquemment, accès rapide
    Active = 0,
    /// Données historiques, rarement utilisées
    Passive = 1,
    /// Données archives, lecture seule, compression extrême
    Fossil = 2,
}

impl MemoryStatus {
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

/// Score de fitness
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct FitnessScore(pub f32);

impl FitnessScore {
    pub fn new(value: f32) -> Self {
        Self(value.clamp(0.0, 1.0))
    }
    
    pub fn value(&self) -> f32 {
        self.0
    }
}

/// Genome - une donnée vivante complète
#[derive(Debug, Clone)]
pub struct Genome {
    /// Identifiant unique
    pub id: GenomeId,
    /// ID du parent (génération précédente)
    pub parent_id: Option<GenomeId>,
    /// Numéro de génération
    pub generation: u64,
    /// Chromosomes de ce genome
    pub chromosomes: Vec<Chromosome>,
    /// Tous les gènes (mappés par ID)
    pub genes: HashMap<GeneId, Gene>,
    /// Score de fitness
    pub fitness: FitnessScore,
    /// Statut mémoire
    pub memory_status: MemoryStatus,
    /// Horodatage de création
    pub created_at: u64,
    /// Dernier accès
    pub last_access: u64,
    /// Nombre d'accès
    pub access_count: u64,
}

impl Genome {
    /// Crée un nouveau genome (première génération)
    pub fn new(id: GenomeId, chromosomes: Vec<Chromosome>, genes: Vec<Gene>) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        let genes_map = genes.into_iter().map(|g| (g.id, g)).collect();
        
        Self {
            id,
            parent_id: None,
            generation: 1,
            chromosomes,
            genes: genes_map,
            fitness: FitnessScore::new(0.5),
            memory_status: MemoryStatus::Active,
            created_at: now,
            last_access: now,
            access_count: 0,
        }
    }

    /// Crée un enfant par mutation
    pub fn child(&self, new_id: GenomeId) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        Self {
            id: new_id,
            parent_id: Some(self.id),
            generation: self.generation + 1,
            chromosomes: self.chromosomes.clone(),
            genes: self.genes.clone(),
            fitness: FitnessScore::new(0.5),
            memory_status: MemoryStatus::Active,
            created_at: now,
            last_access: now,
            access_count: 0,
        }
    }

    /// Enregistre un accès au genome
    pub fn record_access(&mut self) {
        self.access_count += 1;
        self.last_access = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
    }

    /// Met à jour le statut mémoire selon le fitness
    pub fn update_memory_status(&mut self) {
        self.memory_status = MemoryStatus::from_fitness(self.fitness.0);
    }

    /// Récupère un gène par son ID
    pub fn get_gene(&self, gene_id: GeneId) -> Option<&Gene> {
        self.genes.get(&gene_id)
    }

    /// Récupère un gène par son nom (premier trouvé)
    pub fn get_gene_by_name(&self, name: &str) -> Option<&Gene> {
        self.genes.values().find(|g| g.name == name)
    }

    /// Modifie la valeur d'un gène
    pub fn set_gene_value(&mut self, gene_id: GeneId, new_value: Value) -> bool {
        if let Some(gene) = self.genes.get_mut(&gene_id) {
            gene.value = new_value;
            true
        } else {
            false
        }
    }

    /// Calcule la taille approximative en bytes
    pub fn size_bytes(&self) -> usize {
        let mut size = 8 + 9 + 8 + 4 + 8 + 8; // id, parent, generation, fitness, created_at, last_access, access_count
        
        for chromosome in &self.chromosomes {
            size += chromosome.size_bytes(0);
        }
        
        for gene in self.genes.values() {
            size += gene.size_bytes();
        }
        
        size
    }

    /// Sérialise le genome
    pub fn serialize<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        // Header
        writer.write_all(b"DNAD")?; // magic
        writer.write_all(&[1u8])?; // version
        writer.write_all(&[0u8])?; // type (checkpoint)
        
        writer.write_all(&self.id.to_le_bytes())?;
        
        let parent_id = self.parent_id.unwrap_or(0);
        writer.write_all(&parent_id.to_le_bytes())?;
        
        writer.write_all(&self.generation.to_le_bytes())?;
        writer.write_all(&self.fitness.0.to_le_bytes())?;
        writer.write_all(&self.created_at.to_le_bytes())?;
        writer.write_all(&self.last_access.to_le_bytes())?;
        writer.write_all(&self.access_count.to_le_bytes())?;
        
        // Chromosomes
        let chromo_count = self.chromosomes.len() as u32;
        writer.write_all(&chromo_count.to_le_bytes())?;
        
        for chromosome in &self.chromosomes {
            let chromo_genes: Vec<Gene> = chromosome.genes
                .iter()
                .filter_map(|id| self.genes.get(id))
                .cloned()
                .collect();
            chromosome.serialize(writer, &chromo_genes)?;
        }
        
        // Gènes (ceux qui ne sont pas déjà sérialisés avec les chromosomes)
        let serialized_gene_ids: std::collections::HashSet<_> = self.chromosomes
            .iter()
            .flat_map(|c| c.genes.iter())
            .collect();
        
        for gene in self.genes.values() {
            if !serialized_gene_ids.contains(&gene.id) {
                gene.serialize(writer)?;
            }
        }
        
        Ok(())
    }

    /// Désérialise un genome
    pub fn deserialize<R: Read>(reader: &mut R) -> std::io::Result<Self> {
        let mut magic = [0u8; 4];
        reader.read_exact(&mut magic)?;
        if &magic != b"DNAD" {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid magic number"));
        }
        
        let mut version = [0u8; 1];
        reader.read_exact(&mut version)?;
        
        let mut type_byte = [0u8; 1];
        reader.read_exact(&mut type_byte)?;
        
        let mut id_bytes = [0u8; 8];
        reader.read_exact(&mut id_bytes)?;
        let id = u64::from_le_bytes(id_bytes);
        
        let mut parent_bytes = [0u8; 8];
        reader.read_exact(&mut parent_bytes)?;
        let parent_id = u64::from_le_bytes(parent_bytes);
        let parent_id = if parent_id == 0 { None } else { Some(parent_id) };
        
        let mut gen_bytes = [0u8; 8];
        reader.read_exact(&mut gen_bytes)?;
        let generation = u64::from_le_bytes(gen_bytes);
        
        let mut fitness_bytes = [0u8; 4];
        reader.read_exact(&mut fitness_bytes)?;
        let fitness = FitnessScore::new(f32::from_le_bytes(fitness_bytes));
        
        let mut created_bytes = [0u8; 8];
        reader.read_exact(&mut created_bytes)?;
        let created_at = u64::from_le_bytes(created_bytes);
        
        let mut access_bytes = [0u8; 8];
        reader.read_exact(&mut access_bytes)?;
        let last_access = u64::from_le_bytes(access_bytes);
        
        let mut count_bytes = [0u8; 8];
        reader.read_exact(&mut count_bytes)?;
        let access_count = u64::from_le_bytes(count_bytes);
        
        let mut chromo_count_bytes = [0u8; 4];
        reader.read_exact(&mut chromo_count_bytes)?;
        let chromo_count = u32::from_le_bytes(chromo_count_bytes) as usize;
        
        let mut chromosomes = Vec::with_capacity(chromo_count);
        let mut genes = HashMap::new();
        
        for _ in 0..chromo_count {
            let chromosome = Chromosome::deserialize(reader)?;
            chromosomes.push(chromosome);
        }
        
        // Lire les gènes restants
        while let Ok(gene) = Gene::deserialize(reader) {
            genes.insert(gene.id, gene);
        }
        
        Ok(Self {
            id,
            parent_id,
            generation,
            chromosomes,
            genes,
            fitness,
            memory_status: MemoryStatus::from_fitness(fitness.0),
            created_at,
            last_access,
            access_count,
        })
    }
}