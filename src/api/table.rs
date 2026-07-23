use std::path::PathBuf;

use crate::cache::{DataCache, EvictionPolicy, ResultCache};
use crate::exec::batch::RecordBatch;
use crate::exec::expr::Expr;
use crate::exec::logical_plan::LogicalPlan;
use crate::exec::optimizer::Optimizer;
use crate::exec::physical_plan::{
    BoxedOperator, FileScan, PhysicalAggregate, PhysicalFilter, PhysicalProject, PhysicalScan,
};

use crate::api::result::ResultSet;
use crate::schema::Schema;
use crate::storage::format::{BlobReader, BlobWriter};

pub struct Table {
    name: String,
    schema: Schema,
    batches: Vec<RecordBatch>,
    persisted: bool,
    blob_bytes: Option<Vec<u8>>,
    data_version: u64,
    #[allow(dead_code)]
    data_cache: DataCache,
    result_cache: ResultCache,
    base_path: PathBuf,
}

impl Table {
    pub fn create(name: &str, schema: Schema, base_path: PathBuf) -> Self {
        Self {
            name: name.to_string(),
            schema,
            batches: Vec::new(),
            persisted: false,
            blob_bytes: None,
            data_version: 0,
            data_cache: DataCache::new(64 * 1024 * 1024, EvictionPolicy::LRU),
            result_cache: ResultCache::new(128),
            base_path,
        }
    }

    pub fn open(name: &str, base_path: PathBuf) -> Self {
        let mut path = base_path.clone();
        path.push(format!("{}.tuck", name));
        let (schema, batches, blob_bytes) = if path.exists() {
            let bytes = std::fs::read(&path).unwrap_or_default();
            let (schema, _chunks) = BlobReader::read_header(&bytes);
            let batches = BlobReader::read_all(&bytes);
            (schema, batches, Some(bytes))
        } else {
            (Schema::new(vec![]), Vec::new(), None)
        };

        Self {
            name: name.to_string(),
            schema,
            batches,
            persisted: true,
            blob_bytes,
            data_version: 1,
            data_cache: DataCache::new(64 * 1024 * 1024, EvictionPolicy::LRU),
            result_cache: ResultCache::new(128),
            base_path,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    pub fn insert_batch(&mut self, batch: RecordBatch) {
        assert_eq!(
            self.schema, batch.schema,
            "batch schema must match table schema"
        );
        self.batches.push(batch);
        self.data_version += 1;
        self.result_cache.invalidate();
    }

    pub fn scan_all(&self) -> ResultSet {
        ResultSet::new(self.batches.clone())
    }

    pub fn num_rows(&self) -> usize {
        self.batches.iter().map(|b| b.num_rows).sum()
    }

    pub fn execute(&mut self, plan: LogicalPlan) -> ResultSet {
        let optimized = Optimizer::optimize(plan, &self.schema);

        // Build physical plan
        let mut operator = self.build_physical(&optimized);

        let mut batches = Vec::new();
        while let Some(batch) = operator.next_batch() {
            batches.push(batch);
        }
        ResultSet::new(batches)
    }

    fn build_physical(&mut self, plan: &LogicalPlan) -> BoxedOperator {
        match plan {
            LogicalPlan::Scan {
                table: _,
                projection,
                filter,
            } => {
                if self.persisted {
                    if let Some(ref bytes) = self.blob_bytes {
                        return Box::new(FileScan::new(
                            self.name.clone(),
                            bytes.clone(),
                            projection.clone(),
                            filter.clone(),
                        ));
                    }
                }
                let mut batches = self.batches.clone();
                if let Some(pred) = filter {
                    batches = self.apply_chunk_skipping(batches, pred);
                }
                Box::new(PhysicalScan::new(
                    batches,
                    projection.clone(),
                    filter.clone(),
                ))
            }
            LogicalPlan::Filter { input, predicate } => {
                let child = self.build_physical(input);
                Box::new(PhysicalFilter::new(child, predicate.clone()))
            }
            LogicalPlan::Project { input, columns } => {
                let child = self.build_physical(input);
                Box::new(PhysicalProject::new(child, columns.clone()))
            }
            LogicalPlan::Aggregate {
                input,
                aggs,
                group_by,
            } => {
                let child = self.build_physical(input);
                Box::new(PhysicalAggregate::new(
                    child,
                    aggs.clone(),
                    group_by.clone(),
                ))
            }
        }
    }

    fn apply_chunk_skipping(
        &self,
        batches: Vec<RecordBatch>,
        predicate: &Expr,
    ) -> Vec<RecordBatch> {
        let ref_cols = crate::exec::expr::referenced_columns(predicate);
        // For each batch (chunk), check if it can possibly match using stats
        let mut result = Vec::new();
        for batch in &batches {
            let mut possible = true;
            for col_name in &ref_cols {
                if let Some(col_idx) = self.schema.index_of(col_name) {
                    let col = &batch.columns[col_idx];
                    let stats = crate::storage::encoding::column_stats(&col.data);
                    if let Some(min) = stats.min {
                        if let Some(max) = stats.max {
                            if !crate::exec::expr::could_match(predicate, col_name, min, max) {
                                possible = false;
                                break;
                            }
                        }
                    }
                }
            }
            if possible {
                result.push(batch.clone());
            }
        }
        result
    }

    pub fn flush(&mut self) {
        let blob = BlobWriter::write(&self.batches);
        let mut path = self.base_path.clone();
        path.push(format!("{}.tuck", self.name));
        std::fs::create_dir_all(path.parent().unwrap()).ok();
        std::fs::write(&path, &blob).expect("Failed to flush table");
        self.persisted = true;
        self.blob_bytes = Some(blob);
    }

    pub fn data_version(&self) -> u64 {
        self.data_version
    }
}
