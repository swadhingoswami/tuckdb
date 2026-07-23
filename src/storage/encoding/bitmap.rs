use crate::exec::batch::ColumnData;
use crate::storage::encoding::{Decoder, Encoder};

pub struct BitmapEncoder;

impl Encoder for BitmapEncoder {
    fn encode(&self, data: &ColumnData) -> Vec<u8> {
        match data {
            ColumnData::Int64(values) => {
                let count = values.len() as u64;
                let mut buf = Vec::new();
                encode_varint(count, &mut buf);
                for chunk in values.chunks(64) {
                    let mut word = 0u64;
                    for (i, &v) in chunk.iter().enumerate() {
                        if v != 0 {
                            word |= 1u64 << i;
                        }
                    }
                    buf.extend_from_slice(&word.to_le_bytes());
                }
                buf
            }
            _ => panic!("BitmapEncoder only supports Int64"),
        }
    }

    fn encoding_id(&self) -> u16 {
        6
    }
}

pub struct BitmapDecoder;

impl Decoder for BitmapDecoder {
    fn decode(&self, bytes: &[u8], count: usize) -> ColumnData {
        if count == 0 {
            return ColumnData::Int64(Vec::new());
        }
        let (stored_count, mut pos) = decode_varint(bytes);
        assert_eq!(stored_count, count as u64, "BitmapDecoder: count mismatch");
        let word_count = count.div_ceil(64);
        let mut values = Vec::with_capacity(count);
        for wi in 0..word_count {
            let mut word_bytes = [0u8; 8];
            word_bytes.copy_from_slice(&bytes[pos..pos + 8]);
            pos += 8;
            let word = u64::from_le_bytes(word_bytes);
            let bits_in_word = if wi == word_count - 1 && !count.is_multiple_of(64) {
                count % 64
            } else {
                64
            };
            for i in 0..bits_in_word {
                if word & (1u64 << i) != 0 {
                    values.push(1);
                } else {
                    values.push(0);
                }
            }
        }
        ColumnData::Int64(values)
    }

    fn decoding_id(&self) -> u16 {
        6
    }
}

fn encode_varint(mut val: u64, buf: &mut Vec<u8>) {
    loop {
        if val < 0x80 {
            buf.push(val as u8);
            break;
        }
        buf.push((val as u8) | 0x80);
        val >>= 7;
    }
}

fn decode_varint(buf: &[u8]) -> (u64, usize) {
    let mut val = 0u64;
    let mut shift = 0;
    let mut pos = 0;
    loop {
        let byte = buf[pos];
        pos += 1;
        val |= ((byte & 0x7F) as u64) << shift;
        shift += 7;
        if byte & 0x80 == 0 {
            break;
        }
    }
    (val, pos)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_bitmap() {
        let data = ColumnData::Int64(vec![1, 0, 1, 1, 0, 0, 1, 0]);
        let enc = BitmapEncoder;
        let bytes = enc.encode(&data);
        let dec = BitmapDecoder;
        let result = dec.decode(&bytes, 8);
        match result {
            ColumnData::Int64(v) => assert_eq!(v, vec![1, 0, 1, 1, 0, 0, 1, 0]),
            _ => panic!("wrong type"),
        }
    }

    #[test]
    fn all_zeros() {
        let data = ColumnData::Int64(vec![0, 0, 0]);
        let enc = BitmapEncoder;
        let bytes = enc.encode(&data);
        let dec = BitmapDecoder;
        let result = dec.decode(&bytes, 3);
        match result {
            ColumnData::Int64(v) => assert_eq!(v, vec![0, 0, 0]),
            _ => panic!("wrong type"),
        }
    }

    #[test]
    fn all_ones() {
        let data = ColumnData::Int64(vec![1; 100]);
        let enc = BitmapEncoder;
        let bytes = enc.encode(&data);
        let dec = BitmapDecoder;
        let result = dec.decode(&bytes, 100);
        match result {
            ColumnData::Int64(v) => assert_eq!(v, vec![1; 100]),
            _ => panic!("wrong type"),
        }
    }

    #[test]
    fn empty_bitmap() {
        let data = ColumnData::Int64(vec![]);
        let enc = BitmapEncoder;
        let bytes = enc.encode(&data);
        let dec = BitmapDecoder;
        let result = dec.decode(&bytes, 0);
        match result {
            ColumnData::Int64(v) => assert!(v.is_empty()),
            _ => panic!("wrong type"),
        }
    }
}
