//! `UniformValue`.

use serde_json::Value;

/// One member of a uniform block.
#[derive(Clone, Debug, PartialEq)]
pub enum UniformValue {
    Int(i32),
    IVec3([i32; 3]),
    Float(f32),
    Vec2([f32; 2]),
    Vec3([f32; 3]),
    Vec4([f32; 4]),
    /// Column-major, as std140 stores it.
    Mat4([f32; 16]),
}

impl UniformValue {
    /// `UniformValue.CODEC`: `{"type": ..., "value": ...}`.
    pub fn parse(json: &Value) -> Result<Self, String> {
        let kind = json["type"].as_str().ok_or("uniform without a type")?;
        let value = &json["value"];
        let floats = |count: usize| -> Result<Vec<f32>, String> {
            let list = value
                .as_array()
                .filter(|l| l.len() == count)
                .ok_or_else(|| format!("{kind} needs {count} numbers"))?;
            list.iter()
                .map(|n| {
                    n.as_f64()
                        .map(|n| n as f32)
                        .ok_or_else(|| format!("{kind}: not a number"))
                })
                .collect()
        };
        Ok(match kind {
            "int" => Self::Int(int(value).ok_or("int needs a whole number")?),
            "float" => Self::Float(value.as_f64().ok_or("float needs a number")? as f32),
            "ivec3" => {
                let list = value
                    .as_array()
                    .filter(|l| l.len() == 3)
                    .ok_or("ivec3 needs 3 numbers")?;
                let mut out = [0; 3];
                for (slot, n) in out.iter_mut().zip(list) {
                    *slot = int(n).ok_or("ivec3 needs whole numbers")?;
                }
                Self::IVec3(out)
            }
            "vec2" => Self::Vec2(floats(2)?.try_into().expect("two")),
            "vec3" => Self::Vec3(floats(3)?.try_into().expect("three")),
            "vec4" => Self::Vec4(floats(4)?.try_into().expect("four")),
            "matrix4x4" => {
                let rows: [f32; 16] = floats(16)?.try_into().expect("sixteen");
                let mut columns = [0.0; 16];
                for (i, v) in rows.iter().enumerate() {
                    columns[(i & 3) * 4 + (i >> 2)] = *v;
                }
                Self::Mat4(columns)
            }
            other => return Err(format!("unknown uniform type {other:?}")),
        })
    }

    /// Alignment and size in a std140 block.
    fn layout(&self) -> (usize, usize) {
        match self {
            Self::Int(_) | Self::Float(_) => (4, 4),
            Self::Vec2(_) => (8, 8),
            Self::Vec3(_) => (16, 12),
            Self::IVec3(_) => (16, 12),
            Self::Vec4(_) => (16, 16),
            Self::Mat4(_) => (16, 64),
        }
    }

    fn words(&self) -> Vec<u32> {
        match self {
            Self::Int(v) => vec![*v as u32],
            Self::IVec3(v) => v.iter().map(|&n| n as u32).collect(),
            Self::Float(v) => vec![v.to_bits()],
            Self::Vec2(v) => v.iter().map(|n| n.to_bits()).collect(),
            Self::Vec3(v) => v.iter().map(|n| n.to_bits()).collect(),
            Self::Vec4(v) => v.iter().map(|n| n.to_bits()).collect(),
            Self::Mat4(v) => v.iter().map(|n| n.to_bits()).collect(),
        }
    }
}

fn int(value: &Value) -> Option<i32> {
    value
        .as_i64()
        .and_then(|n| i32::try_from(n).ok())
        .or_else(|| value.as_u64().map(|n| n as u32 as i32))
}

/// A std140 block built member by member (`Std140Builder`).
#[derive(Default)]
pub struct Std140 {
    bytes: Vec<u8>,
}

impl Std140 {
    pub fn push(&mut self, value: &UniformValue) -> &mut Self {
        let (align, _) = value.layout();
        self.align(align);
        for word in value.words() {
            self.bytes.extend(word.to_le_bytes());
        }
        self
    }

    pub fn vec2(&mut self, x: f32, y: f32) -> &mut Self {
        self.push(&UniformValue::Vec2([x, y]))
    }

    fn align(&mut self, align: usize) {
        while !self.bytes.len().is_multiple_of(align) {
            self.bytes.push(0);
        }
    }

    /// The block's bytes, a multiple of 16 long (never empty).
    pub fn finish(mut self) -> Vec<u8> {
        self.align(16);
        if self.bytes.is_empty() {
            self.bytes.resize(16, 0);
        }
        self.bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse(json: Value) -> UniformValue {
        UniformValue::parse(&json).unwrap()
    }

    #[test]
    fn every_type_parses() {
        assert_eq!(
            parse(json!({"type": "int", "value": 3})),
            UniformValue::Int(3)
        );
        assert_eq!(
            parse(json!({"type": "vec2", "name": "BlurDir", "value": [1.0, 0]})),
            UniformValue::Vec2([1.0, 0.0])
        );
        assert_eq!(
            parse(json!({"type": "ivec3", "value": [1, -2, 3]})),
            UniformValue::IVec3([1, -2, 3])
        );
        assert!(UniformValue::parse(&json!({"type": "vec3", "value": [1, 2]})).is_err());
        assert!(UniformValue::parse(&json!({"type": "quat", "value": 1})).is_err());
        assert!(UniformValue::parse(&json!({"value": 1})).is_err());
    }

    #[test]
    fn a_matrix_is_listed_by_rows_and_stored_by_columns() {
        let rows: Vec<f32> = (0..16).map(|n| n as f32).collect();
        let UniformValue::Mat4(columns) = parse(json!({"type": "matrix4x4", "value": rows})) else {
            panic!("not a matrix");
        };
        assert_eq!(columns[4], 1.0);
        assert_eq!(columns[1], 4.0);
        assert_eq!(columns[15], 15.0);
    }

    #[test]
    fn members_are_aligned_as_std140_has_them() {
        let mut block = Std140::default();
        block
            .push(&UniformValue::Float(1.0))
            .push(&UniformValue::Vec2([2.0, 3.0]))
            .push(&UniformValue::Vec4([4.0; 4]));
        let bytes = block.finish();
        assert_eq!(bytes.len(), 32);
        assert_eq!(&bytes[8..12], &2.0f32.to_le_bytes());
        assert_eq!(&bytes[16..20], &4.0f32.to_le_bytes());
        let mut block = Std140::default();
        block
            .push(&UniformValue::Vec3([1.0; 3]))
            .push(&UniformValue::Float(9.0));
        assert_eq!(&block.finish()[12..16], &9.0f32.to_le_bytes());
        assert_eq!(Std140::default().finish().len(), 16);
    }
}
