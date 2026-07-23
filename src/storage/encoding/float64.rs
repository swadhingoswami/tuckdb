use crate::exec::batch::ColumnData;
use crate::storage::encoding::{Decoder, Encoder};

/// Simplified XOR encoding (Gorilla-inspired).
/// Stores first value as raw f64. For each subsequent value,
/// XOR with previous and store leading zeros, trailing zeros, and meaningful bits.
pub struct XorEncoder;

impl Encoder for XorEncoder {
    fn encode(&self, data: &ColumnData) -> Vec<u8> {
        match data {
            ColumnData::Float64(values) => {
                if values.is_empty() {
                    return Vec::new();
                }
                let mut buf = Vec::new();
                let first = values[0].to_bits();
                buf.extend_from_slice(&first.to_le_bytes());
                let mut prev = first;
                for &v in &values[1..] {
                    let bits = v.to_bits();
                    let xor = prev ^ bits;
                    if xor == 0 {
                        // same value: store a single zero bit
                        buf.push(0x00);
                    } else {
                        let leading = xor.leading_zeros() as u16;
                        let trailing = xor.trailing_zeros() as u16;
                        let meaningful = 64 - leading - trailing;
                        // store control byte: 0x01 means full info follows
                        buf.push(0x01);
                        buf.extend_from_slice(&leading.to_le_bytes());
                        buf.extend_from_slice(&meaningful.to_le_bytes());
                        // store the meaningful bits packed
                        let mask = if meaningful == 64 {
                            xor
                        } else {
                            (xor >> trailing) & ((1u64 << meaningful) - 1)
                        };
                        let byte_count = meaningful.div_ceil(8) as usize;
                        buf.extend_from_slice(&mask.to_le_bytes()[..byte_count]);
                    }
                    prev = bits;
                }
                buf
            }
            _ => panic!("XorEncoder only supports Float64"),
        }
    }

    fn encoding_id(&self) -> u16 {
        2
    }
}

pub struct XorDecoder;

impl Decoder for XorDecoder {
    fn decode(&self, bytes: &[u8], count: usize) -> ColumnData {
        if count == 0 {
            return ColumnData::Float64(Vec::new());
        }
        let mut pos = 0;
        let first = u64::from_le_bytes(bytes[pos..pos + 8].try_into().unwrap());
        pos += 8;
        let mut values = Vec::with_capacity(count);
        values.push(f64::from_bits(first));
        let mut prev = first;
        while values.len() < count {
            if pos >= bytes.len() {
                break;
            }
            let control = bytes[pos];
            pos += 1;
            if control == 0x00 {
                // same as previous
                values.push(f64::from_bits(prev));
                continue;
            }
            // read leading and meaningful bits
            let leading = u16::from_le_bytes(bytes[pos..pos + 2].try_into().unwrap());
            pos += 2;
            let meaningful = u16::from_le_bytes(bytes[pos..pos + 2].try_into().unwrap());
            pos += 2;
            let byte_count = meaningful.div_ceil(8) as usize;
            let mut mask = 0u64;
            for i in 0..byte_count {
                mask |= (bytes[pos + i] as u64) << (i * 8);
            }
            pos += byte_count;
            let trailing = 64 - leading as u32 - meaningful as u32;
            let xor = mask << trailing;
            let bits = prev ^ xor;
            values.push(f64::from_bits(bits));
            prev = bits;
        }
        ColumnData::Float64(values)
    }

    fn decoding_id(&self) -> u16 {
        2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_float64() {
        let data = ColumnData::Float64(vec![1.0, 2.5, 3.14, 0.0, -1.0, 1e10, 1.0, 1.0]);
        let enc = XorEncoder;
        let bytes = enc.encode(&data);
        let dec = XorDecoder;
        let result = dec.decode(&bytes, 8);
        match result {
            ColumnData::Float64(v) => {
                assert_eq!(v.len(), 8);
                for (a, b) in v
                    .iter()
                    .zip(vec![1.0, 2.5, 3.14, 0.0, -1.0, 1e10, 1.0, 1.0].iter())
                {
                    assert!((a - b).abs() < 1e-10);
                }
            }
            _ => panic!("wrong type"),
        }
    }
}
