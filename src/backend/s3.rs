use crate::backend::traits::BlobBackend;

pub struct S3Backend {
    client: aws_sdk_s3::Client,
    bucket: String,
    prefix: String,
    runtime: tokio::runtime::Runtime,
}

impl S3Backend {
    pub fn new(bucket: &str, prefix: Option<&str>) -> Self {
        let runtime = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
        let config = runtime.block_on(aws_config::load_defaults(
            aws_config::BehaviorVersion::latest(),
        ));
        let client = aws_sdk_s3::Client::new(&config);
        Self {
            client,
            bucket: bucket.to_string(),
            prefix: prefix.unwrap_or("").to_string(),
            runtime,
        }
    }

    fn key(&self, path: &str) -> String {
        format!("{}{}", self.prefix, path)
    }
}

impl BlobBackend for S3Backend {
    fn read(&self, path: &str, offset: u64, len: u64) -> Result<Vec<u8>, String> {
        let key = self.key(path);
        let range = format!("bytes={}-{}", offset, offset + len - 1);
        let client = self.client.clone();
        let bucket = self.bucket.clone();
        self.runtime.block_on(async {
            let output = client
                .get_object()
                .bucket(&bucket)
                .key(&key)
                .range(&range)
                .send()
                .await
                .map_err(|e| e.to_string())?;
            let data = output.body.collect().await.map_err(|e| e.to_string())?;
            Ok(data.to_vec())
        })
    }

    fn write(&self, path: &str, data: &[u8]) -> Result<(), String> {
        let key = self.key(path);
        let body = aws_sdk_s3::primitives::ByteStream::from(data.to_vec());
        let client = self.client.clone();
        let bucket = self.bucket.clone();
        self.runtime.block_on(async {
            client
                .put_object()
                .bucket(&bucket)
                .key(&key)
                .body(body)
                .send()
                .await
                .map_err(|e| e.to_string())?;
            Ok(())
        })
    }

    fn exists(&self, path: &str) -> bool {
        let key = self.key(path);
        let client = self.client.clone();
        let bucket = self.bucket.clone();
        self.runtime.block_on(async {
            client
                .head_object()
                .bucket(&bucket)
                .key(&key)
                .send()
                .await
                .is_ok()
        })
    }

    fn list(&self, prefix: &str) -> Result<Vec<String>, String> {
        let list_prefix = self.key(prefix);
        let client = self.client.clone();
        let bucket = self.bucket.clone();
        let backend_prefix = self.prefix.clone();
        self.runtime.block_on(async {
            let mut result = Vec::new();
            let mut continuation_token: Option<String> = None;
            loop {
                let mut request = client
                    .list_objects_v2()
                    .bucket(&bucket)
                    .prefix(&list_prefix);
                if let Some(token) = &continuation_token {
                    request = request.continuation_token(token);
                }
                let output = request.send().await.map_err(|e| e.to_string())?;
                if let Some(contents) = output.contents {
                    for obj in contents {
                        if let Some(key) = obj.key {
                            let relative =
                                if !backend_prefix.is_empty() && key.starts_with(&backend_prefix) {
                                    key[backend_prefix.len()..].to_string()
                                } else {
                                    key
                                };
                            result.push(relative);
                        }
                    }
                }
                if output.is_truncated == Some(true) {
                    continuation_token = output.next_continuation_token;
                } else {
                    break;
                }
            }
            result.sort();
            Ok(result)
        })
    }
}
