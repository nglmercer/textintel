//! Evaluate a generative model (OpenAI-compatible chat endpoint) on a
//! shared choice-question eval set.
//!
//! Arguments are declared in [`textintel::cli::eval_llm_spec`] (run
//! with `--help` for usage).
//!
//! Protocol v4: each example becomes one zero-shot numbered
//! multiple-choice prompt, temperature 0, seed 7. The model replies with
//! exactly one option number (v1 letters and v2/v3 labels both collapsed:
//! label words carry priors that override these tiny models' judgment,
//! while numbers stay neutral — see report). The parser takes the first
//! digit in range, else records `unparsed` (counts as wrong). Every raw
//! reply is kept in the output for audit.

use std::collections::BTreeMap;
use std::time::Instant;

use textintel::decision::{DecisionDataset, DecisionExample, DecisionQuestion};

const PROMPT_VERSION: &str = "choice-numbers-zeroshot-v4";

fn letters(count: usize) -> Vec<char> {
    (0..count)
        .map(|index| (b'A' + index as u8) as char)
        .collect()
}

fn render_options(question: &DecisionQuestion) -> Option<Vec<(char, String, String)>> {
    let DecisionQuestion::Choice { criteria, .. } = question else {
        return None;
    };
    let mut ids: Vec<&String> = criteria.keys().collect();
    ids.sort_unstable();
    Some(
        letters(ids.len())
            .into_iter()
            .zip(ids)
            .map(|(letter, id)| (letter, id.clone(), criteria[id].clone()))
            .collect(),
    )
}

fn render_prompt(example: &DecisionExample) -> Option<(String, Vec<String>)> {
    let DecisionQuestion::Choice { instructions, .. } = &example.question else {
        return None;
    };
    let options = render_options(&example.question)?;
    let ids: Vec<String> = options.iter().map(|(_, id, _)| id.clone()).collect();
    let mut prompt = format!("{instructions}\n");
    for (number, (_, id, description)) in options.iter().enumerate() {
        prompt.push_str(&format!("{}. {id}: {description}\n", number + 1));
    }
    prompt.push_str(&format!(
        "Message: {}\nReply with exactly one number (1-{}) and nothing else.\nAnswer:",
        example.state,
        ids.len()
    ));
    Some((prompt, ids))
}

fn parse_answer(raw: &str, ids: &[String]) -> Option<String> {
    // First digit in range 1..=K.
    for character in raw.chars() {
        if let Some(digit) = character.to_digit(10) {
            let number = digit as usize;
            if (1..=ids.len()).contains(&number) {
                return Some(ids[number - 1].clone());
            }
        }
    }
    None
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let spec = textintel::cli::eval_llm_spec();
    if let Some(text) = textintel::cli::handle_meta(spec, env!("CARGO_PKG_VERSION"), &argv) {
        println!("{text}");
        return Ok(());
    }
    let parsed = match textintel::cli::parse_args(spec, &argv) {
        Ok(parsed) => parsed,
        Err(error) => {
            eprintln!("{error}");
            eprintln!("Run `{} --help` for usage.", spec.name);
            std::process::exit(error.exit_code());
        }
    };
    let eval = parsed.required_value("eval")?;
    let endpoint = parsed.required_value("endpoint")?;
    let model = parsed.required_value("model")?;
    let out = parsed.required_value("out")?;

    let dataset = DecisionDataset::load_path(eval).map_err(|error| error.to_string())?;
    let examples = dataset.test.clone();
    if examples.is_empty() {
        return Err("eval set has no examples".into());
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(120)))
        .build()
        .into();
    let url = format!("{}/v1/chat/completions", endpoint.trim_end_matches('/'));

    let mut results = Vec::new();
    let mut correct = 0usize;
    let mut parsed = 0usize;
    let mut total_micros = 0u128;
    for (position, example) in examples.iter().enumerate() {
        let Some((prompt, ids)) = render_prompt(example) else {
            eprintln!("example {}: skipped (not a choice question)", example.id);
            continue;
        };
        let started = Instant::now();
        let mut response = agent
            .post(&url)
            .header("Content-Type", "application/json")
            .send_json(serde_json::json!({
                "model": model,
                "messages": [{"role": "user", "content": prompt}],
                "max_tokens": 16,
                "temperature": 0.0,
                "seed": 7,
            }))
            .map_err(|error| format!("example {}: request failed: {error}", example.id))?;
        total_micros += started.elapsed().as_micros();
        let body: serde_json::Value = response
            .body_mut()
            .read_json()
            .map_err(|error| format!("example {}: bad response: {error}", example.id))?;
        let raw = body["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let predicted = parse_answer(&raw, &ids);
        if predicted.is_some() {
            parsed += 1;
        }
        let hit = predicted.as_deref() == Some(example.gold.as_str());
        if hit {
            correct += 1;
        }
        println!(
            "[{}/{}] {} gold={} predicted={:?} {}",
            position + 1,
            examples.len(),
            example.id,
            example.gold,
            predicted,
            if hit { "OK" } else { "MISS" }
        );
        results.push(serde_json::json!({
            "id": example.id,
            "task": example.task,
            "gold": example.gold,
            "predicted": predicted,
            "correct": hit,
            "raw": raw,
        }));
    }
    let scored = results.len();
    let per_task: BTreeMap<String, (usize, usize)> =
        results.iter().fold(BTreeMap::new(), |mut map, entry| {
            let task = entry["task"].as_str().unwrap_or("?").to_string();
            let slot = map.entry(task).or_insert((0, 0));
            slot.1 += 1;
            if entry["correct"].as_bool().unwrap_or(false) {
                slot.0 += 1;
            }
            map
        });
    let report = serde_json::json!({
        "eval_version": dataset.version,
        "prompt_version": PROMPT_VERSION,
        "model": model,
        "endpoint": endpoint,
        "scored": scored,
        "correct": correct,
        "accuracy": correct as f64 / scored.max(1) as f64,
        "parsed": parsed,
        "mean_latency_micros": total_micros as f64 / scored.max(1) as f64,
        "per_task": per_task.iter().map(|(task, (hit, total))| {
            (task.clone(), serde_json::json!({"correct": hit, "total": total}))
        }).collect::<BTreeMap<_, _>>(),
        "results": results,
    });
    std::fs::write(out, serde_json::to_string_pretty(&report)?)?;
    println!(
        "accuracy={:.3} ({correct}/{scored}) parsed={parsed}/{scored} wrote {out}",
        correct as f64 / scored.max(1) as f64
    );
    Ok(())
}
