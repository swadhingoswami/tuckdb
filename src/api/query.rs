pub struct Query {
    #[allow(dead_code)]
    table: String,
    projection: Vec<String>,
    filter: Option<String>,
}

impl Query {
    pub fn scan(table: &str) -> Self {
        Self {
            table: table.to_string(),
            projection: Vec::new(),
            filter: None,
        }
    }

    pub fn project(mut self, columns: &[&str]) -> Self {
        self.projection = columns.iter().map(|c| c.to_string()).collect();
        self
    }

    pub fn filter(mut self, predicate: &str) -> Self {
        self.filter = Some(predicate.to_string());
        self
    }
}
