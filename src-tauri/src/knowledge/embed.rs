//! Sentence embeddings from an XLM-R family ONNX model (int8) + L2 norm.
//! Each model has its own pooling and query/passage prefixes — see `EmbedSpec`.

use std::path::Path;
use std::sync::Mutex;

use anyhow::{anyhow, Result};
use ndarray::Array2;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::TensorRef;
use tokenizers::{PaddingParams, PaddingStrategy, Tokenizer, TruncationParams};

const MAX_TOKENS: usize = 512;
const BATCH: usize = 8;

#[derive(Clone, Copy, Debug)]
pub enum Pooling {
    Mean,
    Cls,
}

#[derive(Clone, Copy, Debug)]
pub struct EmbedSpec {
    pub query_prefix: &'static str,
    pub passage_prefix: &'static str,
    pub pooling: Pooling,
}

pub const E5: EmbedSpec = EmbedSpec { query_prefix: "query: ", passage_prefix: "passage: ", pooling: Pooling::Mean };
pub const BGE_M3: EmbedSpec = EmbedSpec { query_prefix: "", passage_prefix: "", pooling: Pooling::Cls };

pub struct Embedder {
    spec: EmbedSpec,
    session: Mutex<Session>,
    tokenizer: Tokenizer,
    needs_token_types: bool,
    output_name: String,
}

impl Embedder {
    pub fn load(dir: &Path, spec: EmbedSpec) -> Result<Self> {
        let mut tokenizer =
            Tokenizer::from_file(dir.join("tokenizer.json")).map_err(|e| anyhow!("tokenizer: {e}"))?;
        let pad_id = tokenizer.token_to_id("<pad>").unwrap_or(1);
        tokenizer
            .with_truncation(Some(TruncationParams {
                max_length: MAX_TOKENS,
                ..Default::default()
            }))
            .map_err(|e| anyhow!("tokenizer: {e}"))?;
        tokenizer.with_padding(Some(PaddingParams {
            strategy: PaddingStrategy::BatchLongest,
            pad_id,
            pad_token: "<pad>".into(),
            ..Default::default()
        }));

        // Few threads: indexing must not starve live transcription.
        let session = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_intra_threads(4)?
            .commit_from_file(dir.join("model_quantized.onnx"))?;
        let needs_token_types = session.inputs.iter().any(|i| i.name == "token_type_ids");
        let output_name = session
            .outputs
            .first()
            .map(|o| o.name.clone())
            .ok_or_else(|| anyhow!("embedding model has no outputs"))?;

        Ok(Self {
            spec,
            session: Mutex::new(session),
            tokenizer,
            needs_token_types,
            output_name,
        })
    }

    pub fn embed_query(&self, q: &str) -> Result<Vec<f32>> {
        Ok(self.embed_batch(&[format!("{}{q}", self.spec.query_prefix)])?.remove(0))
    }

    pub fn embed_passages(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let texts: Vec<String> = texts.iter().map(|t| format!("{}{t}", self.spec.passage_prefix)).collect();
        let mut out = Vec::with_capacity(texts.len());
        for batch in texts.chunks(BATCH) {
            out.extend(self.embed_batch(batch)?);
        }
        Ok(out)
    }

    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let enc = self
            .tokenizer
            .encode_batch(texts.iter().map(String::as_str).collect::<Vec<_>>(), true)
            .map_err(|e| anyhow!("tokenize: {e}"))?;
        let rows = enc.len();
        let cols = enc.iter().map(|e| e.get_ids().len()).max().unwrap_or(0);

        let mut ids = Array2::<i64>::zeros((rows, cols));
        let mut mask = Array2::<i64>::zeros((rows, cols));
        for (r, e) in enc.iter().enumerate() {
            for (c, (&id, &m)) in e.get_ids().iter().zip(e.get_attention_mask()).enumerate() {
                ids[[r, c]] = id as i64;
                mask[[r, c]] = m as i64;
            }
        }
        let types = Array2::<i64>::zeros((rows, cols));

        let mut session = self.session.lock().unwrap();
        let mut inputs = ort::inputs![
            "input_ids" => TensorRef::from_array_view(ids.view())?,
            "attention_mask" => TensorRef::from_array_view(mask.view())?,
        ];
        if self.needs_token_types {
            inputs.push((
                "token_type_ids".into(),
                TensorRef::from_array_view(types.view())?.into(),
            ));
        }
        let outputs = session.run(inputs)?;
        let hidden = outputs
            .get(self.output_name.as_str())
            .ok_or_else(|| anyhow!("missing output {}", self.output_name))?
            .try_extract_array::<f32>()?;
        // hidden: [rows, cols, dim]
        let dim = *hidden.shape().last().unwrap();

        let mut result = Vec::with_capacity(rows);
        for r in 0..rows {
            let mut v = vec![0f32; dim];
            match self.spec.pooling {
                Pooling::Cls => (0..dim).for_each(|d| v[d] = hidden[[r, 0, d]]),
                Pooling::Mean => {
                    let mut n = 0f32;
                    for c in 0..cols {
                        if mask[[r, c]] == 1 {
                            n += 1.0;
                            for d in 0..dim {
                                v[d] += hidden[[r, c, d]];
                            }
                        }
                    }
                    let n = n.max(1.0);
                    v.iter_mut().for_each(|x| *x /= n);
                }
            }
            let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-12);
            v.iter_mut().for_each(|x| *x /= norm);
            result.push(v);
        }
        Ok(result)
    }
}
