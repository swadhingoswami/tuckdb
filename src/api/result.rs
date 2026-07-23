use crate::exec::batch::RecordBatch;

pub struct ResultSet {
    batches: Vec<RecordBatch>,
    idx: usize,
}

impl ResultSet {
    pub fn new(batches: Vec<RecordBatch>) -> Self {
        Self { batches, idx: 0 }
    }

    pub fn next_batch(&mut self) -> Option<&RecordBatch> {
        if self.idx < self.batches.len() {
            let batch = &self.batches[self.idx];
            self.idx += 1;
            Some(batch)
        } else {
            None
        }
    }

    pub fn collect(self) -> Vec<RecordBatch> {
        self.batches
    }
}
