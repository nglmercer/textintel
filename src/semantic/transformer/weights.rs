//! BERT checkpoint weights (safetensors) plus strict shape checks.

use std::collections::HashMap;

use candle_core::{DType, Tensor};

use crate::core::error::ProviderError;

use super::{CONFIG_FILE, EncoderConfig, WEIGHTS_FILE, invalid};

fn debug_shapes(tensors: &[&Tensor]) -> Vec<Vec<usize>> {
    tensors
        .iter()
        .map(|tensor| tensor.dims().to_vec())
        .collect()
}

/// One encoder layer's validated tensors (all F32, CPU).
pub(crate) struct LayerWeights {
    pub(crate) query_w: Tensor,
    pub(crate) query_b: Tensor,
    pub(crate) key_w: Tensor,
    pub(crate) key_b: Tensor,
    pub(crate) value_w: Tensor,
    pub(crate) value_b: Tensor,
    pub(crate) attn_out_w: Tensor,
    pub(crate) attn_out_b: Tensor,
    pub(crate) attn_ln_w: Tensor,
    pub(crate) attn_ln_b: Tensor,
    pub(crate) inter_w: Tensor,
    pub(crate) inter_b: Tensor,
    pub(crate) out_w: Tensor,
    pub(crate) out_b: Tensor,
    pub(crate) out_ln_w: Tensor,
    pub(crate) out_ln_b: Tensor,
}

pub(crate) struct BertWeights {
    pub(crate) word_emb: Tensor,
    pub(crate) pos_emb: Tensor,
    pub(crate) token_type_emb: Tensor,
    pub(crate) emb_ln_w: Tensor,
    pub(crate) emb_ln_b: Tensor,
    pub(crate) layers: Vec<LayerWeights>,
}

impl std::fmt::Debug for LayerWeights {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LayerWeights")
            .field(
                "shapes",
                &debug_shapes(&[
                    &self.query_w,
                    &self.key_w,
                    &self.value_w,
                    &self.attn_out_w,
                    &self.inter_w,
                    &self.out_w,
                ]),
            )
            .finish()
    }
}

impl std::fmt::Debug for BertWeights {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BertWeights")
            .field(
                "embeddings",
                &debug_shapes(&[&self.word_emb, &self.pos_emb, &self.token_type_emb]),
            )
            .field("layers", &self.layers.len())
            .finish()
    }
}

fn take_f32(
    tensors: &mut HashMap<String, Tensor>,
    name: &str,
    shape: &[usize],
) -> Result<Tensor, ProviderError> {
    // Real Hugging Face checkpoints nest BERT tensors under `bert.`; the
    // mechanics fixture stores them bare. Accept both layouts.
    let prefixed = format!("bert.{name}");
    let tensor = tensors
        .remove(&prefixed)
        .or_else(|| tensors.remove(name))
        .ok_or_else(|| {
            invalid(format!(
                "{WEIGHTS_FILE}: missing tensor `{prefixed}` (nor bare `{name}`): \
                 not a BERT-wiring checkpoint?"
            ))
        })?;
    let tensor = match tensor.dtype() {
        DType::F32 => tensor,
        // Modern checkpoints may ship half precision (mxbai-embed-xsmall is
        // F16); CPU inference runs in F32, so cast once at load.
        DType::F16 | DType::BF16 => tensor.to_dtype(DType::F32).map_err(|error| {
            invalid(format!(
                "{WEIGHTS_FILE}: tensor `{name}` half-precision cast failed: {error}"
            ))
        })?,
        other => {
            return Err(invalid(format!(
                "{WEIGHTS_FILE}: tensor `{name}` has dtype {other:?}, \
                 only F32/F16/BF16 are supported"
            )));
        }
    };
    if tensor.dims() != shape {
        return Err(invalid(format!(
            "{WEIGHTS_FILE}: tensor `{name}` has shape {:?}, expected {shape:?} \
             (checkpoint does not match {CONFIG_FILE})",
            tensor.dims()
        )));
    }
    Ok(tensor)
}

pub(crate) fn load_weights(
    tensors: &mut HashMap<String, Tensor>,
    config: &EncoderConfig,
) -> Result<BertWeights, ProviderError> {
    let hidden = config.hidden_size;
    let intermediate = config.intermediate_size;
    let vocab = config.vocab_size;
    let positions = config.max_positions;
    let vec = |size: usize| vec![size];
    let mat = |rows: usize, cols: usize| vec![rows, cols];
    let word_emb = take_f32(
        tensors,
        "embeddings.word_embeddings.weight",
        &mat(vocab, hidden),
    )?;
    let pos_emb = take_f32(
        tensors,
        "embeddings.position_embeddings.weight",
        &mat(positions, hidden),
    )?;
    let token_type_emb = take_f32(
        tensors,
        "embeddings.token_type_embeddings.weight",
        &mat(2, hidden),
    )?;
    let emb_ln_w = take_f32(tensors, "embeddings.LayerNorm.weight", &vec(hidden))?;
    let emb_ln_b = take_f32(tensors, "embeddings.LayerNorm.bias", &vec(hidden))?;
    let mut layers = Vec::with_capacity(config.num_layers);
    for layer in 0..config.num_layers {
        let base = format!("encoder.layer.{layer}");
        let attention = format!("{base}.attention.self");
        let mut linear = |name: &str, shape: &[usize]| take_f32(tensors, name, shape);
        layers.push(LayerWeights {
            query_w: linear(&format!("{attention}.query.weight"), &mat(hidden, hidden))?,
            query_b: linear(&format!("{attention}.query.bias"), &vec(hidden))?,
            key_w: linear(&format!("{attention}.key.weight"), &mat(hidden, hidden))?,
            key_b: linear(&format!("{attention}.key.bias"), &vec(hidden))?,
            value_w: linear(&format!("{attention}.value.weight"), &mat(hidden, hidden))?,
            value_b: linear(&format!("{attention}.value.bias"), &vec(hidden))?,
            attn_out_w: linear(
                &format!("{base}.attention.output.dense.weight"),
                &mat(hidden, hidden),
            )?,
            attn_out_b: linear(&format!("{base}.attention.output.dense.bias"), &vec(hidden))?,
            attn_ln_w: linear(
                &format!("{base}.attention.output.LayerNorm.weight"),
                &vec(hidden),
            )?,
            attn_ln_b: linear(
                &format!("{base}.attention.output.LayerNorm.bias"),
                &vec(hidden),
            )?,
            inter_w: linear(
                &format!("{base}.intermediate.dense.weight"),
                &mat(intermediate, hidden),
            )?,
            inter_b: linear(
                &format!("{base}.intermediate.dense.bias"),
                &vec(intermediate),
            )?,
            out_w: linear(
                &format!("{base}.output.dense.weight"),
                &mat(hidden, intermediate),
            )?,
            out_b: linear(&format!("{base}.output.dense.bias"), &vec(hidden))?,
            out_ln_w: linear(&format!("{base}.output.LayerNorm.weight"), &vec(hidden))?,
            out_ln_b: linear(&format!("{base}.output.LayerNorm.bias"), &vec(hidden))?,
        });
    }
    Ok(BertWeights {
        word_emb,
        pos_emb,
        token_type_emb,
        emb_ln_w,
        emb_ln_b,
        layers,
    })
}
