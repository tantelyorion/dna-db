//! Utilitaires internes : horloge, erreurs, checksum, hash stable, PRNG et E/S binaires.

use std::io::{self, Read, Write};
use std::time::{SystemTime, UNIX_EPOCH};

/// Taille maximale acceptée pour un champ lu sur disque (protège des fichiers corrompus).
pub(crate) const MAX_FIELD_BYTES: usize = 1 << 30;

/// Secondes depuis l'epoch Unix (0 si l'horloge est antérieure à 1970).
pub(crate) fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Graine pseudo-aléatoire dérivée de l'horloge.
pub(crate) fn time_seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x5DEE_CE66_D1CE_4E5B)
}

pub(crate) fn invalid_data(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.into())
}

pub(crate) fn invalid_input(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, msg.into())
}

pub(crate) fn not_found(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, msg.into())
}

/// Ramène un flottant dans [0, 1] ; NaN devient 0.
pub(crate) fn clamp01(x: f32) -> f32 {
    if x.is_nan() {
        0.0
    } else {
        x.clamp(0.0, 1.0)
    }
}

/// CRC-32 (IEEE 802.3), implémentation simple sans table.
pub(crate) fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            let mask = (!(crc & 1)).wrapping_add(1);
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

/// Hash 64 bits stable (FNV-1a + finalisation), identique sur toutes les versions de Rust.
pub(crate) fn hash64(data: &[u8]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h ^= h >> 30;
    h = h.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    h ^= h >> 27;
    h = h.wrapping_mul(0x94d0_49bb_1331_11eb);
    h ^ (h >> 31)
}

/// Petit générateur pseudo-aléatoire SplitMix64 (aucune dépendance externe).
pub(crate) struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub(crate) fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub(crate) fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Entier dans [0, n) (0 si n == 0).
    pub(crate) fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next_u64() % n as u64) as usize
        }
    }

    /// Flottant dans [0, 1).
    pub(crate) fn unit_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

pub(crate) fn read_u8<R: Read>(r: &mut R) -> io::Result<u8> {
    let mut b = [0u8; 1];
    r.read_exact(&mut b)?;
    Ok(b[0])
}

pub(crate) fn read_u32<R: Read>(r: &mut R) -> io::Result<u32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

pub(crate) fn read_u64<R: Read>(r: &mut R) -> io::Result<u64> {
    let mut b = [0u8; 8];
    r.read_exact(&mut b)?;
    Ok(u64::from_le_bytes(b))
}

pub(crate) fn read_f32<R: Read>(r: &mut R) -> io::Result<f32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(f32::from_le_bytes(b))
}

/// Lit exactement `len` octets sans pré-allouer aveuglément (sûr face à une longueur corrompue).
pub(crate) fn read_vec<R: Read>(r: &mut R, len: usize) -> io::Result<Vec<u8>> {
    if len > MAX_FIELD_BYTES {
        return Err(invalid_data("Champ trop volumineux"));
    }
    let mut v = Vec::new();
    r.by_ref().take(len as u64).read_to_end(&mut v)?;
    if v.len() != len {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "Données tronquées"));
    }
    Ok(v)
}

pub(crate) fn write_len_bytes<W: Write>(w: &mut W, data: &[u8]) -> io::Result<()> {
    let len = u32::try_from(data.len()).map_err(|_| invalid_input("Champ de plus de 4 Gio"))?;
    w.write_all(&len.to_le_bytes())?;
    w.write_all(data)
}

pub(crate) fn read_len_bytes<R: Read>(r: &mut R) -> io::Result<Vec<u8>> {
    let len = read_u32(r)? as usize;
    read_vec(r, len)
}

pub(crate) fn read_string<R: Read>(r: &mut R) -> io::Result<String> {
    String::from_utf8(read_len_bytes(r)?).map_err(|_| invalid_data("UTF-8 invalide"))
}

/// Écrit un compteur sur 4 octets en vérifiant le dépassement.
pub(crate) fn write_count<W: Write>(w: &mut W, count: usize) -> io::Result<()> {
    let n = u32::try_from(count).map_err(|_| invalid_input("Trop d'éléments"))?;
    w.write_all(&n.to_le_bytes())
}

/// Position optionnelle encodée sur 8 octets (`u64::MAX` = aucune).
pub(crate) fn write_opt_pos<W: Write>(w: &mut W, pos: Option<usize>) -> io::Result<()> {
    let v = match pos {
        Some(p) => p as u64,
        None => u64::MAX,
    };
    w.write_all(&v.to_le_bytes())
}

pub(crate) fn read_opt_pos<R: Read>(r: &mut R) -> io::Result<Option<usize>> {
    let v = read_u64(r)?;
    if v == u64::MAX {
        Ok(None)
    } else {
        usize::try_from(v)
            .map(Some)
            .map_err(|_| invalid_data("Position invalide"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_known_vector() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn hash64_is_stable_and_discriminating() {
        assert_eq!(hash64(b"abc"), hash64(b"abc"));
        assert_ne!(hash64(b"abc"), hash64(b"abd"));
    }

    #[test]
    fn read_vec_rejects_truncated_data() {
        let data = [1u8, 2, 3];
        let mut r = &data[..];
        assert!(read_vec(&mut r, 10).is_err());
    }
}
