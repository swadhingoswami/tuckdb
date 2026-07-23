#[derive(Debug, Clone)]
pub struct ColumnStats {
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub null_count: usize,
}

impl ColumnStats {
    pub fn new(min: Option<f64>, max: Option<f64>, null_count: usize) -> Self {
        Self {
            min,
            max,
            null_count,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        match self.min {
            Some(v) => {
                buf.push(1u8);
                buf.extend_from_slice(&v.to_le_bytes());
            }
            None => buf.push(0u8),
        }
        match self.max {
            Some(v) => {
                buf.push(1u8);
                buf.extend_from_slice(&v.to_le_bytes());
            }
            None => buf.push(0u8),
        }
        buf.extend_from_slice(&(self.null_count as u64).to_le_bytes());
        buf
    }

    pub fn from_bytes(bytes: &[u8]) -> (Self, usize) {
        let mut pos = 0;
        let min = if bytes[pos] == 1 {
            pos += 1;
            let v = f64::from_le_bytes(bytes[pos..pos + 8].try_into().unwrap());
            pos += 8;
            Some(v)
        } else {
            pos += 1;
            None
        };
        let max = if bytes[pos] == 1 {
            pos += 1;
            let v = f64::from_le_bytes(bytes[pos..pos + 8].try_into().unwrap());
            pos += 8;
            Some(v)
        } else {
            pos += 1;
            None
        };
        let null_count = u64::from_le_bytes(bytes[pos..pos + 8].try_into().unwrap()) as usize;
        pos += 8;
        (ColumnStats::new(min, max, null_count), pos)
    }
}

#[derive(Debug, Clone)]
pub struct ChunkMeta {
    pub col_idx: u32,
    pub chunk_idx: u32,
    pub encoding: u16,
    pub offset: u64,
    pub compressed_size: u64,
    pub uncompressed_size: u64,
    pub stats: ColumnStats,
}

impl ChunkMeta {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&self.col_idx.to_le_bytes());
        buf.extend_from_slice(&self.chunk_idx.to_le_bytes());
        buf.extend_from_slice(&self.encoding.to_le_bytes());
        buf.extend_from_slice(&self.offset.to_le_bytes());
        buf.extend_from_slice(&self.compressed_size.to_le_bytes());
        buf.extend_from_slice(&self.uncompressed_size.to_le_bytes());
        buf.extend_from_slice(&self.stats.to_bytes());
        buf
    }

    pub fn from_bytes(bytes: &[u8]) -> (Self, usize) {
        let mut pos = 0;
        let col_idx = u32::from_le_bytes(bytes[pos..pos + 4].try_into().unwrap());
        pos += 4;
        let chunk_idx = u32::from_le_bytes(bytes[pos..pos + 4].try_into().unwrap());
        pos += 4;
        let encoding = u16::from_le_bytes(bytes[pos..pos + 2].try_into().unwrap());
        pos += 2;
        let offset = u64::from_le_bytes(bytes[pos..pos + 8].try_into().unwrap());
        pos += 8;
        let compressed_size = u64::from_le_bytes(bytes[pos..pos + 8].try_into().unwrap());
        pos += 8;
        let uncompressed_size = u64::from_le_bytes(bytes[pos..pos + 8].try_into().unwrap());
        pos += 8;
        let (stats, n) = ColumnStats::from_bytes(&bytes[pos..]);
        pos += n;
        (
            ChunkMeta {
                col_idx,
                chunk_idx,
                encoding,
                offset,
                compressed_size,
                uncompressed_size,
                stats,
            },
            pos,
        )
    }
}

#[derive(Debug, Clone)]
pub struct EncodedChunk {
    pub meta: ChunkMeta,
    pub data: Vec<u8>,
}

impl EncodedChunk {
    pub fn new(meta: ChunkMeta, data: Vec<u8>) -> Self {
        Self { meta, data }
    }
}
