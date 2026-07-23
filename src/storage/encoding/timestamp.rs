use crate::exec::batch::ColumnData;
use crate::storage::encoding::{Decoder, Encoder, int64};

/// Wraps Int64 delta+varint encoding but returns Timestamp type on decode.
pub struct TimestampEncoder;

impl Encoder for TimestampEncoder {
    fn encode(&self, data: &ColumnData) -> Vec<u8> {
        match data {
            ColumnData::Timestamp(values) => {
                let inner = ColumnData::Int64(values.clone());
                let enc = int64::DeltaBitpackEncoder;
                enc.encode(&inner)
            }
            _ => panic!("TimestampEncoder only supports Timestamp"),
        }
    }

    fn encoding_id(&self) -> u16 {
        4
    }
}

pub struct TimestampDecoder;

impl Decoder for TimestampDecoder {
    fn decode(&self, bytes: &[u8], count: usize) -> ColumnData {
        let dec = int64::DeltaBitpackDecoder;
        match dec.decode(bytes, count) {
            ColumnData::Int64(v) => ColumnData::Timestamp(v),
            _ => panic!("TimestampDecoder expected Int64 from inner decoder"),
        }
    }

    fn decoding_id(&self) -> u16 {
        4
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_timestamp() {
        let data = ColumnData::Timestamp(vec![1700000000, 1700003600, 1700007200]);
        let enc = TimestampEncoder;
        let bytes = enc.encode(&data);
        let dec = TimestampDecoder;
        let result = dec.decode(&bytes, 3);
        match result {
            ColumnData::Timestamp(v) => {
                assert_eq!(v, vec![1700000000, 1700003600, 1700007200]);
            }
            _ => panic!("wrong type"),
        }
    }
}
