//! Write-Ahead Log pour la durabilité

use crate::types::GenomeId;
use crate::mutation::Mutation;
use std::fs::{File, OpenOptions};
use std::io::{Write, Read, BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use parking_lot::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Entrée du WAL
#[derive(Debug, Clone)]
pub struct WalEntry {
    pub sequence: u64,
    pub timestamp: u64,
    pub mutation: Mutation,
    pub result_id: GenomeId,
}

/// Write-Ahead Log
pub struct WriteAheadLog {
    path: PathBuf,
    current_file: Mutex<BufWriter<File>>,
    current_sequence: AtomicU64,
    current_size: AtomicU64,
    max_file_size: u64,
}

impl WriteAheadLog {
    /// Crée un nouveau WAL
    pub fn new(base_path: &Path, max_file_size: u64) -> std::io::Result<Self> {
        std::fs::create_dir_all(base_path)?;
        
        let sequence = Self::find_last_sequence(base_path)?;
        
        let current_file_path = base_path.join(format!("segment_{:08x}.log", sequence));
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&current_file_path)?;
        
        Ok(Self {
            path: base_path.to_path_buf(),
            current_file: Mutex::new(BufWriter::new(file)),
            current_sequence: AtomicU64::new(sequence),
            current_size: AtomicU64::new(0),
            max_file_size,
        })
    }
    
    fn find_last_sequence(path: &Path) -> std::io::Result<u64> {
        let mut max_seq = 0u64;
        
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if let Some(hex) = name_str.strip_prefix("segment_").and_then(|s| s.strip_suffix(".log")) {
                    if let Ok(seq) = u64::from_str_radix(hex, 16) {
                        max_seq = max_seq.max(seq);
                    }
                }
            }
        }
        
        Ok(max_seq)
    }
    
    /// Append une entrée
    pub fn append(&self, entry: WalEntry) -> std::io::Result<()> {
        let mut writer = self.current_file.lock();
        
        let size = self.current_size.load(Ordering::Relaxed);
        if size > self.max_file_size {
            drop(writer);
            self.rotate()?;
            writer = self.current_file.lock();
        }
        
        let serialized = entry.serialize();
        writer.write_all(&(serialized.len() as u32).to_le_bytes())?;
        writer.write_all(&serialized)?;
        writer.flush()?;
        
        self.current_size.fetch_add(serialized.len() as u64 + 4, Ordering::Relaxed);
        
        Ok(())
    }
    
    fn rotate(&self) -> std::io::Result<()> {
        let new_seq = self.current_sequence.fetch_add(1, Ordering::Relaxed) + 1;
        let new_path = self.path.join(format!("segment_{:08x}.log", new_seq));
        
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&new_path)?;
        
        let mut writer = self.current_file.lock();
        *writer = BufWriter::new(file);
        self.current_size.store(0, Ordering::Relaxed);
        
        Ok(())
    }
    
    /// Lit toutes les entrées
    pub fn read_all(&self) -> std::io::Result<Vec<WalEntry>> {
        let mut entries = Vec::new();
        
        let mut seq = 0;
        loop {
            let path = self.path.join(format!("segment_{:08x}.log", seq));
            if !path.exists() {
                break;
            }
            
            let file = File::open(&path)?;
            let mut reader = BufReader::new(file);
            
            loop {
                let mut len_bytes = [0u8; 4];
                if reader.read_exact(&mut len_bytes).is_err() {
                    break;
                }
                let len = u32::from_le_bytes(len_bytes) as usize;
                
                let mut data = vec![0u8; len];
                if reader.read_exact(&mut data).is_err() {
                    break;
                }
                
                if let Ok(entry) = WalEntry::deserialize(&data) {
                    entries.push(entry);
                }
            }
            
            seq += 1;
        }
        
        Ok(entries)
    }
}

impl WalEntry {
    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&self.sequence.to_le_bytes());
        buf.extend_from_slice(&self.timestamp.to_le_bytes());
        buf.extend_from_slice(&self.result_id.to_le_bytes());
        // Mutation serialization simplifiée
        buf.push(0); // placeholder
        buf
    }
    
    pub fn deserialize(data: &[u8]) -> std::io::Result<Self> {
        if data.len() < 25 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid WAL entry"));
        }
        
        let sequence = u64::from_le_bytes(data[0..8].try_into().unwrap());
        let timestamp = u64::from_le_bytes(data[8..16].try_into().unwrap());
        let result_id = u64::from_le_bytes(data[16..24].try_into().unwrap());
        
        Ok(Self {
            sequence,
            timestamp,
            mutation: Mutation::Noop, // placeholder
            result_id,
        })
    }
}

impl Default for WalEntry {
    fn default() -> Self {
        Self {
            sequence: 0,
            timestamp: 0,
            mutation: Mutation::Noop,
            result_id: 0,
        }
    }
}