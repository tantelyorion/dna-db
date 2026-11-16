//! Stockage sur disque : un fichier par genome, écrit de façon atomique.
//!
//! Disposition :
//! - `checkpoints/<id>.bin` : snapshot complet du genome
//! - `deltas/<id>.delta`    : delta ayant produit ce genome à partir de son parent (historique)

use super::file_format::{read_framed, write_framed, FileType};
use super::StorageBackend;
use crate::mutation::Delta;
use crate::types::{Genome, GenomeId};
use crate::util::{invalid_data, not_found};
use parking_lot::{Mutex, RwLock};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Stockage fichier avec écriture atomique (fichier temporaire + fsync + rename).
pub struct FileStorage {
    base_path: PathBuf,
    ids: RwLock<HashSet<GenomeId>>,
    write_lock: Mutex<()>,
}

impl FileStorage {
    /// Ouvre (ou crée) le stockage et recense les genomes existants.
    pub fn new(base_path: &Path) -> io::Result<Self> {
        fs::create_dir_all(base_path.join("checkpoints"))?;
        fs::create_dir_all(base_path.join("deltas"))?;

        let storage = Self {
            base_path: base_path.to_path_buf(),
            ids: RwLock::new(HashSet::new()),
            write_lock: Mutex::new(()),
        };
        storage.scan_existing_files()?;
        Ok(storage)
    }

    fn scan_existing_files(&self) -> io::Result<()> {
        let mut ids = self.ids.write();
        for entry in fs::read_dir(self.base_path.join("checkpoints"))? {
            let path = entry?.path();
            let name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };
            if name.ends_with(".tmp") {
                // écriture interrompue : le fichier définitif précédent reste valide
                let _ = fs::remove_file(&path);
                continue;
            }
            if let Some(stem) = name.strip_suffix(".bin") {
                if let Ok(id) = stem.parse::<u64>() {
                    ids.insert(id);
                }
            }
        }
        // nettoyage des deltas temporaires
        if let Ok(entries) = fs::read_dir(self.base_path.join("deltas")) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("tmp") {
                    let _ = fs::remove_file(path);
                }
            }
        }
        Ok(())
    }

    fn checkpoint_path(&self, genome_id: GenomeId) -> PathBuf {
        self.base_path.join("checkpoints").join(format!("{}.bin", genome_id))
    }

    fn delta_path(&self, genome_id: GenomeId) -> PathBuf {
        self.base_path.join("deltas").join(format!("{}.delta", genome_id))
    }

    /// Écrit `bytes` dans `path` de façon atomique.
    fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
        let mut tmp_name = path
            .file_name()
            .map(|n| n.to_os_string())
            .unwrap_or_default();
        tmp_name.push(".tmp");
        let tmp = path.with_file_name(tmp_name);

        let mut file = File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, path)
    }

    /// Sauvegarde un genome (remplace atomiquement la version précédente).
    pub fn save_genome(&self, genome: &Genome) -> io::Result<()> {
        let mut payload = Vec::with_capacity(genome.size_bytes());
        genome.serialize(&mut payload)?;
        let mut bytes = Vec::with_capacity(payload.len() + 16);
        write_framed(&mut bytes, FileType::Checkpoint, &payload)?;

        let _guard = self.write_lock.lock();
        Self::write_atomic(&self.checkpoint_path(genome.id), &bytes)?;
        self.ids.write().insert(genome.id);
        Ok(())
    }

    /// Charge un genome.
    pub fn load_genome(&self, genome_id: GenomeId) -> io::Result<Genome> {
        if !self.exists(genome_id) {
            return Err(not_found(format!("Genome {} introuvable", genome_id)));
        }
        let bytes = fs::read(self.checkpoint_path(genome_id))?;
        let payload = read_framed(&mut bytes.as_slice(), FileType::Checkpoint)?;
        let genome = Genome::deserialize(&mut payload.as_slice())?;
        if genome.id != genome_id {
            return Err(invalid_data(format!(
                "Le fichier du genome {} contient le genome {}",
                genome_id, genome.id
            )));
        }
        Ok(genome)
    }

    /// Vrai si le genome existe.
    pub fn exists(&self, genome_id: GenomeId) -> bool {
        self.ids.read().contains(&genome_id)
    }

    /// Supprime un genome et son delta (opération idempotente).
    pub fn delete_genome(&self, genome_id: GenomeId) -> io::Result<()> {
        let _guard = self.write_lock.lock();
        remove_if_exists(&self.checkpoint_path(genome_id))?;
        remove_if_exists(&self.delta_path(genome_id))?;
        self.ids.write().remove(&genome_id);
        Ok(())
    }

    /// Sauvegarde le delta qui a produit `delta.genome_id`.
    pub fn save_delta(&self, delta: &Delta) -> io::Result<()> {
        let mut payload = Vec::new();
        delta.serialize(&mut payload)?;
        let mut bytes = Vec::with_capacity(payload.len() + 16);
        write_framed(&mut bytes, FileType::Delta, &payload)?;

        let _guard = self.write_lock.lock();
        Self::write_atomic(&self.delta_path(delta.genome_id), &bytes)
    }

    /// Charge le delta ayant produit un genome (`None` si ce genome n'en a pas, ex. genome initial).
    pub fn load_delta(&self, genome_id: GenomeId) -> io::Result<Option<Delta>> {
        let path = self.delta_path(genome_id);
        let bytes = match fs::read(&path) {
            Ok(b) => b,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        let payload = read_framed(&mut bytes.as_slice(), FileType::Delta)?;
        Ok(Some(Delta::deserialize(&mut payload.as_slice())?))
    }

    /// Identifiants de tous les genomes, triés.
    pub fn list_ids(&self) -> Vec<GenomeId> {
        let mut ids: Vec<GenomeId> = self.ids.read().iter().copied().collect();
        ids.sort_unstable();
        ids
    }

    /// Plus grand identifiant présent (0 si vide).
    pub fn max_id(&self) -> GenomeId {
        self.ids.read().iter().copied().max().unwrap_or(0)
    }
}

fn remove_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

impl StorageBackend for FileStorage {
    fn save_genome(&self, genome: &Genome) -> io::Result<()> {
        FileStorage::save_genome(self, genome)
    }

    fn load_genome(&self, genome_id: GenomeId) -> io::Result<Genome> {
        FileStorage::load_genome(self, genome_id)
    }

    fn exists(&self, genome_id: GenomeId) -> bool {
        FileStorage::exists(self, genome_id)
    }

    fn delete_genome(&self, genome_id: GenomeId) -> io::Result<()> {
        FileStorage::delete_genome(self, genome_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Chromosome, Gene, Value};

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dna_db_{}_{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn genome(id: u64) -> Genome {
        let genes = vec![Gene::new(1, "a".to_string(), Value::Int(7), 0.5)];
        let chromosome = Chromosome::new(1, "c".to_string(), vec![1], 0.0);
        Genome::try_new(id, vec![chromosome], genes).unwrap()
    }

    #[test]
    fn save_load_delete_and_reopen() {
        let dir = temp_dir("storage");
        let storage = FileStorage::new(&dir).unwrap();
        storage.save_genome(&genome(3)).unwrap();
        storage.save_genome(&genome(8)).unwrap();

        let loaded = storage.load_genome(3).unwrap();
        assert_eq!(loaded.get_gene(1).unwrap().value, Value::Int(7));

        drop(storage);
        let reopened = FileStorage::new(&dir).unwrap();
        assert_eq!(reopened.list_ids(), vec![3, 8]);
        assert_eq!(reopened.max_id(), 8);

        reopened.delete_genome(3).unwrap();
        reopened.delete_genome(3).unwrap();
        assert!(!reopened.exists(3));
        assert!(reopened.load_genome(3).is_err());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupted_checkpoint_is_detected() {
        let dir = temp_dir("corrupt");
        let storage = FileStorage::new(&dir).unwrap();
        storage.save_genome(&genome(1)).unwrap();

        let path = dir.join("checkpoints").join("1.bin");
        let mut bytes = fs::read(&path).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xFF;
        fs::write(&path, bytes).unwrap();

        assert!(storage.load_genome(1).is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}
