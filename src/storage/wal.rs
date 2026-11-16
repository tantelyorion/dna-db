//! Write-Ahead Log : chaque mutation est journalisée (avec CRC32) avant d'être appliquée,
//! ce qui permet de rejouer les mutations interrompues par un crash.
//!
//! Format d'une entrée : `longueur u32 | crc32 u32 | charge utile`.
//! Une entrée tronquée ou corrompue en fin de segment (crash en cours d'écriture) est ignorée.

use crate::mutation::Mutation;
use crate::types::GenomeId;
use crate::util::{crc32, invalid_data, read_u64};
use parking_lot::Mutex;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

/// Entrée du WAL.
#[derive(Debug, Clone)]
pub struct WalEntry {
    /// Numéro de séquence
    pub sequence: u64,
    /// Horodatage (secondes Unix)
    pub timestamp: u64,
    /// Mutation journalisée
    pub mutation: Mutation,
    /// Identifiant du genome produit par la mutation
    pub result_id: GenomeId,
}

impl WalEntry {
    /// Sérialise l'entrée (la mutation est écrite en entier).
    pub fn serialize(&self) -> io::Result<Vec<u8>> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&self.sequence.to_le_bytes());
        buf.extend_from_slice(&self.timestamp.to_le_bytes());
        buf.extend_from_slice(&self.result_id.to_le_bytes());
        self.mutation.serialize(&mut buf)?;
        Ok(buf)
    }

    /// Désérialise une entrée.
    pub fn deserialize(data: &[u8]) -> io::Result<Self> {
        let mut reader = data;
        let sequence = read_u64(&mut reader)?;
        let timestamp = read_u64(&mut reader)?;
        let result_id = read_u64(&mut reader)?;
        let mutation = Mutation::deserialize(&mut reader)?;
        if !reader.is_empty() {
            return Err(invalid_data("Octets en trop dans l'entrée WAL"));
        }
        Ok(Self { sequence, timestamp, mutation, result_id })
    }
}

impl Default for WalEntry {
    fn default() -> Self {
        Self { sequence: 0, timestamp: 0, mutation: Mutation::Noop, result_id: 0 }
    }
}

struct WalState {
    writer: BufWriter<File>,
    segment: u64,
    size: u64,
}

/// Write-Ahead Log à segment actif unique, tronqué explicitement via [`WriteAheadLog::reset`].
pub struct WriteAheadLog {
    path: PathBuf,
    state: Mutex<WalState>,
    max_file_size: u64,
    sync_on_append: bool,
}

fn segment_path(dir: &Path, segment: u64) -> PathBuf {
    dir.join(format!("segment_{:08x}.log", segment))
}

fn open_segment(dir: &Path, segment: u64) -> io::Result<(BufWriter<File>, u64)> {
    let file = OpenOptions::new().create(true).append(true).open(segment_path(dir, segment))?;
    let size = file.metadata()?.len();
    Ok((BufWriter::new(file), size))
}

impl WriteAheadLog {
    /// Ouvre (ou crée) le WAL dans `base_path`.
    ///
    /// `max_file_size` : taille à partir de laquelle [`WriteAheadLog::needs_truncate`] devient vrai.
    /// `sync_on_append` : si vrai, chaque entrée est forcée sur disque (`fdatasync`).
    pub fn new(base_path: &Path, max_file_size: u64, sync_on_append: bool) -> io::Result<Self> {
        fs::create_dir_all(base_path)?;
        let segment = Self::list_segments_in(base_path)?.last().copied().unwrap_or(0);
        let (writer, size) = open_segment(base_path, segment)?;
        Ok(Self {
            path: base_path.to_path_buf(),
            state: Mutex::new(WalState { writer, segment, size }),
            max_file_size,
            sync_on_append,
        })
    }

    fn list_segments_in(dir: &Path) -> io::Result<Vec<u64>> {
        let mut segments = Vec::new();
        for entry in fs::read_dir(dir)? {
            let name = entry?.file_name();
            let name = name.to_string_lossy();
            if let Some(hex) = name.strip_prefix("segment_").and_then(|s| s.strip_suffix(".log")) {
                if let Ok(seq) = u64::from_str_radix(hex, 16) {
                    segments.push(seq);
                }
            }
        }
        segments.sort_unstable();
        Ok(segments)
    }

    /// Ajoute une entrée au journal.
    pub fn append(&self, entry: &WalEntry) -> io::Result<()> {
        let payload = entry.serialize()?;
        let len = u32::try_from(payload.len()).map_err(|_| invalid_data("Entrée WAL trop grande"))?;

        let mut state = self.state.lock();
        state.writer.write_all(&len.to_le_bytes())?;
        state.writer.write_all(&crc32(&payload).to_le_bytes())?;
        state.writer.write_all(&payload)?;
        state.writer.flush()?;
        if self.sync_on_append {
            state.writer.get_ref().sync_data()?;
        }
        state.size += 8 + payload.len() as u64;
        Ok(())
    }

    /// Taille en octets du segment actif.
    pub fn size(&self) -> u64 {
        self.state.lock().size
    }

    /// Vrai si le journal dépasse la taille maximale configurée.
    pub fn needs_truncate(&self) -> bool {
        self.size() >= self.max_file_size
    }

    /// Lit toutes les entrées valides, dans l'ordre. S'arrête au premier cadre
    /// tronqué ou corrompu de chaque segment.
    pub fn read_all(&self) -> io::Result<Vec<WalEntry>> {
        self.state.lock().writer.flush()?;

        let mut entries = Vec::new();
        for segment in Self::list_segments_in(&self.path)? {
            let data = fs::read(segment_path(&self.path, segment))?;
            let mut pos = 0usize;
            while pos + 8 <= data.len() {
                let len = u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
                let crc = u32::from_le_bytes([data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]]);
                let start = pos + 8;
                let end = match start.checked_add(len) {
                    Some(e) if e <= data.len() => e,
                    _ => break,
                };
                let payload = &data[start..end];
                if crc32(payload) != crc {
                    break;
                }
                match WalEntry::deserialize(payload) {
                    Ok(entry) => entries.push(entry),
                    Err(_) => break,
                }
                pos = end;
            }
        }
        Ok(entries)
    }

    /// Vide le journal : démarre un nouveau segment puis supprime les anciens.
    /// À n'appeler que lorsque toutes les mutations journalisées sont déjà sauvegardées.
    pub fn reset(&self) -> io::Result<()> {
        let mut state = self.state.lock();
        state.writer.flush()?;

        let new_segment = state.segment + 1;
        let (writer, size) = open_segment(&self.path, new_segment)?;
        state.writer = writer; // ferme l'ancien fichier avant suppression (nécessaire sous Windows)
        state.segment = new_segment;
        state.size = size;

        for segment in Self::list_segments_in(&self.path)? {
            if segment != new_segment {
                let _ = fs::remove_file(segment_path(&self.path, segment));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Value;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dna_db_wal_{}_{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn entry(seq: u64) -> WalEntry {
        WalEntry {
            sequence: seq,
            timestamp: 1,
            mutation: Mutation::SubstituteGene {
                genome_id: 1,
                chromosome_id: 1,
                gene_id: 1,
                new_value: Value::Int(seq as i64),
            },
            result_id: 100 + seq,
        }
    }

    #[test]
    fn append_read_and_reopen() {
        let dir = temp_dir("rw");
        let wal = WriteAheadLog::new(&dir, 1 << 20, false).unwrap();
        wal.append(&entry(0)).unwrap();
        wal.append(&entry(1)).unwrap();
        drop(wal);

        let wal = WriteAheadLog::new(&dir, 1 << 20, false).unwrap();
        wal.append(&entry(2)).unwrap();
        let entries = wal.read_all().unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[2].result_id, 102);
        assert!(matches!(entries[0].mutation, Mutation::SubstituteGene { .. }));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn torn_tail_is_ignored_and_reset_empties_the_log() {
        let dir = temp_dir("torn");
        let wal = WriteAheadLog::new(&dir, 1 << 20, false).unwrap();
        wal.append(&entry(0)).unwrap();
        wal.append(&entry(1)).unwrap();
        drop(wal);

        // simule un crash au milieu de l'écriture de la dernière entrée
        let segment = segment_path(&dir, 0);
        let mut data = fs::read(&segment).unwrap();
        data.truncate(data.len() - 5);
        fs::write(&segment, data).unwrap();

        let wal = WriteAheadLog::new(&dir, 1 << 20, false).unwrap();
        assert_eq!(wal.read_all().unwrap().len(), 1);

        wal.reset().unwrap();
        assert_eq!(wal.size(), 0);
        assert!(wal.read_all().unwrap().is_empty());
        let _ = fs::remove_dir_all(&dir);
    }
}
