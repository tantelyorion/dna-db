//! Format binaire des fichiers : en-tête + longueur + CRC32 + charge utile.
//!
//! Disposition d'un fichier :
//! `magic "DNAD" (4) | version (1) | type (1) | réservé (2) | longueur u64 | crc32 u32 | charge utile`

use crate::util::{crc32, invalid_data, read_u32, read_u64, read_u8, read_vec};
use std::io::{self, Read, Write};

/// Type de fichier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FileType {
    /// Genome complet
    Checkpoint = 0,
    /// Delta de mutation
    Delta = 1,
    /// Index (réservé)
    Index = 2,
    /// Segment WAL (réservé)
    Wal = 3,
}

impl FileType {
    /// Convertit un octet en type de fichier.
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(FileType::Checkpoint),
            1 => Some(FileType::Delta),
            2 => Some(FileType::Index),
            3 => Some(FileType::Wal),
            _ => None,
        }
    }
}

/// En-tête de fichier (8 octets).
#[derive(Debug, Clone, Copy)]
pub struct FileHeader {
    magic: [u8; 4],
    version: u8,
    file_type: u8,
    reserved: [u8; 2],
}

impl FileHeader {
    /// Nombre magique.
    pub const MAGIC: [u8; 4] = *b"DNAD";
    /// Version du format (2 : en-tête unique, CRC32, tous les gènes sérialisés).
    pub const VERSION: u8 = 2;
    /// Taille de l'en-tête en octets.
    pub const SIZE: usize = 8;

    /// Crée un en-tête.
    pub fn new(file_type: FileType) -> Self {
        Self {
            magic: Self::MAGIC,
            version: Self::VERSION,
            file_type: file_type as u8,
            reserved: [0, 0],
        }
    }

    /// Type de fichier déclaré.
    pub fn file_type(&self) -> Option<FileType> {
        FileType::from_u8(self.file_type)
    }

    /// Vérifie le nombre magique et la version.
    pub fn validate(&self) -> bool {
        self.magic == Self::MAGIC && self.version == Self::VERSION
    }

    /// Écrit l'en-tête.
    pub fn write<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        writer.write_all(&self.magic)?;
        writer.write_all(&[self.version, self.file_type])?;
        writer.write_all(&self.reserved)
    }

    /// Lit l'en-tête.
    pub fn read<R: Read>(reader: &mut R) -> io::Result<Self> {
        let mut magic = [0u8; 4];
        reader.read_exact(&mut magic)?;
        let version = read_u8(reader)?;
        let file_type = read_u8(reader)?;
        let mut reserved = [0u8; 2];
        reader.read_exact(&mut reserved)?;
        Ok(Self { magic, version, file_type, reserved })
    }
}

/// Écrit un fichier complet : en-tête, longueur, CRC32 et charge utile.
pub fn write_framed<W: Write>(writer: &mut W, file_type: FileType, payload: &[u8]) -> io::Result<()> {
    FileHeader::new(file_type).write(writer)?;
    writer.write_all(&(payload.len() as u64).to_le_bytes())?;
    writer.write_all(&crc32(payload).to_le_bytes())?;
    writer.write_all(payload)
}

/// Lit un fichier complet et retourne sa charge utile après vérification
/// de l'en-tête, du type attendu et du CRC32.
pub fn read_framed<R: Read>(reader: &mut R, expected: FileType) -> io::Result<Vec<u8>> {
    let header = FileHeader::read(reader)?;
    if !header.validate() {
        return Err(invalid_data("En-tête invalide ou version de format non supportée"));
    }
    if header.file_type() != Some(expected) {
        return Err(invalid_data("Type de fichier inattendu"));
    }
    let len = read_u64(reader)?;
    let len = usize::try_from(len).map_err(|_| invalid_data("Longueur invalide"))?;
    let expected_crc = read_u32(reader)?;
    let payload = read_vec(reader, len)?;
    if crc32(&payload) != expected_crc {
        return Err(invalid_data("Checksum invalide : fichier corrompu"));
    }
    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framed_roundtrip() {
        let mut buf = Vec::new();
        write_framed(&mut buf, FileType::Checkpoint, b"hello").unwrap();
        assert_eq!(buf.len(), FileHeader::SIZE + 8 + 4 + 5);
        let payload = read_framed(&mut buf.as_slice(), FileType::Checkpoint).unwrap();
        assert_eq!(payload, b"hello");
    }

    #[test]
    fn corruption_and_wrong_type_are_detected() {
        let mut buf = Vec::new();
        write_framed(&mut buf, FileType::Delta, b"hello").unwrap();
        assert!(read_framed(&mut buf.as_slice(), FileType::Checkpoint).is_err());

        let last = buf.len() - 1;
        buf[last] ^= 0xFF;
        assert!(read_framed(&mut buf.as_slice(), FileType::Delta).is_err());
    }
}
