use std::collections::HashMap;

use crate::exec::batch::{Column, ColumnData, RecordBatch};
use crate::exec::expr::{Expr, eval_filter};
use crate::schema::Schema;
use crate::storage::chunk::ChunkMeta;
use crate::storage::encoding;
use crate::storage::format::BlobReader;

pub type BoxedOperator = Box<dyn PhysicalOperator>;

pub trait PhysicalOperator: Send {
    fn next_batch(&mut self) -> Option<RecordBatch>;
}

pub struct PhysicalScan {
    batches: Vec<RecordBatch>,
    idx: usize,
    projection: Vec<String>,
    filter: Option<Expr>,
}

impl PhysicalScan {
    pub fn new(batches: Vec<RecordBatch>, projection: Vec<String>, filter: Option<Expr>) -> Self {
        Self {
            batches,
            idx: 0,
            projection,
            filter,
        }
    }
}

impl PhysicalOperator for PhysicalScan {
    fn next_batch(&mut self) -> Option<RecordBatch> {
        let batch = self.batches.get(self.idx)?;
        self.idx += 1;

        let mut result = batch.clone();

        // apply filter if present
        if let Some(ref pred) = self.filter {
            let mask = crate::exec::expr::eval_filter(pred, &result);
            let mut new_cols = Vec::with_capacity(result.columns.len());
            for col in &result.columns {
                let new_data = match &col.data {
                    crate::exec::batch::ColumnData::Int64(v) => {
                        let filtered: Vec<i64> = v
                            .iter()
                            .enumerate()
                            .filter(|(i, _)| mask[*i])
                            .map(|(_, val)| *val)
                            .collect();
                        crate::exec::batch::ColumnData::Int64(filtered)
                    }
                    crate::exec::batch::ColumnData::Float64(v) => {
                        let filtered: Vec<f64> = v
                            .iter()
                            .enumerate()
                            .filter(|(i, _)| mask[*i])
                            .map(|(_, val)| *val)
                            .collect();
                        crate::exec::batch::ColumnData::Float64(filtered)
                    }
                    crate::exec::batch::ColumnData::Utf8(v) => {
                        let filtered: Vec<String> = v
                            .iter()
                            .enumerate()
                            .filter(|(i, _)| mask[*i])
                            .map(|(_, val)| val.clone())
                            .collect();
                        crate::exec::batch::ColumnData::Utf8(filtered)
                    }
                    crate::exec::batch::ColumnData::Timestamp(v) => {
                        let filtered: Vec<i64> = v
                            .iter()
                            .enumerate()
                            .filter(|(i, _)| mask[*i])
                            .map(|(_, val)| *val)
                            .collect();
                        crate::exec::batch::ColumnData::Timestamp(filtered)
                    }
                };
                new_cols.push(crate::exec::batch::Column::new(col.field.clone(), new_data));
            }
            result = RecordBatch::new(result.schema.clone(), new_cols);
        }

        if !self.projection.is_empty() {
            let cols: Vec<_> = self
                .projection
                .iter()
                .filter_map(|name| {
                    let idx = result.schema.index_of(name)?;
                    Some(result.columns[idx].clone())
                })
                .collect();
            let fields: Vec<_> = self
                .projection
                .iter()
                .filter_map(|name| {
                    let idx = result.schema.index_of(name)?;
                    Some(result.schema.fields[idx].clone())
                })
                .collect();
            let new_schema = crate::schema::Schema::new(fields);
            result = RecordBatch::new(new_schema, cols);
        }

        Some(result)
    }
}

pub struct PhysicalFilter {
    input: BoxedOperator,
    predicate: Expr,
}

impl PhysicalFilter {
    pub fn new(input: BoxedOperator, predicate: Expr) -> Self {
        Self { input, predicate }
    }
}

impl PhysicalOperator for PhysicalFilter {
    fn next_batch(&mut self) -> Option<RecordBatch> {
        let batch = self.input.next_batch()?;
        let mask = crate::exec::expr::eval_filter(&self.predicate, &batch);
        let mut new_cols = Vec::with_capacity(batch.columns.len());
        for col in &batch.columns {
            let new_data = match &col.data {
                crate::exec::batch::ColumnData::Int64(v) => {
                    let filtered: Vec<i64> = v
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| mask[*i])
                        .map(|(_, val)| *val)
                        .collect();
                    crate::exec::batch::ColumnData::Int64(filtered)
                }
                crate::exec::batch::ColumnData::Float64(v) => {
                    let filtered: Vec<f64> = v
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| mask[*i])
                        .map(|(_, val)| *val)
                        .collect();
                    crate::exec::batch::ColumnData::Float64(filtered)
                }
                crate::exec::batch::ColumnData::Utf8(v) => {
                    let filtered: Vec<String> = v
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| mask[*i])
                        .map(|(_, val)| val.clone())
                        .collect();
                    crate::exec::batch::ColumnData::Utf8(filtered)
                }
                crate::exec::batch::ColumnData::Timestamp(v) => {
                    let filtered: Vec<i64> = v
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| mask[*i])
                        .map(|(_, val)| *val)
                        .collect();
                    crate::exec::batch::ColumnData::Timestamp(filtered)
                }
            };
            new_cols.push(crate::exec::batch::Column::new(col.field.clone(), new_data));
        }

        Some(RecordBatch::new(batch.schema, new_cols))
    }
}

pub struct PhysicalProject {
    input: BoxedOperator,
    columns: Vec<String>,
}

impl PhysicalProject {
    pub fn new(input: BoxedOperator, columns: Vec<String>) -> Self {
        Self { input, columns }
    }
}

impl PhysicalOperator for PhysicalProject {
    fn next_batch(&mut self) -> Option<RecordBatch> {
        let batch = self.input.next_batch()?;
        let cols: Vec<_> = self
            .columns
            .iter()
            .filter_map(|name| {
                let idx = batch.schema.index_of(name)?;
                Some(batch.columns[idx].clone())
            })
            .collect();
        let fields: Vec<_> = self
            .columns
            .iter()
            .filter_map(|name| {
                let idx = batch.schema.index_of(name)?;
                Some(batch.schema.fields[idx].clone())
            })
            .collect();
        let new_schema = crate::schema::Schema::new(fields);
        Some(RecordBatch::new(new_schema, cols))
    }
}

pub struct PhysicalAggregate {
    input: BoxedOperator,
    aggs: Vec<(crate::exec::logical_plan::AggOp, String, String)>,
    group_by: Vec<String>,
    done: bool,
}

impl PhysicalAggregate {
    pub fn new(
        input: BoxedOperator,
        aggs: Vec<(crate::exec::logical_plan::AggOp, String, String)>,
        group_by: Vec<String>,
    ) -> Self {
        Self {
            input,
            aggs,
            group_by,
            done: false,
        }
    }
}

impl PhysicalOperator for PhysicalAggregate {
    fn next_batch(&mut self) -> Option<RecordBatch> {
        if self.done {
            return None;
        }
        self.done = true;

        let mut accum: HashMap<Vec<String>, Vec<Accumulator>> = HashMap::new();

        while let Some(batch) = self.input.next_batch() {
            for row in 0..batch.num_rows {
                let key: Vec<String> = self
                    .group_by
                    .iter()
                    .map(|col| {
                        let idx = batch.schema.index_of(col).unwrap();
                        format_value(&batch.columns[idx], row)
                    })
                    .collect();

                let entry = accum.entry(key).or_insert_with(|| {
                    self.aggs
                        .iter()
                        .map(|(op, _, _)| match op {
                            crate::exec::logical_plan::AggOp::Sum => Accumulator::Sum(0.0),
                            crate::exec::logical_plan::AggOp::Count => Accumulator::Count(0u64),
                            crate::exec::logical_plan::AggOp::Avg => Accumulator::Avg(0.0, 0u64),
                            crate::exec::logical_plan::AggOp::Min => Accumulator::Min(f64::MAX),
                            crate::exec::logical_plan::AggOp::Max => Accumulator::Max(f64::MIN),
                        })
                        .collect()
                });

                for (i, (op, input_col, _)) in self.aggs.iter().enumerate() {
                    let idx = batch.schema.index_of(input_col).unwrap();
                    let val = numeric_value(&batch.columns[idx], row);
                    if let Some(n) = val {
                        match op {
                            crate::exec::logical_plan::AggOp::Sum => {
                                if let Accumulator::Sum(ref mut s) = entry[i] {
                                    *s += n;
                                }
                            }
                            crate::exec::logical_plan::AggOp::Count => {
                                if let Accumulator::Count(ref mut c) = entry[i] {
                                    *c += 1;
                                }
                            }
                            crate::exec::logical_plan::AggOp::Avg => {
                                if let Accumulator::Avg(ref mut s, ref mut c) = entry[i] {
                                    *s += n;
                                    *c += 1;
                                }
                            }
                            crate::exec::logical_plan::AggOp::Min => {
                                if let Accumulator::Min(ref mut m) = entry[i]
                                    && n < *m
                                {
                                    *m = n;
                                }
                            }
                            crate::exec::logical_plan::AggOp::Max => {
                                if let Accumulator::Max(ref mut m) = entry[i]
                                    && n > *m
                                {
                                    *m = n;
                                }
                            }
                        }
                    }
                }
            }
        }

        if accum.is_empty() {
            return None;
        }

        let mut output_fields = Vec::new();
        for gb in &self.group_by {
            output_fields.push(crate::schema::Field::new(
                gb,
                crate::schema::DataType::Utf8,
                true,
            ));
        }
        for (_, _, out_name) in &self.aggs {
            output_fields.push(crate::schema::Field::new(
                out_name,
                crate::schema::DataType::Float64,
                true,
            ));
        }
        let schema = crate::schema::Schema::new(output_fields);

        let mut string_cols: Vec<Vec<String>> = self.group_by.iter().map(|_| Vec::new()).collect();
        let mut float_cols: Vec<Vec<f64>> = self.aggs.iter().map(|_| Vec::new()).collect();

        for (key, accums) in &accum {
            for (i, val) in key.iter().enumerate() {
                string_cols[i].push(val.clone());
            }
            for (i, accum) in accums.iter().enumerate() {
                float_cols[i].push(accum.value());
            }
        }

        let mut columns = Vec::new();
        for (i, col) in self.group_by.iter().enumerate() {
            let field = crate::schema::Field::new(col, crate::schema::DataType::Utf8, true);
            columns.push(crate::exec::batch::Column::new(
                field,
                crate::exec::batch::ColumnData::Utf8(string_cols[i].clone()),
            ));
        }
        for (i, (_, _, out_name)) in self.aggs.iter().enumerate() {
            let field = crate::schema::Field::new(out_name, crate::schema::DataType::Float64, true);
            columns.push(crate::exec::batch::Column::new(
                field,
                crate::exec::batch::ColumnData::Float64(float_cols[i].clone()),
            ));
        }

        Some(RecordBatch::new(schema, columns))
    }
}

fn format_value(col: &crate::exec::batch::Column, row: usize) -> String {
    match &col.data {
        crate::exec::batch::ColumnData::Int64(v) => format!("{}", v[row]),
        crate::exec::batch::ColumnData::Float64(v) => format!("{}", v[row]),
        crate::exec::batch::ColumnData::Utf8(v) => v[row].clone(),
        crate::exec::batch::ColumnData::Timestamp(v) => format!("{}", v[row]),
    }
}

fn numeric_value(col: &crate::exec::batch::Column, row: usize) -> Option<f64> {
    match &col.data {
        crate::exec::batch::ColumnData::Int64(v) => Some(v[row] as f64),
        crate::exec::batch::ColumnData::Float64(v) => Some(v[row]),
        crate::exec::batch::ColumnData::Utf8(_) => None,
        crate::exec::batch::ColumnData::Timestamp(v) => Some(v[row] as f64),
    }
}

pub struct FileScan {
    #[allow(dead_code)]
    blob_id: String,
    bytes: Vec<u8>,
    schema: Schema,
    chunks: Vec<ChunkMeta>,
    projection: Vec<String>,
    filter: Option<Expr>,
    current_chunk: u32,
    max_chunk_idx: u32,
}

impl FileScan {
    pub fn new(
        blob_id: String,
        bytes: Vec<u8>,
        projection: Vec<String>,
        filter: Option<Expr>,
    ) -> Self {
        let (schema, chunks) = BlobReader::read_header(&bytes);
        let max_chunk_idx = chunks.iter().map(|m| m.chunk_idx).max().unwrap_or(0) + 1;
        Self {
            blob_id,
            bytes,
            schema,
            chunks,
            projection,
            filter,
            current_chunk: 0,
            max_chunk_idx,
        }
    }
}

impl PhysicalOperator for FileScan {
    fn next_batch(&mut self) -> Option<RecordBatch> {
        let num_cols = self.schema.fields.len();

        while self.current_chunk < self.max_chunk_idx {
            let chunk_idx = self.current_chunk;
            self.current_chunk += 1;

            let col_metas: Vec<&ChunkMeta> = self
                .chunks
                .iter()
                .filter(|m| m.chunk_idx == chunk_idx)
                .collect();

            if col_metas.is_empty() {
                continue;
            }

            if let Some(ref pred) = self.filter {
                let ref_cols = crate::exec::expr::referenced_columns(pred);
                let mut can_skip = false;
                for col_name in &ref_cols {
                    if let Some(col_idx) = self.schema.index_of(col_name)
                        && let Some(meta) = col_metas.iter().find(|m| m.col_idx == col_idx as u32)
                        && let (Some(min), Some(max)) = (meta.stats.min, meta.stats.max)
                        && !crate::exec::expr::could_match(pred, col_name, min, max)
                    {
                        can_skip = true;
                        break;
                    }
                }
                if can_skip {
                    continue;
                }
            }

            let mut columns = Vec::with_capacity(num_cols);
            let mut sorted_metas = col_metas.clone();
            sorted_metas.sort_by_key(|m| m.col_idx);

            for meta in &sorted_metas {
                let col_idx = meta.col_idx as usize;
                let start = meta.offset as usize;
                let end = start + meta.compressed_size as usize;
                let raw = self.bytes[start..end].to_vec();
                let decoded =
                    encoding::decode_column(meta.encoding, &raw, meta.uncompressed_size as usize);
                let field = self.schema.fields[col_idx].clone();
                columns.push(Column::new(field, decoded));
            }

            if let Some(ref pred) = self.filter {
                let tmp = RecordBatch::new(self.schema.clone(), columns);
                let mask = eval_filter(pred, &tmp);
                let mut new_cols = Vec::with_capacity(tmp.columns.len());
                for col in &tmp.columns {
                    let new_data = match &col.data {
                        ColumnData::Int64(v) => ColumnData::Int64(
                            v.iter()
                                .enumerate()
                                .filter(|(i, _)| mask[*i])
                                .map(|(_, vv)| *vv)
                                .collect(),
                        ),
                        ColumnData::Float64(v) => ColumnData::Float64(
                            v.iter()
                                .enumerate()
                                .filter(|(i, _)| mask[*i])
                                .map(|(_, vv)| *vv)
                                .collect(),
                        ),
                        ColumnData::Utf8(v) => ColumnData::Utf8(
                            v.iter()
                                .enumerate()
                                .filter(|(i, _)| mask[*i])
                                .map(|(_, vv)| vv.clone())
                                .collect(),
                        ),
                        ColumnData::Timestamp(v) => ColumnData::Timestamp(
                            v.iter()
                                .enumerate()
                                .filter(|(i, _)| mask[*i])
                                .map(|(_, vv)| *vv)
                                .collect(),
                        ),
                    };
                    new_cols.push(Column::new(col.field.clone(), new_data));
                }
                columns = new_cols;
            }

            let batch_schema = self.schema.clone();

            if !self.projection.is_empty() {
                let projected_cols: Vec<_> = self
                    .projection
                    .iter()
                    .filter_map(|name| {
                        let idx = batch_schema.index_of(name)?;
                        Some(columns[idx].clone())
                    })
                    .collect();
                let projected_fields: Vec<_> = self
                    .projection
                    .iter()
                    .filter_map(|name| {
                        let idx = batch_schema.index_of(name)?;
                        Some(batch_schema.fields[idx].clone())
                    })
                    .collect();
                let new_schema = Schema::new(projected_fields);
                return Some(RecordBatch::new(new_schema, projected_cols));
            }

            return Some(RecordBatch::new(batch_schema, columns));
        }

        None
    }
}

enum Accumulator {
    Sum(f64),
    Count(u64),
    Avg(f64, u64),
    Min(f64),
    Max(f64),
}

impl Accumulator {
    fn value(&self) -> f64 {
        match self {
            Accumulator::Sum(s) => *s,
            Accumulator::Count(c) => *c as f64,
            Accumulator::Avg(s, c) => {
                if *c == 0 {
                    0.0
                } else {
                    *s / *c as f64
                }
            }
            Accumulator::Min(m) => {
                if *m == f64::MAX {
                    0.0
                } else {
                    *m
                }
            }
            Accumulator::Max(m) => {
                if *m == f64::MIN {
                    0.0
                } else {
                    *m
                }
            }
        }
    }
}
