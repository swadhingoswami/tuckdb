use std::sync::OnceLock;

use crate::lifecycle::fnv1a;

/// Default embedding dimension for the deterministic mock provider.
pub const DEFAULT_EMBEDDING_DIM: usize = 128;

/// Abstraction over embedding providers.
pub trait EmbeddingProvider: Send + Sync {
    /// Embed text into a fixed-dimension vector.
    fn embed(&self, text: &str) -> Vec<f64>;
    /// Identifier of the embedding model backing this provider.
    fn model_id(&self) -> &str;
    /// Dimension of produced vectors.
    fn dimension(&self) -> usize;
}

/// Deterministic, dependency-free embedding provider based on feature hashing.
/// The same text always produces the same vector, so lifecycle mechanics can
/// be tested before depending on a real model. A small related-terms lexicon
/// stands in for real embedding semantics.
pub struct MockEmbeddingProvider {
    model_id: String,
    dimension: usize,
}

/// Lightweight semantic lexicon: a word expands to related terms so that
/// paraphrases such as "transfer ownership" vs "move constructor" overlap.
const RELATED_TERMS: &[(&str, &[&str])] = &[
    ("transfer", &["move", "ownership", "constructor"]),
    ("ownership", &["move", "transfer", "constructor"]),
    ("move", &["transfer", "ownership", "constructor"]),
    ("constructor", &["transfer", "ownership", "move"]),
    ("raii", &["resource", "acquisition", "initialization"]),
    ("resource", &["raii", "acquisition", "initialization"]),
    ("acquisition", &["raii", "resource", "initialization"]),
    ("initialization", &["raii", "resource", "acquisition"]),
    ("virtual", &["polymorphism", "function"]),
    ("polymorphism", &["virtual", "function"]),
    ("mutex", &["mutual", "exclusion", "concurrent"]),
    ("mutual", &["mutex", "exclusion", "concurrent"]),
    ("exclusion", &["mutex", "mutual", "concurrent"]),
    ("concurrent", &["mutex", "mutual", "exclusion"]),
];

impl MockEmbeddingProvider {
    pub fn new(model_id: &str, dimension: usize) -> Self {
        assert!(dimension > 0, "embedding dimension must be > 0");
        Self {
            model_id: model_id.to_string(),
            dimension,
        }
    }
}

impl Default for MockEmbeddingProvider {
    fn default() -> Self {
        Self::new("model-v1", DEFAULT_EMBEDDING_DIM)
    }
}

static DEFAULT_EMBEDDING_PROVIDER: OnceLock<MockEmbeddingProvider> = OnceLock::new();

/// Shared default provider used for in-query similarity evaluation.
pub fn default_embedding_provider() -> &'static MockEmbeddingProvider {
    DEFAULT_EMBEDDING_PROVIDER.get_or_init(MockEmbeddingProvider::default)
}

impl EmbeddingProvider for MockEmbeddingProvider {
    fn embed(&self, text: &str) -> Vec<f64> {
        let mut vec = vec![0.0; self.dimension];
        let lower = text.to_lowercase();
        for token in lower.split(|c: char| !c.is_alphanumeric()) {
            if token.is_empty() {
                continue;
            }
            add_feature(&mut vec, token.as_bytes(), 2.0);
            if let Some(terms) = related_terms(stem(token)) {
                for term in terms {
                    add_feature(&mut vec, term.as_bytes(), 2.0);
                }
            }
        }
        for n in 2..=3 {
            for ngram in char_ngrams(&lower, n) {
                add_feature(&mut vec, ngram.as_bytes(), 0.25);
            }
        }
        normalize(&mut vec);
        vec
    }

    fn model_id(&self) -> &str {
        &self.model_id
    }

    fn dimension(&self) -> usize {
        self.dimension
    }
}

/// Strip common inflectional suffixes so inflected forms unify.
fn stem(word: &str) -> &str {
    for suffix in ["ing", "es", "s", "ed"] {
        if let Some(stripped) = word.strip_suffix(suffix)
            && !stripped.is_empty()
        {
            return stripped;
        }
    }
    word
}

fn related_terms(word: &str) -> Option<&'static [&'static str]> {
    for (key, terms) in RELATED_TERMS {
        if *key == word {
            return Some(terms);
        }
    }
    None
}

fn add_feature(vec: &mut [f64], bytes: &[u8], weight: f64) {
    let h = fnv1a(bytes);
    let idx = (h % vec.len() as u64) as usize;
    let sign = if ((h >> 63) & 1) == 1 { 1.0 } else { -1.0 };
    vec[idx] += sign * weight;
}

fn char_ngrams(text: &str, n: usize) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() < n {
        return Vec::new();
    }
    chars.windows(n).map(|w| w.iter().collect()).collect()
}

fn normalize(vec: &mut [f64]) {
    let norm = vec.iter().map(|x| x * x).sum::<f64>().sqrt();
    if norm > 0.0 {
        for x in vec.iter_mut() {
            *x /= norm;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embed_is_deterministic() {
        let provider = MockEmbeddingProvider::default();
        assert_eq!(
            provider.embed("What is RAII?"),
            provider.embed("What is RAII?")
        );
    }

    #[test]
    fn different_text_produces_different_vector() {
        let provider = MockEmbeddingProvider::default();
        assert_ne!(
            provider.embed("What is RAII?"),
            provider.embed("What is a virtual function?")
        );
    }

    #[test]
    fn dimension_and_model_id() {
        let provider = MockEmbeddingProvider::new("model-x", 64);
        assert_eq!(provider.embed("hello").len(), 64);
        assert_eq!(provider.model_id(), "model-x");
        assert_eq!(provider.dimension(), 64);
    }

    #[test]
    fn vector_is_unit_length() {
        let provider = MockEmbeddingProvider::default();
        let v = provider.embed("Resource Acquisition Is Initialization");
        let norm = v.iter().map(|x| x * x).sum::<f64>().sqrt();
        assert!((norm - 1.0).abs() < 1e-9);
    }

    #[test]
    fn overlapping_texts_share_similarity() {
        let provider = MockEmbeddingProvider::default();
        let a = provider.embed("ownership transfer constructor");
        let b = provider.embed("ownership transfer copying");
        let dot: f64 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        assert!(dot > 0.0);
    }
}
