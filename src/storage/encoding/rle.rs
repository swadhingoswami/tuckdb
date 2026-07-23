use crate::exec::batch::ColumnData;
use crate::storage::encoding::{Decoder, Encoder};

pub struct RleEncoder;

impl Encoder for RleEncoder {
    fn encode(&self, data: &ColumnData) -> Vec<u8> {
        match data {
            ColumnData::Int64(values) => {
                if values.is_empty() {
                    return Vec::new();
                }
                let mut buf = Vec::new();
                let mut i = 0;
                while i < values.len() {
                    let v = values[i];
                    let mut run_len: u64 = 1;
                    while i + (run_len as usize) < values.len()
                        && values[i + (run_len as usize)] == v
                    {
                        run_len += 1;
                    }
                    encode_varint_signed(v, &mut buf);
                    encode_varint(run_len, &mut buf);
                    i += run_len as usize;
                }
                buf
            }
            _ => panic!("RleEncoder only supports Int64"),
        }
    }

    fn encoding_id(&self) -> u16 {
        5
    }
}

pub struct RleDecoder;

impl Decoder for RleDecoder {
    fn decode(&self, bytes: &[u8], count: usize) -> ColumnData {
        if count == 0 {
            return ColumnData::Int64(Vec::new());
        }
        let mut pos = 0;
        let mut values = Vec::with_capacity(count);
        while values.len() < count {
            let (v, n1) = decode_varint_signed(&bytes[pos..]);
            pos += n1;
            let (run_len, n2) = decode_varint(&bytes[pos..]);
            pos += n2;
            for _ in 0..run_len {
                values.push(v);
            }
        }
        ColumnData::Int64(values)
    }

    fn decoding_id(&self) -> u16 {
        5
    }
}

fn encode_varint_signed(val: i64, buf: &mut Vec<u8>) {
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
    let (v, n) = decode_varint(buf);
    let val = ((v >> 1) as i64) ^ (-((v & 1) as i64));
    (val, n)
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
    fn roundtrip_rle() {
        let data = ColumnData::Int64(vec![5, 5, 5, 3, 3, 7]);
        let enc = RleEncoder;
        let bytes = enc.encode(&data);
        let dec = RleDecoder;
        let result = dec.decode(&bytes, 6);
        match result {
            ColumnData::Int64(v) => assert_eq!(v, vec![5, 5, 5, 3, 3, 7]),
            _ => panic!("wrong type"),
        }
    }

    #[test]
    fn no_runs() {
        let data = ColumnData::Int64(vec![1, 2, 3, 4]);
        let enc = RleEncoder;
        let bytes = enc.encode(&data);
        let dec = RleDecoder;
        let result = dec.decode(&bytes, 4);
        match result {
            ColumnData::Int64(v) => assert_eq!(v, vec![1, 2, 3, 4]),
            _ => panic!("wrong type"),
        }
    }

    #[test]
    fn empty_rle() {
        let data = ColumnData::Int64(vec![]);
        let enc = RleEncoder;
        let bytes = enc.encode(&data);
        let dec = RleDecoder;
        let result = dec.decode(&bytes, 0);
        match result {
            ColumnData::Int64(v) => assert!(v.is_empty()),
            _ => panic!("wrong type"),
        }
    }
}
