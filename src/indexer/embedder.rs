use anyhow::{Context, Result};
use ort::value::Tensor;
use std::path::Path;

/// Wrapper around ONNX Runtime for generating text embeddings
pub struct Embedder {
    session: ort::session::Session,
    tokenizer: tokenizers::Tokenizer,
    max_tokens: usize,
}

/// Embedding dimension for all-MiniLM-L6-v2
pub const EMBEDDING_DIM: usize = 384;

impl Embedder {
    /// Creates a new embedder from model files in the given directory
    pub fn new(model_dir: &Path) -> Result<Self> {
        let model_path = model_dir.join("model.onnx");
        let tokenizer_path = model_dir.join("tokenizer.json");

        // Initialize ONNX Runtime session
        let session = ort::session::Session::builder()
            .context("Failed to create ONNX session builder")?
            .with_intra_threads(1)
            .context("Failed to set thread count")?
            .commit_from_file(&model_path)
            .with_context(|| format!("Failed to load ONNX model from {:?}", model_path))?;

        // Load tokenizer
        let tokenizer = tokenizers::Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| anyhow::anyhow!("Failed to load tokenizer: {}", e))?;

        Ok(Self {
            session,
            tokenizer,
            max_tokens: 512,
        })
    }

    /// Generates an embedding for the given text.
    /// For long texts, chunks into overlapping segments and mean-pools.
    pub fn embed(&mut self, text: &str) -> Result<Vec<f32>> {
        let text = text.trim();
        if text.is_empty() {
            return Ok(vec![0.0; EMBEDDING_DIM]);
        }

        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| anyhow::anyhow!("Tokenization failed: {}", e))?;

        let token_count = encoding.get_ids().len();

        if token_count <= self.max_tokens {
            // Single-pass embedding
            self.embed_tokens(encoding.get_ids(), encoding.get_attention_mask())
        } else {
            // Chunked embedding with mean pooling
            self.embed_chunked(text)
        }
    }

    /// Embeds a single token sequence using ort v2 Tensor API
    fn embed_tokens(&mut self, input_ids: &[u32], attention_mask: &[u32]) -> Result<Vec<f32>> {
        let seq_len = input_ids.len();

        let input_ids_i64: Vec<i64> = input_ids.iter().map(|&x| x as i64).collect();
        let attention_mask_i64: Vec<i64> = attention_mask.iter().map(|&x| x as i64).collect();
        let token_type_ids: Vec<i64> = vec![0i64; seq_len];

        let shape = vec![1i64, seq_len as i64];

        let input_ids_tensor = Tensor::from_array((shape.clone(), input_ids_i64))
            .context("Failed to create input_ids tensor")?;
        let attention_mask_tensor = Tensor::from_array((shape.clone(), attention_mask_i64))
            .context("Failed to create attention_mask tensor")?;
        let token_type_ids_tensor = Tensor::from_array((shape, token_type_ids))
            .context("Failed to create token_type_ids tensor")?;

        let outputs = self
            .session
            .run(ort::inputs! {
                "input_ids" => input_ids_tensor,
                "attention_mask" => attention_mask_tensor,
                "token_type_ids" => token_type_ids_tensor,
            })
            .context("ONNX inference failed")?;

        // Get the output tensor (last_hidden_state: [1, seq_len, 384])
        // try_extract_tensor returns (&Shape, &[f32])
        let (_shape, data) = outputs[0]
            .try_extract_tensor::<f32>()
            .context("Failed to extract output tensor")?;

        // Mean pooling over the sequence dimension with attention mask
        let mask_f32: Vec<f32> = attention_mask.iter().map(|&x| x as f32).collect();
        let embedding = mean_pool_flat(data, &mask_f32, seq_len, EMBEDDING_DIM);

        // L2 normalize
        Ok(l2_normalize(&embedding))
    }

    /// Chunks long text and mean-pools the chunk embeddings
    fn embed_chunked(&mut self, text: &str) -> Result<Vec<f32>> {
        let chunk_size = self.max_tokens - 2; // Reserve for [CLS] and [SEP]
        let overlap = 50; // Token overlap between chunks

        let encoding = self
            .tokenizer
            .encode(text, false)
            .map_err(|e| anyhow::anyhow!("Tokenization failed: {}", e))?;

        let all_ids = encoding.get_ids();
        let mut embeddings: Vec<Vec<f32>> = Vec::new();
        let mut start = 0;

        while start < all_ids.len() {
            let end = (start + chunk_size).min(all_ids.len());
            let chunk_ids = &all_ids[start..end];

            // Add [CLS] and [SEP] tokens
            let mut padded_ids = vec![101u32]; // [CLS]
            padded_ids.extend_from_slice(chunk_ids);
            padded_ids.push(102); // [SEP]

            let attention_mask: Vec<u32> = vec![1; padded_ids.len()];

            let emb = self.embed_tokens(&padded_ids, &attention_mask)?;
            embeddings.push(emb);

            if end >= all_ids.len() {
                break;
            }
            start = end - overlap;
        }

        if embeddings.is_empty() {
            return Ok(vec![0.0; EMBEDDING_DIM]);
        }

        // Mean pool across chunks
        let mut result = vec![0.0f32; EMBEDDING_DIM];
        for emb in &embeddings {
            for (i, val) in emb.iter().enumerate() {
                result[i] += val;
            }
        }
        let n = embeddings.len() as f32;
        for val in &mut result {
            *val /= n;
        }

        Ok(l2_normalize(&result))
    }
}

/// Mean pooling on a flat f32 slice with shape [1, seq_len, embedding_dim]
fn mean_pool_flat(data: &[f32], mask: &[f32], seq_len: usize, dim: usize) -> Vec<f32> {
    let mut result = vec![0.0f32; dim];
    let mut total_weight = 0.0f32;

    for (i, &w) in mask.iter().enumerate().take(seq_len) {
        total_weight += w;
        let offset = i * dim; // data layout: [batch=0, token=i, dim=j]
        for j in 0..dim {
            result[j] += data[offset + j] * w;
        }
    }

    if total_weight > 0.0 {
        for val in &mut result {
            *val /= total_weight;
        }
    }

    result
}

/// L2 normalization
fn l2_normalize(vec: &[f32]) -> Vec<f32> {
    let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        vec.iter().map(|x| x / norm).collect()
    } else {
        vec.to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_l2_normalize() {
        let v = vec![3.0, 4.0];
        let n = l2_normalize(&v);
        assert!((n[0] - 0.6).abs() < 1e-6);
        assert!((n[1] - 0.8).abs() < 1e-6);
    }

    #[test]
    fn test_l2_normalize_zero() {
        let v = vec![0.0, 0.0];
        let n = l2_normalize(&v);
        assert_eq!(n, vec![0.0, 0.0]);
    }

    #[test]
    fn test_mean_pool_flat() {
        // 1 token, dim=3
        let data = vec![1.0, 2.0, 3.0];
        let mask = vec![1.0];
        let result = mean_pool_flat(&data, &mask, 1, 3);
        assert_eq!(result, vec![1.0, 2.0, 3.0]);

        // 2 tokens, dim=2, one masked out
        let data = vec![1.0, 2.0, 3.0, 4.0];
        let mask = vec![1.0, 0.0];
        let result = mean_pool_flat(&data, &mask, 2, 2);
        assert_eq!(result, vec![1.0, 2.0]);
    }
}
