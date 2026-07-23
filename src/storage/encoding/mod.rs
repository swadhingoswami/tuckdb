pub mod int64;
pub mod float64;
pub mod utf8;
pub mod timestamp;
pub mod rle;
pub mod bitmap;

use crate::exec::batch::ColumnData;
use crate::storage::chunk::ColumnStats;

pub trait Encoder {
    fn encode(&self, data: &ColumnData) -> Vec<u8>;
    fn encoding_id(&self) -> u16;
}

pub trait Decoder {
    fn decode(&self, bytes: &[u8], count: usize) -> ColumnData;
    fn decoding_id(&self) -> u16;
}

pub fn column_stats(data: &ColumnData) -> ColumnStats {
    match data {
        ColumnData::Int64(v) => {
            let min = v.iter().min().copied();
            let max = v.iter().max().copied();
            let null_count = 0;
            ColumnStats::new(
                min.map(|m| m as f64),
                max.map(|m| m as f64),
                null_count,
            )
        }
        ColumnData::Float64(v) => {
            let min = v.iter().min_by(|a, b| a.partial_cmp(b).unwrap()).copied();
            let max = v.iter().max_by(|a, b| a.partial_cmp(b).unwrap()).copied();
            let null_count = 0;
            ColumnStats::new(min, max, null_count)
        }
        ColumnData::Utf8(_v) => {
            let null_count = 0;
            ColumnStats::new(None, None, null_count)
        }
        ColumnData::Timestamp(v) => {
            let min = v.iter().min().copied();
            let max = v.iter().max().copied();
            ColumnStats::new(
                min.map(|m| m as f64),
                max.map(|m| m as f64),
                0,
            )
        }
    }
}

pub fn encode_column(data: &ColumnData) -> (u16, Vec<u8>) {
    match data {
        ColumnData::Int64(v) => {
            if v.iter().all(|&x| x == 0 || x == 1) {
                let enc = bitmap::BitmapEncoder;
                (enc.encoding_id(), enc.encode(data))
            } else {
                let enc = int64::DeltaBitpackEncoder;
                (enc.encoding_id(), enc.encode(data))
            }
        }
        ColumnData::Float64(_) => {
            let enc = float64::XorEncoder;
            (enc.encoding_id(), enc.encode(data))
        }
        ColumnData::Utf8(_) => {
            let enc = utf8::DictZstdEncoder;
            (enc.encoding_id(), enc.encode(data))
        }
        ColumnData::Timestamp(_) => {
            let enc = timestamp::TimestampEncoder;
            (enc.encoding_id(), enc.encode(data))
        }
    }
}

pub fn decode_column(encoding_id: u16, bytes: &[u8], count: usize) -> ColumnData {
    match encoding_id {
        1 => {
            let dec = int64::DeltaBitpackDecoder;
            dec.decode(bytes, count)
        }
        2 => {
            let dec = float64::XorDecoder;
            dec.decode(bytes, count)
        }
        3 => {
            let dec = utf8::DictZstdDecoder;
            dec.decode(bytes, count)
        }
        4 => {
            let dec = timestamp::TimestampDecoder;
            dec.decode(bytes, count)
        }
        5 => {
            let dec = rle::RleDecoder;
            dec.decode(bytes, count)
        }
        6 => {
            let dec = bitmap::BitmapDecoder;
            dec.decode(bytes, count)
        }
        _ => panic!("unknown encoding id: {}", encoding_id),
    }
}
