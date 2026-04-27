//! Calculateur de fitness

use crate::types::*;
use std::time::{SystemTime, UNIX_EPOCH};

/// Configuration du calcul de fitness
#[derive(Debug, Clone)]
pub struct FitnessConfig {
    pub weight_access_frequency: f32,
    weight_recency: f32,
    weight_query_match: f32,
    weight_structural: f32,
}

impl Default for FitnessConfig {
    fn default() -> Self {
        Self {
            weight_access_frequency: 0.4,
            weight_recency: 0.3,
            weight_query_match: 0.2,
            weight_structural: 0.1,
        }
    }
}

impl FitnessConfig {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn with_weights(mut self, freq: f32, recency: f32, query: f32, structural: f32) -> Self {
        self.weight_access_frequency = freq;
        self.weight_recency = recency;
        self.weight_query_match = query;
        self.weight_structural = structural;
        self
    }
}

/// Calculateur de fitness
pub struct FitnessCalculator {
    config: FitnessConfig,
}

impl FitnessCalculator {
    pub fn new(config: FitnessConfig) -> Self {
        Self { config }
    }
    
    pub fn with_defaults() -> Self {
        Self::new(FitnessConfig::default())
    }
    
    /// Calcule le fitness d'un genome
    pub fn compute(&self, genome: &Genome, query_match_score: Option<f32>) -> FitnessScore {
        // Facteur 1: Fréquence d'accès (normalisée à 1.0)
        let access_score = (genome.access_count as f32 / 1000.0).min(1.0);
        
        // Facteur 2: Récence (décroissance exponentielle sur 30 jours)
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let age_days = (now.saturating_sub(genome.last_access)) as f32 / 86400.0;
        let recency_score = (-age_days / 30.0).exp();
        
        // Facteur 3: Score de correspondance à la requête
        let query_score = query_match_score.unwrap_or(0.5);
        
        // Facteur 4: Complexité structurelle (entre 0 et 1)
        let structural_score = (genome.chromosomes.len() as f32 / 100.0).min(1.0);
        
        let raw_score = 
            access_score * self.config.weight_access_frequency +
            recency_score * self.config.weight_recency +
            query_score * self.config.weight_query_match +
            structural_score * self.config.weight_structural;
        
        FitnessScore::new(raw_score)
    }
    
    /// Met à jour le fitness d'un genome et son statut mémoire
    pub fn update(&self, genome: &mut Genome, query_match_score: Option<f32>) {
        genome.fitness = self.compute(genome, query_match_score);
        genome.update_memory_status();
    }
}