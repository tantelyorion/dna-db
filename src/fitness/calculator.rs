//! Calculateur de fitness.

use crate::types::{FitnessScore, Genome};
use crate::util::{clamp01, now_secs};

/// Pondération des quatre facteurs du fitness. Les poids sont normalisés au calcul :
/// seule leur proportion compte.
#[derive(Debug, Clone)]
pub struct FitnessConfig {
    /// Poids de la fréquence d'accès
    pub weight_access_frequency: f32,
    /// Poids de la récence du dernier accès
    pub weight_recency: f32,
    /// Poids de la correspondance à la requête
    pub weight_query_match: f32,
    /// Poids de la complexité structurelle
    pub weight_structural: f32,
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
    /// Configuration par défaut.
    pub fn new() -> Self {
        Self::default()
    }

    /// Définit les quatre poids (les valeurs négatives ou NaN deviennent 0).
    pub fn with_weights(mut self, freq: f32, recency: f32, query: f32, structural: f32) -> Self {
        let sane = |w: f32| if w.is_nan() || w < 0.0 { 0.0 } else { w };
        self.weight_access_frequency = sane(freq);
        self.weight_recency = sane(recency);
        self.weight_query_match = sane(query);
        self.weight_structural = sane(structural);
        self
    }

    fn total(&self) -> f32 {
        self.weight_access_frequency.max(0.0)
            + self.weight_recency.max(0.0)
            + self.weight_query_match.max(0.0)
            + self.weight_structural.max(0.0)
    }
}

/// Calculateur de fitness.
pub struct FitnessCalculator {
    config: FitnessConfig,
}

impl FitnessCalculator {
    /// Crée un calculateur avec une configuration.
    pub fn new(config: FitnessConfig) -> Self {
        Self { config }
    }

    /// Crée un calculateur avec la configuration par défaut.
    pub fn with_defaults() -> Self {
        Self::new(FitnessConfig::default())
    }

    /// Calcule le fitness d'un genome (résultat dans [0, 1]).
    pub fn compute(&self, genome: &Genome, query_match_score: Option<f32>) -> FitnessScore {
        let total = self.config.total();
        if !(total > 0.0) {
            return FitnessScore::new(0.0);
        }

        // Fréquence d'accès, saturée à 1000 accès
        let access_score = (genome.access_count as f32 / 1000.0).min(1.0);

        // Récence : décroissance exponentielle (constante de temps de 30 jours)
        let age_days = now_secs().saturating_sub(genome.last_access) as f32 / 86_400.0;
        let recency_score = (-age_days / 30.0).exp();

        // Correspondance à la requête (0.5 par défaut)
        let query_score = clamp01(query_match_score.unwrap_or(0.5));

        // Complexité structurelle, saturée à 100 chromosomes
        let structural_score = (genome.chromosomes.len() as f32 / 100.0).min(1.0);

        let raw = access_score * self.config.weight_access_frequency.max(0.0)
            + recency_score * self.config.weight_recency.max(0.0)
            + query_score * self.config.weight_query_match.max(0.0)
            + structural_score * self.config.weight_structural.max(0.0);

        FitnessScore::new(raw / total)
    }

    /// Met à jour le fitness d'un genome et son statut mémoire.
    pub fn update(&self, genome: &mut Genome, query_match_score: Option<f32>) {
        genome.fitness = self.compute(genome, query_match_score);
        genome.update_memory_status();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fitness_stays_in_unit_interval() {
        let calc = FitnessCalculator::new(FitnessConfig::new().with_weights(5.0, 5.0, 5.0, 5.0));
        let mut genome = Genome::new(1, vec![], vec![]);
        genome.access_count = 1_000_000;
        let f = calc.compute(&genome, Some(7.0)).value();
        assert!((0.0..=1.0).contains(&f));
    }

    #[test]
    fn zero_weights_give_zero() {
        let calc = FitnessCalculator::new(FitnessConfig::new().with_weights(0.0, 0.0, 0.0, 0.0));
        let genome = Genome::new(1, vec![], vec![]);
        assert_eq!(calc.compute(&genome, None).value(), 0.0);
    }

    #[test]
    fn update_syncs_memory_status() {
        let calc = FitnessCalculator::with_defaults();
        let mut genome = Genome::new(1, vec![], vec![]);
        calc.update(&mut genome, Some(1.0));
        assert_eq!(
            genome.memory_status,
            crate::types::MemoryStatus::from_fitness(genome.fitness.value())
        );
    }
}
