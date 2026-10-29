//! Aseprite user-data property maps (the 0x2020 payload).
//!
//! Every property has a name, a type word and a typed value. Unknown types
//! make the whole payload opaque, so callers keep the raw bytes instead.

#[derive(Clone, Debug, PartialEq)]
pub struct PropertyMap {
    pub key: u32,
    pub props: Vec<(String, PropertyValue)>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PropertyValue {
    Bool(bool),
    Int8(i8),
    Uint8(u8),
    Int16(i16),
    Uint16(u16),
    Int32(i32),
    Uint32(u32),
    Int64(i64),
    Uint64(u64),
    Fixed(f32),
    Float(f32),
    Double(f64),
    String(String),
    Point(i32, i32),
    Size(i32, i32),
    Rect(i32, i32, i32, i32),
    Vector(Vec<PropertyValue>),
    Map(Vec<(String, PropertyValue)>),
    Uuid([u8; 16]),
}

impl PropertyValue {
    pub fn as_string(&self) -> Option<&str> {
        match self {
            PropertyValue::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_i32(&self) -> Option<i32> {
        Some(match self {
            PropertyValue::Int8(v) => i32::from(*v),
            PropertyValue::Uint8(v) => i32::from(*v),
            PropertyValue::Int16(v) => i32::from(*v),
            PropertyValue::Uint16(v) => i32::from(*v),
            PropertyValue::Int32(v) => *v,
            PropertyValue::Uint32(v) => *v as i32,
            _ => return None,
        })
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            PropertyValue::Bool(v) => Some(*v),
            _ => None,
        }
    }
}

fn u16_at(b: &[u8], p: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(p..p + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], p: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(p..p + 4)?.try_into().ok()?))
}

fn u64_at(b: &[u8], p: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(p..p + 8)?.try_into().ok()?))
}

fn string_at(b: &[u8], p: &mut usize) -> Option<String> {
    let len = u16_at(b, *p)? as usize;
    *p += 2;
    let s = String::from_utf8_lossy(b.get(*p..*p + len)?).into_owned();
    *p += len;
    Some(s)
}

pub fn parse(bytes: &[u8]) -> Option<Vec<PropertyMap>> {
    if bytes.len() < 8 {
        return None;
    }
    let size = u32_at(bytes, 0)? as usize;
    if size < 8 || size > bytes.len() {
        return None;
    }
    let count = u32_at(bytes, 4)? as usize;
    let mut p = 8usize;
    let mut maps = Vec::with_capacity(count);
    for _ in 0..count {
        let key = u32_at(bytes, p)?;
        p += 4;
        let nprops = u32_at(bytes, p)? as usize;
        p += 4;
        let mut props = Vec::with_capacity(nprops);
        for _ in 0..nprops {
            let name = string_at(bytes, &mut p)?;
            let ty = u16_at(bytes, p)?;
            p += 2;
            props.push((name, value_at(bytes, &mut p, ty)?));
        }
        maps.push(PropertyMap { key, props });
    }
    Some(maps)
}

fn value_at(bytes: &[u8], p: &mut usize, ty: u16) -> Option<PropertyValue> {
    Some(match ty {
        0x0001 => {
            let v = *bytes.get(*p)?;
            *p += 1;
            PropertyValue::Bool(v != 0)
        }
        0x0002 => {
            let v = *bytes.get(*p)? as i8;
            *p += 1;
            PropertyValue::Int8(v)
        }
        0x0003 => {
            let v = *bytes.get(*p)?;
            *p += 1;
            PropertyValue::Uint8(v)
        }
        0x0004 => {
            let v = i16::from_le_bytes(bytes.get(*p..*p + 2)?.try_into().ok()?);
            *p += 2;
            PropertyValue::Int16(v)
        }
        0x0005 => {
            let v = u16_at(bytes, *p)?;
            *p += 2;
            PropertyValue::Uint16(v)
        }
        0x0006 => {
            let v = u32_at(bytes, *p)? as i32;
            *p += 4;
            PropertyValue::Int32(v)
        }
        0x0007 => {
            let v = u32_at(bytes, *p)?;
            *p += 4;
            PropertyValue::Uint32(v)
        }
        0x0008 => {
            let v = u64_at(bytes, *p)? as i64;
            *p += 8;
            PropertyValue::Int64(v)
        }
        0x0009 => {
            let v = u64_at(bytes, *p)?;
            *p += 8;
            PropertyValue::Uint64(v)
        }
        0x000A => {
            let v = u32_at(bytes, *p)? as f32 / 65536.0;
            *p += 4;
            PropertyValue::Fixed(v)
        }
        0x000B => {
            let v = f32::from_bits(u32_at(bytes, *p)?);
            *p += 4;
            PropertyValue::Float(v)
        }
        0x000C => {
            let v = f64::from_bits(u64_at(bytes, *p)?);
            *p += 8;
            PropertyValue::Double(v)
        }
        0x000D => PropertyValue::String(string_at(bytes, p)?),
        0x000E => {
            let x = u32_at(bytes, *p)? as i32;
            let y = u32_at(bytes, *p + 4)? as i32;
            *p += 8;
            PropertyValue::Point(x, y)
        }
        0x000F => {
            let w = u32_at(bytes, *p)? as i32;
            let h = u32_at(bytes, *p + 4)? as i32;
            *p += 8;
            PropertyValue::Size(w, h)
        }
        0x0010 => {
            let x = u32_at(bytes, *p)? as i32;
            let y = u32_at(bytes, *p + 4)? as i32;
            let w = u32_at(bytes, *p + 8)? as i32;
            let h = u32_at(bytes, *p + 12)? as i32;
            *p += 16;
            PropertyValue::Rect(x, y, w, h)
        }
        0x0011 => {
            let count = u32_at(bytes, *p)? as usize;
            let elem = u16_at(bytes, *p + 4)?;
            *p += 6;
            let mut out = Vec::with_capacity(count);
            for _ in 0..count {
                let ty = if elem == 0 {
                    let t = u16_at(bytes, *p)?;
                    *p += 2;
                    t
                } else {
                    elem
                };
                out.push(value_at(bytes, p, ty)?);
            }
            PropertyValue::Vector(out)
        }
        0x0012 => {
            let count = u32_at(bytes, *p)? as usize;
            *p += 4;
            let mut out = Vec::with_capacity(count);
            for _ in 0..count {
                let name = string_at(bytes, p)?;
                let ty = u16_at(bytes, *p)?;
                *p += 2;
                out.push((name, value_at(bytes, p, ty)?));
            }
            PropertyValue::Map(out)
        }
        0x0013 => {
            let u = bytes.get(*p..*p + 16)?;
            *p += 16;
            let mut uuid = [0u8; 16];
            uuid.copy_from_slice(u);
            PropertyValue::Uuid(uuid)
        }
        _ => return None,
    })
}

fn type_of(v: &PropertyValue) -> u16 {
    match v {
        PropertyValue::Bool(_) => 0x0001,
        PropertyValue::Int8(_) => 0x0002,
        PropertyValue::Uint8(_) => 0x0003,
        PropertyValue::Int16(_) => 0x0004,
        PropertyValue::Uint16(_) => 0x0005,
        PropertyValue::Int32(_) => 0x0006,
        PropertyValue::Uint32(_) => 0x0007,
        PropertyValue::Int64(_) => 0x0008,
        PropertyValue::Uint64(_) => 0x0009,
        PropertyValue::Fixed(_) => 0x000A,
        PropertyValue::Float(_) => 0x000B,
        PropertyValue::Double(_) => 0x000C,
        PropertyValue::String(_) => 0x000D,
        PropertyValue::Point(..) => 0x000E,
        PropertyValue::Size(..) => 0x000F,
        PropertyValue::Rect(..) => 0x0010,
        PropertyValue::Vector(_) => 0x0011,
        PropertyValue::Map(_) => 0x0012,
        PropertyValue::Uuid(_) => 0x0013,
    }
}

fn write_string(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u16).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

fn write_value(out: &mut Vec<u8>, v: &PropertyValue) {
    match v {
        PropertyValue::Bool(b) => out.push(u8::from(*b)),
        PropertyValue::Int8(v) => out.push(*v as u8),
        PropertyValue::Uint8(v) => out.push(*v),
        PropertyValue::Int16(v) => out.extend_from_slice(&v.to_le_bytes()),
        PropertyValue::Uint16(v) => out.extend_from_slice(&v.to_le_bytes()),
        PropertyValue::Int32(v) => out.extend_from_slice(&v.to_le_bytes()),
        PropertyValue::Uint32(v) => out.extend_from_slice(&v.to_le_bytes()),
        PropertyValue::Int64(v) => out.extend_from_slice(&v.to_le_bytes()),
        PropertyValue::Uint64(v) => out.extend_from_slice(&v.to_le_bytes()),
        PropertyValue::Fixed(v) => {
            out.extend_from_slice(&((v * 65536.0).round() as u32).to_le_bytes());
        }
        PropertyValue::Float(v) => out.extend_from_slice(&v.to_bits().to_le_bytes()),
        PropertyValue::Double(v) => out.extend_from_slice(&v.to_bits().to_le_bytes()),
        PropertyValue::String(s) => write_string(out, s),
        PropertyValue::Point(x, y) => {
            out.extend_from_slice(&x.to_le_bytes());
            out.extend_from_slice(&y.to_le_bytes());
        }
        PropertyValue::Size(w, h) => {
            out.extend_from_slice(&w.to_le_bytes());
            out.extend_from_slice(&h.to_le_bytes());
        }
        PropertyValue::Rect(x, y, w, h) => {
            out.extend_from_slice(&x.to_le_bytes());
            out.extend_from_slice(&y.to_le_bytes());
            out.extend_from_slice(&w.to_le_bytes());
            out.extend_from_slice(&h.to_le_bytes());
        }
        PropertyValue::Vector(items) => {
            out.extend_from_slice(&(items.len() as u32).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes()); // mixed element types
            for item in items {
                out.extend_from_slice(&type_of(item).to_le_bytes());
                write_value(out, item);
            }
        }
        PropertyValue::Map(props) => {
            out.extend_from_slice(&(props.len() as u32).to_le_bytes());
            for (name, value) in props {
                write_string(out, name);
                out.extend_from_slice(&type_of(value).to_le_bytes());
                write_value(out, value);
            }
        }
        PropertyValue::Uuid(u) => out.extend_from_slice(u),
    }
}

pub fn write(maps: &[PropertyMap]) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&(maps.len() as u32).to_le_bytes());
    for map in maps {
        body.extend_from_slice(&map.key.to_le_bytes());
        body.extend_from_slice(&(map.props.len() as u32).to_le_bytes());
        for (name, value) in &map.props {
            write_string(&mut body, name);
            body.extend_from_slice(&type_of(value).to_le_bytes());
            write_value(&mut body, value);
        }
    }
    let mut out = Vec::with_capacity(body.len() + 4);
    out.extend_from_slice(&((body.len() + 4) as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn property_maps_round_trip_every_type() {
        let maps = vec![PropertyMap {
            key: 0,
            props: vec![
                ("bool".into(), PropertyValue::Bool(true)),
                ("i8".into(), PropertyValue::Int8(-4)),
                ("u8".into(), PropertyValue::Uint8(250)),
                ("i16".into(), PropertyValue::Int16(-1000)),
                ("u16".into(), PropertyValue::Uint16(60000)),
                ("i32".into(), PropertyValue::Int32(-123456)),
                ("u32".into(), PropertyValue::Uint32(4_000_000_000)),
                ("i64".into(), PropertyValue::Int64(-9_000_000_000)),
                ("u64".into(), PropertyValue::Uint64(9_000_000_000)),
                ("fixed".into(), PropertyValue::Fixed(10.5)),
                ("float".into(), PropertyValue::Float(1.25)),
                ("double".into(), PropertyValue::Double(-2.5)),
                ("text".into(), PropertyValue::String("hi".into())),
                ("point".into(), PropertyValue::Point(1, 2)),
                ("size".into(), PropertyValue::Size(3, 4)),
                ("rect".into(), PropertyValue::Rect(1, 2, 3, 4)),
                (
                    "vector".into(),
                    PropertyValue::Vector(vec![
                        PropertyValue::Int32(1),
                        PropertyValue::String("two".into()),
                    ]),
                ),
                (
                    "map".into(),
                    PropertyValue::Map(vec![("nested".into(), PropertyValue::Bool(false))]),
                ),
                ("uuid".into(), PropertyValue::Uuid([7; 16])),
            ],
        }];
        let bytes = write(&maps);
        assert_eq!(parse(&bytes).unwrap(), maps);
    }
}
