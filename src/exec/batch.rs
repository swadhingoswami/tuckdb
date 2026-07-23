use crate::schema::{DataType, Field, Schema};

#[derive(Debug, Clone)]
pub enum ColumnData {
    Int64(Vec<i64>),
    Float64(Vec<f64>),
    Utf8(Vec<String>),
    Timestamp(Vec<i64>),
}

impl ColumnData {
    pub fn len(&self) -> usize {
        match self {
            ColumnData::Int64(v) => v.len(),
            ColumnData::Float64(v) => v.len(),
            ColumnData::Utf8(v) => v.len(),
            ColumnData::Timestamp(v) => v.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn data_type(&self) -> DataType {
        match self {
            ColumnData::Int64(_) => DataType::Int64,
            ColumnData::Float64(_) => DataType::Float64,
            ColumnData::Utf8(_) => DataType::Utf8,
            ColumnData::Timestamp(_) => DataType::Timestamp,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Column {
    pub field: Field,
    pub data: ColumnData,
}

impl Column {
    pub fn new(field: Field, data: ColumnData) -> Self {
        assert_eq!(field.data_type, data.data_type());
        Self { field, data }
    }
}

#[derive(Debug, Clone)]
pub struct RecordBatch {
    pub schema: Schema,
    pub columns: Vec<Column>,
    pub num_rows: usize,
}

impl RecordBatch {
    pub fn new(schema: Schema, columns: Vec<Column>) -> Self {
        let num_rows = if columns.is_empty() {
            0
        } else {
            columns[0].data.len()
        };
        for col in &columns {
            assert_eq!(
                col.data.len(),
                num_rows,
                "all columns must have same length"
            );
        }
        Self {
            schema,
            columns,
            num_rows,
        }
    }
}
