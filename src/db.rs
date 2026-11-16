//! Interface principale de DNA DB.

use crate::cache::LRUCache;
use crate::fitness::{FitnessCalculator, FitnessConfig};
use crate::index::{GenomeMeta, GlobalIndex};
use crate::mutation::{Delta, Mutation, MutationEngine};
use crate::storage::{FileStorage, WalEntry, WriteAheadLog};
use crate::types::*;
use crate::util::{invalid_input, not_found, now_secs};
use parking_lot::Mutex;
use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Configuration de la base de données.
#[derive(Debug, Clone)]
pub struct DNADatabaseConfig {
    /// Dossier de données
    pub data_dir: PathBuf,
    /// Nombre maximal de genomes gardés en cache mémoire
    pub cache_size: usize,
    /// Taille du WAL (octets) au-delà de laquelle il est tronqué
    pub wal_max_file_size: u64,
    /// Force chaque entrée du WAL sur disque (durable, plus lent)
    pub wal_sync: bool,
    /// Reconstruit l'index en mémoire à l'ouverture (nécessaire pour `top_genomes`,
    /// `search_similar` et `gc` sur des données déjà présentes)
    pub rebuild_index_on_open: bool,
    /// Seuil de fitness sous lequel un genome sans enfant est nettoyé par `gc`
    pub gc_threshold: f32,
    /// Pondération du fitness
    pub fitness_config: FitnessConfig,
}

impl Default for DNADatabaseConfig {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from("dna_data"),
            cache_size: 1000,
            wal_max_file_size: 10 * 1024 * 1024,
            wal_sync: true,
            rebuild_index_on_open: true,
            gc_threshold: 0.3,
            fitness_config: FitnessConfig::default(),
        }
    }
}

/// Base de données DNA DB (thread-safe : toutes les méthodes prennent `&self`).
pub struct DNADatabase {
    config: DNADatabaseConfig,
    storage: FileStorage,
    wal: WriteAheadLog,
    cache: LRUCache<GenomeId, Genome>,
    index: GlobalIndex,
    engine: MutationEngine,
    fitness_calculator: FitnessCalculator,
    wal_sequence: AtomicU64,
    /// Sérialise les opérations qui modifient l'état (mutations, fitness, suppression, gc)
    write_lock: Mutex<()>,
}

impl DNADatabase {
    /// Ouvre (ou crée) une base de données : rejoue le WAL si un crash a interrompu
    /// des mutations, puis reconstruit l'index si la configuration le demande.
    pub fn new(config: DNADatabaseConfig) -> io::Result<Self> {
        std::fs::create_dir_all(&config.data_dir)?;

        let storage = FileStorage::new(&config.data_dir)?;
        let wal = WriteAheadLog::new(&config.data_dir.join("wal"), config.wal_max_file_size, config.wal_sync)?;
        let engine = MutationEngine::with_start_id(storage.max_id().saturating_add(1));
        let cache = LRUCache::new(config.cache_size);
        let fitness_calculator = FitnessCalculator::new(config.fitness_config.clone());

        let db = Self {
            config,
            storage,
            wal,
            cache,
            index: GlobalIndex::new(),
            engine,
            fitness_calculator,
            wal_sequence: AtomicU64::new(0),
            write_lock: Mutex::new(()),
        };

        db.recover()?;
        if db.config.rebuild_index_on_open {
            db.rebuild_index()?;
        }
        Ok(db)
    }

    /// Alias de [`DNADatabase::new`].
    pub fn open(config: DNADatabaseConfig) -> io::Result<Self> {
        Self::new(config)
    }

    /// Builder pattern.
    pub fn builder() -> DNADatabaseBuilder {
        DNADatabaseBuilder::new()
    }

    /// Rejoue les mutations du WAL dont le résultat n'a pas été sauvegardé.
    /// Retourne le nombre de mutations rejouées.
    fn recover(&self) -> io::Result<usize> {
        let entries = self.wal.read_all()?;
        let mut replayed = 0usize;
        let mut next_sequence = 0u64;

        for entry in &entries {
            next_sequence = next_sequence.max(entry.sequence.saturating_add(1));
            self.engine.reserve_up_to(entry.result_id);

            if self.storage.exists(entry.result_id) {
                continue;
            }
            let parent_id = match entry.mutation.genome_id() {
                Some(id) => id,
                None => continue,
            };
            if !self.storage.exists(parent_id) {
                continue;
            }
            let parent = self.storage.load_genome(parent_id)?;
            if let Ok((child, delta)) = self.engine.apply_mutation_with_id(&parent, &entry.mutation, entry.result_id) {
                self.storage.save_genome(&child)?;
                self.storage.save_delta(&delta)?;
                replayed += 1;
            }
        }

        self.wal_sequence.store(next_sequence, Ordering::Relaxed);
        if self.wal.size() > 0 {
            self.wal.reset()?;
        }
        Ok(replayed)
    }

    /// Reconstruit l'index en mémoire depuis le disque. Les genomes illisibles
    /// (fichier corrompu) sont ignorés. Retourne le nombre de genomes indexés.
    pub fn rebuild_index(&self) -> io::Result<usize> {
        let mut count = 0;
        for id in self.storage.list_ids() {
            if let Ok(genome) = self.storage.load_genome(id) {
                self.index_genome(&genome);
                count += 1;
            }
        }
        Ok(count)
    }

    fn index_genome(&self, genome: &Genome) {
        self.index.index_genes(genome);
        self.index.upsert(GenomeMeta {
            id: genome.id,
            parent_id: genome.parent_id,
            generation: genome.generation,
            fitness: genome.fitness,
            signature: self.index.signature_generator().signature(genome),
            memory_status: genome.memory_status,
        });
    }

    /// Lecture sans effet de bord (ni accès enregistré, ni mise en cache).
    fn fetch(&self, genome_id: GenomeId) -> io::Result<Option<Genome>> {
        if let Some(cached) = self.cache.get(&genome_id) {
            return Ok(Some((*cached).clone()));
        }
        if self.storage.exists(genome_id) {
            Ok(Some(self.storage.load_genome(genome_id)?))
        } else {
            Ok(None)
        }
    }

    /// Crée un genome de première génération après validation de sa cohérence.
    pub fn create_genome(&self, chromosomes: Vec<Chromosome>, genes: Vec<Gene>) -> io::Result<GenomeId> {
        let id = self.engine.next_id();
        let genome = Genome::try_new(id, chromosomes, genes)?;

        self.storage.save_genome(&genome)?;
        self.index_genome(&genome);
        self.cache.insert(id, genome);
        Ok(id)
    }

    /// Applique une mutation : le genome parent reste intact, un enfant est créé.
    /// Retourne l'identifiant de l'enfant.
    pub fn mutate(&self, mutation: Mutation) -> io::Result<GenomeId> {
        let genome_id = mutation
            .genome_id()
            .ok_or_else(|| invalid_input("Mutation sans genome cible (Noop)"))?;

        let _guard = self.write_lock.lock();

        let parent = self
            .fetch(genome_id)?
            .ok_or_else(|| not_found(format!("Genome parent {} introuvable", genome_id)))?;

        // 1. calcul en mémoire (une mutation invalide échoue ici, sans rien écrire)
        let (child, delta) = self.engine.apply_mutation(&parent, &mutation)?;
        let child_id = child.id;

        // 2. journalisation avant écriture
        let entry = WalEntry {
            sequence: self.wal_sequence.fetch_add(1, Ordering::Relaxed),
            timestamp: now_secs(),
            mutation,
            result_id: child_id,
        };
        self.wal.append(&entry)?;

        // 3. persistance du genome et de son delta
        self.storage.save_genome(&child)?;
        self.storage.save_delta(&delta)?;

        // 4. index, cache, puis troncature du WAL devenu inutile
        self.index_genome(&child);
        self.cache.insert(child_id, child);
        if self.wal.needs_truncate() {
            self.wal.reset()?;
        }
        Ok(child_id)
    }

    /// Applique une mutation aléatoire valide au genome et retourne l'enfant.
    pub fn evolve_random(&self, genome_id: GenomeId) -> io::Result<GenomeId> {
        let genome = self
            .fetch(genome_id)?
            .ok_or_else(|| not_found(format!("Genome {} introuvable", genome_id)))?;
        let mutation = self.engine.random_mutation(&genome);
        self.mutate(mutation)
    }

    /// Récupère un genome et enregistre l'accès.
    pub fn get_genome(&self, genome_id: GenomeId) -> io::Result<Option<Genome>> {
        let mut genome = match self.fetch(genome_id)? {
            Some(g) => g,
            None => return Ok(None),
        };
        genome.record_access();
        self.cache.insert(genome_id, genome.clone());
        if self.index.get_meta(genome_id).is_none() {
            self.index_genome(&genome);
        }
        Ok(Some(genome))
    }

    /// Vrai si le genome existe.
    pub fn contains(&self, genome_id: GenomeId) -> bool {
        self.storage.exists(genome_id)
    }

    /// Nombre de genomes stockés.
    pub fn len(&self) -> usize {
        self.storage.list_ids().len()
    }

    /// Vrai si la base ne contient aucun genome.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Lignée d'un genome : lui-même puis ses ancêtres, du plus récent au plus ancien
    /// (s'arrête si un ancêtre a été supprimé par `gc` ou `delete_genome`).
    pub fn history(&self, genome_id: GenomeId) -> io::Result<Vec<GenomeId>> {
        let mut chain = Vec::new();
        let mut seen = HashSet::new();
        let mut current = Some(genome_id);
        while let Some(id) = current {
            if !seen.insert(id) {
                break;
            }
            match self.fetch(id)? {
                Some(genome) => {
                    chain.push(id);
                    current = genome.parent_id;
                }
                None => break,
            }
        }
        Ok(chain)
    }

    /// Delta ayant produit un genome (`None` pour un genome initial).
    pub fn get_delta(&self, genome_id: GenomeId) -> io::Result<Option<Delta>> {
        self.storage.load_delta(genome_id)
    }

    /// Genomes génétiquement proches de `target_id` (similarité ≥ `threshold`, entre 0 et 1),
    /// du plus au moins proche. Le genome cible est exclu.
    pub fn search_similar(&self, target_id: GenomeId, threshold: f32) -> io::Result<Vec<GenomeId>> {
        let target = match self.fetch(target_id)? {
            Some(g) => g,
            None => return Ok(Vec::new()),
        };
        let signature = self.index.signature_generator().signature(&target);
        let threshold = if threshold.is_nan() { 0.0 } else { threshold.clamp(0.0, 1.0) };
        Ok(self
            .index
            .search_similar(signature, threshold, Some(target_id))
            .into_iter()
            .map(|(id, _)| id)
            .collect())
    }

    /// Recalcule et sauvegarde le fitness d'un genome.
    pub fn update_fitness(&self, genome_id: GenomeId, query_match_score: Option<f32>) -> io::Result<()> {
        let _guard = self.write_lock.lock();
        if let Some(mut genome) = self.fetch(genome_id)? {
            self.fitness_calculator.update(&mut genome, query_match_score);
            self.storage.save_genome(&genome)?;
            self.index_genome(&genome);
            self.cache.insert(genome_id, genome);
        }
        Ok(())
    }

    /// Les `n` genomes au meilleur fitness (d'après l'index en mémoire).
    pub fn top_genomes(&self, n: usize) -> Vec<GenomeId> {
        self.index.top_fitness(n)
    }

    /// Supprime un genome (ses enfants gardent leur `parent_id`, la lignée est alors coupée).
    pub fn delete_genome(&self, genome_id: GenomeId) -> io::Result<()> {
        let _guard = self.write_lock.lock();
        self.storage.delete_genome(genome_id)?;
        self.index.remove(genome_id);
        self.cache.remove(&genome_id);
        // une mutation journalisée ne doit pas ressusciter ce genome après un crash
        if self.wal.size() > 0 {
            self.wal.reset()?;
        }
        Ok(())
    }

    /// Trouve les genomes dont au moins un gène nommé `gene_name` vaut exactement `value`.
    ///
    /// Utilise l'index secondaire (candidats puis vérification). Si l'index n'est pas
    /// reconstruit à l'ouverture (`rebuild_index_on_open = false`), parcourt tout le
    /// stockage. Les genomes illisibles sont ignorés.
    pub fn find_by_gene(&self, gene_name: &str, value: &Value) -> io::Result<Vec<GenomeId>> {
        let candidates = if self.config.rebuild_index_on_open {
            self.index.find_gene_candidates(gene_name, value)
        } else {
            self.storage.list_ids()
        };

        let mut found = Vec::new();
        for id in candidates {
            let genome = match self.fetch(id) {
                Ok(Some(genome)) => genome,
                _ => continue,
            };
            if genome.genes.values().any(|g| g.name == gene_name && &g.value == value) {
                found.push(id);
            }
        }
        Ok(found)
    }

    /// Lit un genome sans enregistrer d'accès (n'influence pas le fitness).
    pub fn peek_genome(&self, genome_id: GenomeId) -> io::Result<Option<Genome>> {
        self.fetch(genome_id)
    }

    /// Identifiants de tous les genomes stockés, triés.
    pub fn list_ids(&self) -> Vec<GenomeId> {
        self.storage.list_ids()
    }

    /// Nettoie les fossiles : supprime les genomes sans enfant dont le fitness est
    /// inférieur à `gc_threshold`. Retourne le nombre de genomes supprimés.
    pub fn gc(&self) -> io::Result<usize> {
        let _guard = self.write_lock.lock();
        let metas = self.index.all_metas();
        let parents: HashSet<GenomeId> = metas.iter().filter_map(|m| m.parent_id).collect();

        let mut removed = 0;
        for meta in metas {
            if meta.fitness.value() < self.config.gc_threshold && !parents.contains(&meta.id) {
                self.storage.delete_genome(meta.id)?;
                self.index.remove(meta.id);
                self.cache.remove(&meta.id);
                removed += 1;
            }
        }
        if removed > 0 && self.wal.size() > 0 {
            self.wal.reset()?;
        }
        Ok(removed)
    }
}

/// Builder pour [`DNADatabase`].
pub struct DNADatabaseBuilder {
    config: DNADatabaseConfig,
}

impl DNADatabaseBuilder {
    /// Builder avec la configuration par défaut.
    pub fn new() -> Self {
        Self { config: DNADatabaseConfig::default() }
    }

    /// Dossier de données.
    pub fn data_dir<P: AsRef<Path>>(mut self, dir: P) -> Self {
        self.config.data_dir = dir.as_ref().to_path_buf();
        self
    }

    /// Taille du cache (en genomes).
    pub fn cache_size(mut self, size: usize) -> Self {
        self.config.cache_size = size;
        self
    }

    /// Taille du WAL au-delà de laquelle il est tronqué.
    pub fn wal_max_file_size(mut self, bytes: u64) -> Self {
        self.config.wal_max_file_size = bytes;
        self
    }

    /// Active ou non la synchronisation disque à chaque entrée du WAL.
    pub fn wal_sync(mut self, sync: bool) -> Self {
        self.config.wal_sync = sync;
        self
    }

    /// Reconstruit ou non l'index à l'ouverture.
    pub fn rebuild_index_on_open(mut self, rebuild: bool) -> Self {
        self.config.rebuild_index_on_open = rebuild;
        self
    }

    /// Seuil de fitness du `gc`.
    pub fn gc_threshold(mut self, threshold: f32) -> Self {
        self.config.gc_threshold = threshold;
        self
    }

    /// Pondération du fitness.
    pub fn fitness_config(mut self, config: FitnessConfig) -> Self {
        self.config.fitness_config = config;
        self
    }

    /// Ouvre la base.
    pub fn build(self) -> io::Result<DNADatabase> {
        DNADatabase::new(self.config)
    }
}

impl Default for DNADatabaseBuilder {
    fn default() -> Self {
        Self::new()
    }
}
