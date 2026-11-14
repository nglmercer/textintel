//! Prepare text once and split it for an application's TTS provider.
use textintel::speech::{normalization as tn, segmentation};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let text = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "El 26/10/2026 pagué S/ 21.50 a las 15:30.".to_owned());
    let options = tn::NormalizeOptions::resolve_with_style(
        tn::NormalizationMode::Auto,
        "es",
        Some("es-PE"),
        tn::NormalizeLimits::default(),
        true,
        tn::ReadingStyle::Precise,
    )?;
    let prepared = tn::normalize(&text, &options)?;
    println!("{}", serde_json::to_string_pretty(&prepared)?);
    let fragments = segmentation::segment_text(&prepared.spoken, &Default::default());
    println!("{}", serde_json::to_string_pretty(&fragments)?);
    Ok(())
}
