use crate::core::providers::SymbolKnowledgeProvider;
use crate::core::types::{SymbolConcept, SymbolReading};

/// Built-in knowledge is intentionally small and transparent.  Each entry is
/// a candidate reading, not a forced replacement.
#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultSymbolKnowledge;

fn reading(text: &str, language: &str, probability: f64, kind: &str) -> SymbolReading {
    SymbolReading::new(text, Some(language), probability, kind)
}

pub fn readings_for_token(token: &str, max_readings: usize) -> Vec<SymbolReading> {
    if max_readings == 0 {
        return Vec::new();
    }
    let values: Vec<SymbolReading> = match token {
        "🏠" => vec![
            reading("casa", "es", 0.93, "symbol_reading"),
            reading("hogar", "es", 0.51, "symbol_reading"),
            reading("vivienda", "es", 0.40, "symbol_reading"),
            reading("house", "en", 0.55, "symbol_reading"),
            reading("home", "en", 0.50, "symbol_reading"),
        ],
        "🔥" => vec![
            reading("fuego", "es", 0.70, "symbol_reading"),
            reading("fire", "en", 0.70, "symbol_reading"),
            reading("hot", "en", 0.45, "symbol_reading"),
            reading("lit", "en", 0.35, "symbol_reading"),
            reading("unknown", "und", 0.20, "symbol_reading"),
        ],
        "💰" => vec![
            reading("dinero", "es", 0.90, "symbol_reading"),
            reading("plata", "es", 0.40, "symbol_reading"),
            reading("money", "en", 0.85, "symbol_reading"),
            reading("cash", "en", 0.40, "symbol_reading"),
        ],
        "❤️" | "❤" => vec![
            reading("amor", "es", 0.80, "symbol_reading"),
            reading("love", "en", 0.80, "symbol_reading"),
            reading("heart", "en", 0.50, "symbol_reading"),
        ],
        "0" => vec![reading("cero", "es", 0.80, "number_reading"), reading("zero", "en", 0.70, "number_reading"), reading("o", "und", 0.50, "number_reading")],
        "1" => vec![reading("uno", "es", 0.80, "number_reading"), reading("one", "en", 0.70, "number_reading"), reading("un", "es", 0.50, "number_reading"), reading("i", "und", 0.40, "number_reading")],
        "2" => vec![reading("dos", "es", 0.90, "number_reading"), reading("two", "en", 0.70, "number_reading"), reading("tu", "es", 0.30, "number_reading")],
        "3" => vec![reading("tres", "es", 0.85, "number_reading"), reading("three", "en", 0.70, "number_reading"), reading("e", "und", 0.35, "number_reading")],
        "4" => vec![reading("cuatro", "es", 0.70, "number_reading"), reading("four", "en", 0.60, "number_reading"), reading("a", "und", 0.55, "number_reading"), reading("for", "en", 0.40, "number_reading")],
        "5" => vec![reading("cinco", "es", 0.70, "number_reading"), reading("five", "en", 0.60, "number_reading"), reading("s", "und", 0.40, "number_reading")],
        "6" => vec![reading("seis", "es", 0.70, "number_reading"), reading("six", "en", 0.60, "number_reading")],
        "7" => vec![reading("siete", "es", 0.70, "number_reading"), reading("seven", "en", 0.60, "number_reading"), reading("t", "und", 0.30, "number_reading")],
        "8" => vec![reading("ocho", "es", 0.70, "number_reading"), reading("eight", "en", 0.60, "number_reading"), reading("ate", "en", 0.35, "number_reading")],
        "9" => vec![reading("nueve", "es", 0.70, "number_reading"), reading("nine", "en", 0.60, "number_reading")],
        "10" => vec![reading("diez", "es", 0.80, "number_reading"), reading("ten", "en", 0.70, "number_reading")],
        "100" => vec![
            reading("cien", "es", 0.85, "number_reading"),
            reading("hundred", "en", 0.70, "number_reading"),
            reading("siem", "es", 0.35, "abbreviation"),
            reading("always", "en", 0.20, "abbreviation"),
        ],
        _ => Vec::new(),
    };
    values.into_iter().take(max_readings).collect()
}

pub fn concepts_for_token(token: &str) -> Vec<SymbolConcept> {
    match token {
        "🏠" => vec![SymbolConcept { id: "house".to_string(), probability: 0.95 }],
        "🔥" => vec![
            SymbolConcept { id: "fire".to_string(), probability: 0.70 },
            SymbolConcept { id: "hotness".to_string(), probability: 0.45 },
        ],
        "💰" => vec![SymbolConcept { id: "money".to_string(), probability: 0.90 }],
        "❤️" | "❤" => vec![SymbolConcept { id: "love".to_string(), probability: 0.80 }],
        token if token.chars().all(char::is_numeric) => vec![SymbolConcept { id: "number".to_string(), probability: 0.80 }],
        _ => Vec::new(),
    }
}

impl SymbolKnowledgeProvider for DefaultSymbolKnowledge {
    fn readings(&self, token: &str, max_readings: usize) -> Vec<SymbolReading> {
        readings_for_token(token, max_readings)
    }
}

/// Small multilingual plausibility lexicon.  It is a fallback signal, not a
/// shortcut for any complete example from the specification.
pub const WORDLIST: &[&str] = &[
    "casa", "hogar", "house", "home", "fracasado", "fracasada", "fracasar", "saludo",
    "saludos", "salud", "hola", "hello", "compra", "comprar", "ahora", "now", "gana",
    "ganar", "dinero", "money", "paypal", "cash", "siempre", "siem", "cien", "pre", "bro",
    "iphone", "fuego", "fire", "amor", "love", "ferrocarril", "cansado", "camino", "gracias",
    "please", "por", "para", "the", "and", "el", "la", "de", "que", "vivienda", "plata",
    "dos", "uno", "tres", "four", "two", "one", "lit", "hot", "unknown", "ok", "lol", "buy",
    "prize", "premio", "reclama", "tu", "siempre",
];

