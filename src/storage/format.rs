use std::collections::BTreeMap;

use crate::exec::batch::{Column, ColumnData, RecordBatch};
use crate::schema::{DataType, Field, Schema};
use crate::storage::chunk::ChunkMeta;
use crate::storage::encoding;

pub const MAGIC: &[u8; 4] = b"TCKB";
pub const VERSION: u32 = 1;

pub struct BlobWriter;

impl BlobWriter {
    pub fn write(batches: &[RecordBatch]) -> Vec<u8> {
        if batches.is_empty() {
            return Vec::new();
        }
        let schema = &batches[0].schema;
        let num_cols = schema.fields.len();

        let schema_bytes = serialize_schema(schema);
        let header_start = 4 + 4;

        let mut all_encoded: Vec<(ChunkMeta, Vec<u8>)> = Vec::new();
        let mut chunk_counts: Vec<u32> = vec![0u32; num_cols];

        for batch in batches {
            for (col_idx, column) in batch.columns.iter().enumerate() {
                let (encoding_id, compressed) = encoding::encode_column(&column.data);
                let stats = encoding::column_stats(&column.data);
                all_encoded.push((
                    ChunkMeta {
                        col_idx: col_idx as u32,
                        chunk_idx: chunk_counts[col_idx],
                        encoding: encoding_id,
                        offset: 0,
                        compressed_size: compressed.len() as u64,
                        uncompressed_size: match &column.data {
                            ColumnData::Int64(v) => v.len() as u64,
                            ColumnData::Float64(v) => v.len() as u64,
                            ColumnData::Utf8(v) => v.len() as u64,
                            ColumnData::Timestamp(v) => v.len() as u64,
                        },
                        stats,
                    },
                    compressed,
                ));
                chunk_counts[col_idx] += 1;
            }
        }

        let meta_size: usize = all_encoded
            .iter()
            .map(|(m, _)| meta_serialized_size(m))
            .sum();

        let header_len = header_start + 4 + schema_bytes.len() + 4 + meta_size;

        let data_start = header_len as u64;
        let mut output = Vec::with_capacity(
            header_len + all_encoded.iter().map(|(_, d)| d.len()).sum::<usize>(),
        );

        output.extend_from_slice(MAGIC);
        output.extend_from_slice(&VERSION.to_le_bytes());
        output.extend_from_slice(&(schema_bytes.len() as u32).to_le_bytes());
        output.extend_from_slice(&schema_bytes);
        output.extend_from_slice(&(all_encoded.len() as u32).to_le_bytes());

        let mut offset = data_start;
        for (meta, _) in &all_encoded {
            let mut m = meta.clone();
            m.offset = offset;
            offset += m.compressed_size;
            output.extend_from_slice(&m.to_bytes());
        }

        for (_, data) in &all_encoded {
            output.extend_from_slice(data);
        }

        output
    }
}

fn meta_serialized_size(m: &ChunkMeta) -> usize {
    let stats_size =
        if m.stats.min.is_some() { 9 } else { 1 } + if m.stats.max.is_some() { 9 } else { 1 } + 8;
    4 + 4 + 2 + 8 + 8 + 8 + stats_size
}

pub struct BlobReader;

impl BlobReader {
    pub fn read_header(bytes: &[u8]) -> (Schema, Vec<ChunkMeta>) {
        if bytes.len() < 8 {
            return (Schema::new(vec![]), Vec::new());
        }
        let mut pos = 0;
        let _magic = &bytes[pos..pos + 4];
        pos += 4;
        let _version = u32::from_le_bytes(bytes[pos..pos + 4].try_into().unwrap());
        pos += 4;
        let schema_len = u32::from_le_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
        pos += 4;
        let schema = deserialize_schema(&bytes[pos..pos + schema_len]);
        pos += schema_len;
        let num_chunks = u32::from_le_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
        pos += 4;

        let mut chunks = Vec::with_capacity(num_chunks);
        for _ in 0..num_chunks {
            let (meta, n) = ChunkMeta::from_bytes(&bytes[pos..]);
            pos += n;
            chunks.push(meta);
        }

        (schema, chunks)
    }

    pub fn read_all(bytes: &[u8]) -> Vec<RecordBatch> {
        let (schema, chunks) = Self::read_header(bytes);
        if chunks.is_empty() {
            return Vec::new();
        }

        let num_cols = schema.fields.len();
        let mut batch_map: BTreeMap<u32, Vec<(u32, &ChunkMeta, &[u8])>> = BTreeMap::new();

        for meta in &chunks {
            let start = meta.offset as usize;
            let end = start + meta.compressed_size as usize;
            if end > bytes.len() {
                continue;
            }
            let data = &bytes[start..end];
            batch_map
                .entry(meta.chunk_idx)
                .or_default()
                .push((meta.col_idx, meta, data));
        }

        let mut batches = Vec::new();
        for (_, cols) in &batch_map {
            let mut columns = Vec::with_capacity(num_cols);
            let mut sorted_cols = cols.clone();
            sorted_cols.sort_by_key(|(idx, _, _)| *idx);
            for (col_idx, meta, data) in &sorted_cols {
                let decoded =
                    encoding::decode_column(meta.encoding, data, meta.uncompressed_size as usize);
                let field = schema.fields[*col_idx as usize].clone();
                columns.push(Column::new(field, decoded));
            }
            let batch = RecordBatch::new(schema.clone(), columns);
            batches.push(batch);
        }

        batches
    }
}

fn serialize_schema(schema: &Schema) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(&(schema.fields.len() as u32).to_le_bytes());
    for field in &schema.fields {
        buf.extend_from_slice(&(field.name.len() as u32).to_le_bytes());
        buf.extend_from_slice(field.name.as_bytes());
        let type_id: u8 = match field.data_type {
            DataType::Int64 => 0,
            DataType::Float64 => 1,
            DataType::Utf8 => 2,
            DataType::Timestamp => 3,
        };
        buf.push(type_id);
        buf.push(field.nullable as u8);
    }
    buf
}

fn deserialize_schema(bytes: &[u8]) -> Schema {
    let mut pos = 0;
    let num_fields = u32::from_le_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
    pos += 4;
    let mut fields = Vec::with_capacity(num_fields);
    for _ in 0..num_fields {
        let name_len = u32::from_le_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
        pos += 4;
        let name = String::from_utf8(bytes[pos..pos + name_len].to_vec()).unwrap();
        pos += name_len;
        let type_id = bytes[pos];
        pos += 1;
        let nullable = bytes[pos] != 0;
        pos += 1;
        let data_type = match type_id {
            0 => DataType::Int64,
            1 => DataType::Float64,
            2 => DataType::Utf8,
            3 => DataType::Timestamp,
            _ => panic!("unknown data type: {}", type_id),
        };
        fields.push(Field::new(&name, data_type, nullable));
    }
    Schema::new(fields)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_blob() {
        let schema = Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("name", DataType::Utf8, false),
            Field::new("score", DataType::Float64, true),
        ]);

        let batch = RecordBatch::new(
            schema.clone(),
            vec![
                Column::new(schema.fields[0].clone(), ColumnData::Int64(vec![1, 2, 3])),
                Column::new(
                    schema.fields[1].clone(),
                    ColumnData::Utf8(vec!["a".into(), "b".into(), "c".into()]),
                ),
                Column::new(
                    schema.fields[2].clone(),
                    ColumnData::Float64(vec![1.0, 2.0, 3.0]),
                ),
            ],
        );

        let blob = BlobWriter::write(&[batch.clone()]);
        assert!(!blob.is_empty());
        let batches = BlobReader::read_all(&blob);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].num_rows, 3);

        let (schema_read, _) = BlobReader::read_header(&blob);
        assert_eq!(schema_read.fields.len(), 3);
    }
}
