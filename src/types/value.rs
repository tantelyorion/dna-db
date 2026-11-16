//! Types de valeurs supportés par les gènes.

use crate::util::{invalid_data, read_len_bytes, read_string, read_u8, write_len_bytes};
use std::io::{self, Read, Write};

/// Valeur portée par un gène.
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
    /// Octets bruts
    Bytes(Vec<u8>),
    /// Valeur nulle
    Null,
}

impl Value {
    /// Discriminant utilisé dans le format binaire.
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

    /// Nom lisible du type.
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::String(_) => "string",
            Value::Bool(_) => "bool",
            Value::Bytes(_) => "bytes",
            Value::Null => "null",
        }
    }

    /// Taille exacte en octets de la valeur une fois sérialisée (type inclus).
    pub fn size_bytes(&self) -> usize {
        1 + match self {
            Value::Int(_) | Value::Float(_) => 8,
            Value::String(s) => 4 + s.len(),
            Value::Bool(_) => 1,
            Value::Bytes(b) => 4 + b.len(),
            Value::Null => 0,
        }
    }

    /// Sérialise la valeur dans un writer.
    pub fn serialize<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        writer.write_all(&[self.type_id()])?;
        match self {
            Value::Int(v) => writer.write_all(&v.to_le_bytes()),
            Value::Float(v) => writer.write_all(&v.to_le_bytes()),
            Value::String(s) => write_len_bytes(writer, s.as_bytes()),
            Value::Bool(v) => writer.write_all(&[*v as u8]),
            Value::Bytes(b) => write_len_bytes(writer, b),
            Value::Null => Ok(()),
        }
    }

    /// Désérialise une valeur depuis un reader.
    pub fn deserialize<R: Read>(reader: &mut R) -> io::Result<Self> {
        match read_u8(reader)? {
            0 => {
                let mut b = [0u8; 8];
                reader.read_exact(&mut b)?;
                Ok(Value::Int(i64::from_le_bytes(b)))
            }
            1 => {
                let mut b = [0u8; 8];
                reader.read_exact(&mut b)?;
                Ok(Value::Float(f64::from_le_bytes(b)))
            }
            2 => Ok(Value::String(read_string(reader)?)),
            3 => Ok(Value::Bool(read_u8(reader)? != 0)),
            4 => Ok(Value::Bytes(read_len_bytes(reader)?)),
            5 => Ok(Value::Null),
            _ => Err(invalid_data("Type de valeur inconnu")),
        }
    }
}

impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Value::Int(v)
    }
}

impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Value::Float(v)
    }
}

impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Value::Bool(v)
    }
}

impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::String(v.to_string())
    }
}

impl From<String> for Value {
    fn from(v: String) -> Self {
        Value::String(v)
    }
}

impl From<Vec<u8>> for Value {
    fn from(v: Vec<u8>) -> Self {
        Value::Bytes(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(v: Value) {
        let mut buf = Vec::new();
        v.serialize(&mut buf).unwrap();
        assert_eq!(buf.len(), v.size_bytes());
        let back = Value::deserialize(&mut buf.as_slice()).unwrap();
        assert_eq!(v, back);
    }

    #[test]
    fn all_variants_roundtrip() {
        roundtrip(Value::Int(-42));
        roundtrip(Value::Float(3.5));
        roundtrip(Value::String("héllo".to_string()));
        roundtrip(Value::Bool(true));
        roundtrip(Value::Bytes(vec![1, 2, 3]));
        roundtrip(Value::Null);
    }

    #[test]
    fn unknown_type_is_rejected() {
        let data = [99u8];
        assert!(Value::deserialize(&mut &data[..]).is_err());
    }
}
