//! Types de valeurs supportés par les gènes

use std::io::{Read, Write};

/// Type de valeur pour un gène
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Entier 64 bits
    Int(i64),
    /// Flottant 64 bits
    Float(f64),
    /// Chaîne de caractères
    String(String),
    /// Booléen
    Bool(bool),
    /// Bytes bruts
    Bytes(Vec<u8>),
    /// Null
    Null,
}

impl Value {
    /// Retourne le type discriminant
    pub fn type_id(&self) -> u8 {
        match self {
            Value::Int(_) => 0,
            Value::Float(_) => 1,
            Value::String(_) => 2,
            Value::Bool(_) => 3,
            Value::Bytes(_) => 4,
            Value::Null => 5,
        }
    }

    /// Taille approximative en bytes
    pub fn size_bytes(&self) -> usize {
        match self {
            Value::Int(_) => 8,
            Value::Float(_) => 8,
            Value::String(s) => s.len(),
            Value::Bool(_) => 1,
            Value::Bytes(b) => b.len(),
            Value::Null => 0,
        }
    }

    /// Sérialise la valeur dans un writer
    pub fn serialize<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writer.write_all(&[self.type_id()])?;
        match self {
            Value::Int(v) => writer.write_all(&v.to_le_bytes())?,
            Value::Float(v) => writer.write_all(&v.to_le_bytes())?,
            Value::String(s) => {
                let len = s.len() as u32;
                writer.write_all(&len.to_le_bytes())?;
                writer.write_all(s.as_bytes())?;
            }
            Value::Bool(v) => writer.write_all(&[*v as u8])?,
            Value::Bytes(b) => {
                let len = b.len() as u32;
                writer.write_all(&len.to_le_bytes())?;
                writer.write_all(b)?;
            }
            Value::Null => {}
        }
        Ok(())
    }

    /// Désérialise une valeur depuis un reader
    pub fn deserialize<R: Read>(reader: &mut R) -> std::io::Result<Self> {
        let mut type_byte = [0u8; 1];
        reader.read_exact(&mut type_byte)?;
        
        match type_byte[0] {
            0 => {
                let mut bytes = [0u8; 8];
                reader.read_exact(&mut bytes)?;
                Ok(Value::Int(i64::from_le_bytes(bytes)))
            }
            1 => {
                let mut bytes = [0u8; 8];
                reader.read_exact(&mut bytes)?;
                Ok(Value::Float(f64::from_le_bytes(bytes)))
            }
            2 => {
                let mut len_bytes = [0u8; 4];
                reader.read_exact(&mut len_bytes)?;
                let len = u32::from_le_bytes(len_bytes) as usize;
                let mut string_bytes = vec![0u8; len];
                reader.read_exact(&mut string_bytes)?;
                Ok(Value::String(String::from_utf8(string_bytes).map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid UTF-8"))?))
            }
            3 => {
                let mut byte = [0u8; 1];
                reader.read_exact(&mut byte)?;
                Ok(Value::Bool(byte[0] != 0))
            }
            4 => {
                let mut len_bytes = [0u8; 4];
                reader.read_exact(&mut len_bytes)?;
                let len = u32::from_le_bytes(len_bytes) as usize;
                let mut bytes = vec![0u8; len];
                reader.read_exact(&mut bytes)?;
                Ok(Value::Bytes(bytes))
            }
            5 => Ok(Value::Null),
            _ => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Unknown value type")),
        }
    }
}