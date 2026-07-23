use crate::exec::batch::ColumnData;
use crate::storage::encoding::{Decoder, Encoder};

/// Dictionary + ZSTD encoding for Utf8 strings.
pub struct DictZstdEncoder;

impl Encoder for DictZstdEncoder {
    fn encode(&self, data: &ColumnData) -> Vec<u8> {
        match data {
            ColumnData::Utf8(values) => {
                if values.is_empty() {
                    return Vec::new();
                }
                // build dictionary
                let mut dict: Vec<&str> = Vec::new();
                let mut dict_map: std::collections::HashMap<&str, u32> =
                    std::collections::HashMap::new();
                for v in values {
                    if !dict_map.contains_key(v.as_str()) {
                        let idx = dict.len() as u32;
                        dict.push(v);
                        dict_map.insert(v, idx);
                    }
                }
                // serialize: dictionary count, dict entries, indices
                let mut buf = Vec::new();
                // dict count as varint
                encode_varint(dict.len() as u64, &mut buf);
                for s in &dict {
                    encode_varint(s.len() as u64, &mut buf);
                    buf.extend_from_slice(s.as_bytes());
                }
                // indices as varints
                for v in values {
                    let idx = dict_map[v.as_str()];
                    encode_varint(idx as u64, &mut buf);
                }
                // compress with zstd
                zstd::encode_all(buf.as_slice(), 3).unwrap()
            }
            _ => panic!("DictZstdEncoder only supports Utf8"),
        }
    }

    fn encoding_id(&self) -> u16 {
        3
    }
}

pub struct DictZstdDecoder;

impl Decoder for DictZstdDecoder {
    fn decode(&self, bytes: &[u8], count: usize) -> ColumnData {
        if count == 0 {
            return ColumnData::Utf8(Vec::new());
        }
        // decompress with zstd
        let decompressed = zstd::decode_all(bytes).unwrap();
        let mut pos = 0;
        // read dictionary count
        let dict_len = decode_varint(&decompressed[pos..]);
        pos += varint_len(&decompressed[pos..]);
        let mut dict: Vec<String> = Vec::with_capacity(dict_len as usize);
        for _ in 0..dict_len {
            let s_len = decode_varint(&decompressed[pos..]);
            pos += varint_len(&decompressed[pos..]);
            let s = String::from_utf8(decompressed[pos..pos + s_len as usize].to_vec()).unwrap();
            pos += s_len as usize;
            dict.push(s);
        }
        // read indices
        let mut values = Vec::with_capacity(count);
        for _ in 0..count {
            let idx = decode_varint(&decompressed[pos..]);
            pos += varint_len(&decompressed[pos..]);
            values.push(dict[idx as usize].clone());
        }
        ColumnData::Utf8(values)
    }

    fn decoding_id(&self) -> u16 {
        3
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

fn decode_varint(buf: &[u8]) -> u64 {
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
    val
}

fn varint_len(buf: &[u8]) -> usize {
    let mut len = 0;
    loop {
        if buf[len] & 0x80 == 0 {
            return len + 1;
        }
        len += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_utf8() {
        let data = ColumnData::Utf8(vec![
            "hello".to_string(),
            "world".to_string(),
            "hello".to_string(),
            "rust".to_string(),
            "world".to_string(),
            "tuckdb".to_string(),
        ]);
        let enc = DictZstdEncoder;
        let bytes = enc.encode(&data);
        let dec = DictZstdDecoder;
        let result = dec.decode(&bytes, 6);
        match result {
            ColumnData::Utf8(v) => {
                assert_eq!(
                    v,
                    vec!["hello", "world", "hello", "rust", "world", "tuckdb"]
                );
            }
            _ => panic!("wrong type"),
        }
    }
}
