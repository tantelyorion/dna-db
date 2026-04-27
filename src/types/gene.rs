//! Gène - unité de base d'information

use super::value::Value;
use std::io::{Read, Write};

/// Identifiant de gène
pub type GeneId = u64;

/// Gène - plus petite unité d'information
#[derive(Debug, Clone)]
pub struct Gene {
    /// Identifiant unique
    pub id: GeneId,
    /// Nom du gène
    pub name: String,
    /// Valeur informationnelle
    pub value: Value,
    /// Poids d'importance (0.0 à 1.0)
    pub weight: f32,
}

impl Gene {
    /// Crée un nouveau gène
    pub fn new(id: GeneId, name: String, value: Value, weight: f32) -> Self {
        Self {
            id,
            name,
            value,
            weight: weight.clamp(0.0, 1.0),
        }
    }

    /// Crée un gène avec poids par défaut (0.5)
    pub fn with_default_weight(id: GeneId, name: String, value: Value) -> Self {
        Self::new(id, name, value, 0.5)
    }

    /// Retourne la taille approximative en bytes
    pub fn size_bytes(&self) -> usize {
        8 + // id
        self.name.len() + 4 + // name length + name
        self.value.size_bytes() + 1 + // value + type
        4 // weight (f32)
    }

    /// Sérialise le gène
    pub fn serialize<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writer.write_all(&self.id.to_le_bytes())?;
        
        let name_len = self.name.len() as u32;
        writer.write_all(&name_len.to_le_bytes())?;
        writer.write_all(self.name.as_bytes())?;
        
        self.value.serialize(writer)?;
        writer.write_all(&self.weight.to_le_bytes())?;
        
        Ok(())
    }

    /// Désérialise un gène
    pub fn deserialize<R: Read>(reader: &mut R) -> std::io::Result<Self> {
        let mut id_bytes = [0u8; 8];
        reader.read_exact(&mut id_bytes)?;
        let id = u64::from_le_bytes(id_bytes);
        
        let mut name_len_bytes = [0u8; 4];
        reader.read_exact(&mut name_len_bytes)?;
        let name_len = u32::from_le_bytes(name_len_bytes) as usize;
        
        let mut name_bytes = vec![0u8; name_len];
        reader.read_exact(&mut name_bytes)?;
        let name = String::from_utf8(name_bytes).map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid name UTF-8"))?;
        
        let value = Value::deserialize(reader)?;
        
        let mut weight_bytes = [0u8; 4];
        reader.read_exact(&mut weight_bytes)?;
        let weight = f32::from_le_bytes(weight_bytes);
        
        Ok(Self::new(id, name, value, weight))
    }
}