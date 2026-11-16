//! Gène : plus petite unité d'information.

use super::value::Value;
use crate::util::{clamp01, read_f32, read_string, read_u64, write_len_bytes};
use std::io::{self, Read, Write};

/// Identifiant de gène.
pub type GeneId = u64;

/// Gène : une valeur nommée avec un poids d'importance.
#[derive(Debug, Clone, PartialEq)]
pub struct Gene {
    /// Identifiant unique (dans un genome)
    pub id: GeneId,
    /// Nom du gène
    pub name: String,
    /// Valeur informationnelle
    pub value: Value,
    /// Poids d'importance (0.0 à 1.0)
    pub weight: f32,
}

impl Gene {
    /// Crée un nouveau gène (le poids est ramené dans [0, 1]).
    pub fn new(id: GeneId, name: String, value: Value, weight: f32) -> Self {
        Self {
            id,
            name,
            value,
            weight: clamp01(weight),
        }
    }

    /// Crée un gène avec le poids par défaut (0.5).
    pub fn with_default_weight(id: GeneId, name: String, value: Value) -> Self {
        Self::new(id, name, value, 0.5)
    }

    /// Taille exacte en octets une fois sérialisé.
    pub fn size_bytes(&self) -> usize {
        8 + 4 + self.name.len() + self.value.size_bytes() + 4
    }

    /// Sérialise le gène.
    pub fn serialize<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        writer.write_all(&self.id.to_le_bytes())?;
        write_len_bytes(writer, self.name.as_bytes())?;
        self.value.serialize(writer)?;
        writer.write_all(&self.weight.to_le_bytes())
    }

    /// Désérialise un gène.
    pub fn deserialize<R: Read>(reader: &mut R) -> io::Result<Self> {
        let id = read_u64(reader)?;
        let name = read_string(reader)?;
        let value = Value::deserialize(reader)?;
        let weight = read_f32(reader)?;
        Ok(Self::new(id, name, value, weight))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gene_roundtrip_and_size() {
        let g = Gene::new(7, "nom".to_string(), Value::Int(5), 0.25);
        let mut buf = Vec::new();
        g.serialize(&mut buf).unwrap();
        assert_eq!(buf.len(), g.size_bytes());
        assert_eq!(Gene::deserialize(&mut buf.as_slice()).unwrap(), g);
    }

    #[test]
    fn weight_is_clamped() {
        assert_eq!(Gene::new(1, "a".into(), Value::Null, 7.0).weight, 1.0);
        assert_eq!(Gene::new(1, "a".into(), Value::Null, f32::NAN).weight, 0.0);
    }
}
