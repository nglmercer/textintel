//! BERT encoder forward pass (attention + feed-forward) on Candle tensors.

use candle_core::Tensor;

use super::EncoderConfig;
use super::weights::LayerWeights;

const TANH_GELU_COEF: f64 = 0.7978845608; // sqrt(2/pi)
const TANH_GELU_CUBIC: f64 = 0.044715;

/// `x @ w + b` with `w: [in, out]` pre-transposed at load.
fn linear(x: &Tensor, weight: &Tensor, bias: &Tensor) -> Result<Tensor, candle_core::Error> {
    x.matmul(weight)?.broadcast_add(bias)
}

pub(crate) fn layer_norm(
    x: &Tensor,
    weight: &Tensor,
    bias: &Tensor,
    eps: f64,
    dim: usize,
) -> Result<Tensor, candle_core::Error> {
    let mean = x.mean_keepdim(dim)?;
    let centered = x.broadcast_sub(&mean)?;
    let variance = centered.sqr()?.mean_keepdim(dim)?;
    let normalized = centered.broadcast_div(&variance.affine(1.0, eps)?.sqrt()?)?;
    normalized.broadcast_mul(weight)?.broadcast_add(bias)
}

/// GELU with the standard tanh approximation.
fn gelu_tanh(x: &Tensor) -> Result<Tensor, candle_core::Error> {
    let cubed = x.sqr()?.mul(x)?;
    let inner = cubed
        .affine(TANH_GELU_CUBIC, 0.0)?
        .add(x)?
        .affine(TANH_GELU_COEF, 0.0)?;
    inner.tanh()?.affine(1.0, 1.0)?.mul(x)?.affine(0.5, 0.0)
}

fn softmax_last_dim(scores: &Tensor) -> Result<Tensor, candle_core::Error> {
    let last = scores.rank().checked_sub(1).ok_or(candle_core::Error::Msg(
        "softmax needs at least one dimension".to_string(),
    ))?;
    let shifted = scores.broadcast_sub(&scores.max_keepdim(last)?)?;
    let exp = shifted.exp()?;
    exp.broadcast_div(&exp.sum_keepdim(last)?)
}

fn attention(
    hidden: &Tensor,
    mask_add: &Tensor,
    layer: &LayerWeights,
    config: &EncoderConfig,
) -> Result<Tensor, candle_core::Error> {
    let tokens = hidden.dim(0)?;
    let heads = config.num_heads;
    let head_dim = config.hidden_size / heads;
    let split = |projected: Tensor| {
        projected
            .reshape((tokens, heads, head_dim))?
            .transpose(0, 1)?
            .contiguous()
    };
    let query = split(linear(hidden, &layer.query_w, &layer.query_b)?)?;
    let key = split(linear(hidden, &layer.key_w, &layer.key_b)?)?;
    let value = split(linear(hidden, &layer.value_w, &layer.value_b)?)?;
    let scale = 1.0 / (head_dim as f64).sqrt();
    let scores = query
        .matmul(&key.transpose(1, 2)?.contiguous()?)?
        .affine(scale, 0.0)?
        .broadcast_add(mask_add)?;
    let context = softmax_last_dim(&scores)?
        .matmul(&value)?
        .transpose(0, 1)?
        .contiguous()?
        .reshape((tokens, config.hidden_size))?;
    let projected = linear(&context, &layer.attn_out_w, &layer.attn_out_b)?;
    layer_norm(
        &projected.add(hidden)?,
        &layer.attn_ln_w,
        &layer.attn_ln_b,
        config.layer_norm_eps,
        1,
    )
}

pub(crate) fn encoder_layer(
    hidden: &Tensor,
    mask_add: &Tensor,
    layer: &LayerWeights,
    config: &EncoderConfig,
) -> Result<Tensor, candle_core::Error> {
    let attended = attention(hidden, mask_add, layer, config)?;
    let activated = gelu_tanh(&linear(&attended, &layer.inter_w, &layer.inter_b)?)?;
    let projected = linear(&activated, &layer.out_w, &layer.out_b)?;
    layer_norm(
        &projected.add(&attended)?,
        &layer.out_ln_w,
        &layer.out_ln_b,
        config.layer_norm_eps,
        1,
    )
}

#[cfg(test)]
mod tests {
    use candle_core::Device;

    use super::*;

    fn tensor(rows: &[Vec<f32>]) -> Tensor {
        let flat: Vec<f32> = rows.iter().flatten().copied().collect();
        Tensor::new(flat, &Device::Cpu)
            .unwrap()
            .reshape((rows.len(), rows[0].len()))
            .unwrap()
    }

    #[test]
    fn softmax_rows_sum_to_one() {
        let scores = tensor(&[vec![1.0, 2.0, 3.0], vec![-1000.0, 0.0, 1000.0]]);
        let probs = softmax_last_dim(&scores).unwrap().to_vec2::<f32>().unwrap();
        for row in &probs {
            let sum: f32 = row.iter().sum();
            assert!((sum - 1.0).abs() < 1e-5, "row sums to {sum}");
            assert!(row.iter().all(|value| (0.0..=1.0).contains(value)));
        }
        assert!(probs[0][2] > probs[0][1] && probs[0][1] > probs[0][0]);
        assert!(probs[1][2] > 0.999);
    }

    #[test]
    fn layer_norm_centers_scales_and_shifts() {
        let values = tensor(&[vec![1.0, 2.0, 3.0, 4.0]]);
        let weight = Tensor::new(vec![2.0f32; 4], &Device::Cpu).unwrap();
        let bias = Tensor::new(vec![0.5f32; 4], &Device::Cpu).unwrap();
        let normed = layer_norm(&values, &weight, &bias, 1e-12, 1)
            .unwrap()
            .to_vec2::<f32>()
            .unwrap();
        let row = &normed[0];
        let mean = row.iter().sum::<f32>() / row.len() as f32;
        assert!((mean - 0.5).abs() < 1e-4, "bias shift, mean={mean}");
        // Unit variance scaled by weight 2: values are +/- fixed multiples.
        assert!((row[0] + 2.1836).abs() < 1e-3, "unexpected {row:?}");
    }

    #[test]
    fn gelu_matches_tanh_reference() {
        let values = Tensor::new(vec![0.0f32, 1.0, -1.0], &Device::Cpu).unwrap();
        let out = gelu_tanh(&values).unwrap().to_vec1::<f32>().unwrap();
        assert!(out[0].abs() < 1e-6);
        assert!((out[1] - 0.8412).abs() < 1e-3, "gelu(1)={}", out[1]);
        assert!((out[2] + 0.1588).abs() < 1e-3, "gelu(-1)={}", out[2]);
    }
}
