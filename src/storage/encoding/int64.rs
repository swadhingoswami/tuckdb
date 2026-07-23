use crate::exec::batch::ColumnData;
use crate::storage::encoding::{Decoder, Encoder};

/// Delta + variable-length integer encoding.
/// Stores the first value as raw i64, then deltas as varints.
pub struct DeltaBitpackEncoder;

impl Encoder for DeltaBitpackEncoder {
    fn encode(&self, data: &ColumnData) -> Vec<u8> {
        match data {
            ColumnData::Int64(values) => {
                if values.is_empty() {
                    return Vec::new();
                }
                let mut buf = Vec::new();
                // store base as raw i64 (little-endian)
                let base = values[0];
                buf.extend_from_slice(&base.to_le_bytes());
                // store deltas as varints
                let mut prev = base;
                for &v in &values[1..] {
                    let delta = v.wrapping_sub(prev);
                    encode_varint_signed(delta, &mut buf);
                    prev = v;
                }
                buf
            }
            _ => panic!("DeltaBitpackEncoder only supports Int64"),
        }
    }

    fn encoding_id(&self) -> u16 {
        1
    }
}

pub struct DeltaBitpackDecoder;

impl Decoder for DeltaBitpackDecoder {
    fn decode(&self, bytes: &[u8], count: usize) -> ColumnData {
        if count == 0 {
            return ColumnData::Int64(Vec::new());
        }
        let mut pos = 0;
        // read base
        let base = i64::from_le_bytes(bytes[pos..pos + 8].try_into().unwrap());
        pos += 8;
        let mut values = Vec::with_capacity(count);
        values.push(base);
        let mut prev = base;
        while values.len() < count {
            let (delta, n) = decode_varint_signed(&bytes[pos..]);
            pos += n;
            let v = prev.wrapping_add(delta);
            values.push(v);
            prev = v;
        }
        ColumnData::Int64(values)
    }

    fn decoding_id(&self) -> u16 {
        1
    }
}

fn encode_varint_signed(val: i64, buf: &mut Vec<u8>) {
    // Zigzag encode
    let mut v = ((val << 1) ^ (val >> 63)) as u64;
    loop {
        if v < 0x80 {
            buf.push(v as u8);
            break;
        }
        buf.push((v as u8) | 0x80);
        v >>= 7;
    }
}

fn decode_varint_signed(buf: &[u8]) -> (i64, usize) {
    let mut v = 0u64;
    let mut shift = 0;
    let mut pos = 0;
    loop {
        let byte = buf[pos];
        pos += 1;
        v |= ((byte & 0x7F) as u64) << shift;
        shift += 7;
        if byte & 0x80 == 0 {
            break;
        }
    }
    // Zigzag decode
    let val = ((v >> 1) as i64) ^ (-((v & 1) as i64));
    (val, pos)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_int64() {
        let data = ColumnData::Int64(vec![100, 105, 110, 200, -5, 0, i64::MAX]);
        let enc = DeltaBitpackEncoder;
        let bytes = enc.encode(&data);
        let dec = DeltaBitpackDecoder;
        let result = dec.decode(&bytes, 7);
        match result {
            ColumnData::Int64(v) => assert_eq!(v, vec![100, 105, 110, 200, -5, 0, i64::MAX]),
            _ => panic!("wrong type"),
        }
    }

    #[test]
    fn empty_int64() {
        let data = ColumnData::Int64(vec![]);
        let enc = DeltaBitpackEncoder;
        let bytes = enc.encode(&data);
        let dec = DeltaBitpackDecoder;
        let result = dec.decode(&bytes, 0);
        match result {
            ColumnData::Int64(v) => assert!(v.is_empty()),
            _ => panic!("wrong type"),
        }
    }
}
