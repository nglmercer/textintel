//! Every supported language ships baseline symbol readings (§8): digits,
//! currency, math, and the core emoji set. Readings are candidates, never
//! forced substitutions.

use textintel::core::providers::SymbolKnowledgeProvider;
use textintel::engine::TextIntelligence;
use textintel::resources::ResourceLoader;

fn readings(loader: &ResourceLoader, language: &str, token: &str) -> Vec<String> {
    SymbolKnowledgeProvider::readings_in_languages(loader, token, 8, Some(&[language.to_string()]))
        .into_iter()
        .map(|item| item.text)
        .collect()
}

#[test]
fn all_supported_languages_cover_the_baseline_set() {
    let expected: &[(&str, &str, &str)] = &[
        ("ar", "🏠", "بيت"),
        ("ar", "💰", "مال"),
        ("ar", "🔥", "نار"),
        ("ar", "❤", "حب"),
        ("ar", "2", "اثنان"),
        ("ar", "$", "دولار"),
        ("hi", "🏠", "घर"),
        ("hi", "💰", "पैसा"),
        ("hi", "🔥", "आग"),
        ("hi", "❤", "प्यार"),
        ("hi", "2", "दो"),
        ("hi", "₹", "रुपया"),
        ("id", "🏠", "rumah"),
        ("id", "💰", "uang"),
        ("id", "🔥", "api"),
        ("id", "❤", "cinta"),
        ("ja", "🏠", "いえ"),
        ("ja", "💰", "おかね"),
        ("ja", "🔥", "ひ"),
        ("ja", "❤", "あい"),
        ("ja", "¥", "えん"),
        ("ko", "🏠", "집"),
        ("ko", "💰", "돈"),
        ("ko", "🔥", "불"),
        ("ko", "❤", "사랑"),
        ("ko", "100", "백"),
        ("nl", "🏠", "huis"),
        ("nl", "💰", "geld"),
        ("nl", "🔥", "vuur"),
        ("nl", "❤", "liefde"),
        ("pl", "🏠", "dom"),
        ("pl", "💰", "pieniądze"),
        ("pl", "🔥", "ogień"),
        ("pl", "❤", "miłość"),
        ("ru", "🏠", "дом"),
        ("ru", "💰", "деньги"),
        ("ru", "🔥", "огонь"),
        ("ru", "❤", "любовь"),
        ("ru", "€", "евро"),
        ("tr", "🏠", "ev"),
        ("tr", "💰", "para"),
        ("tr", "🔥", "ateş"),
        ("tr", "❤", "aşk"),
        ("tr", "%", "yüzde"),
        ("zh", "🏠", "家"),
        ("zh", "💰", "钱"),
        ("zh", "🔥", "火"),
        ("zh", "❤", "爱"),
        ("zh", "10", "十"),
        ("zh", "=", "等于"),
        // Previously covered languages keep working.
        ("en", "🏠", "house"),
        ("es", "🏠", "casa"),
        ("fr", "🏠", "maison"),
        ("de", "🏠", "haus"),
        ("it", "🏠", "casa"),
        ("pt", "🏠", "casa"),
    ];
    // One shared loader: rebuilding the embedded resources per assertion
    // dominated this test's runtime without changing its coverage.
    let loader = ResourceLoader::embedded().expect("embedded resources must load");
    for (language, token, wanted) in expected {
        let found = readings(&loader, language, token);
        assert!(
            found.iter().any(|item| item == wanted),
            "{language} {token:?}: expected {wanted:?} in {found:?}"
        );
    }
}

#[test]
fn variation_selector_sequences_stay_whole() {
    let engine = TextIntelligence::default();
    let fingerprint = engine.analyze("❤️").expect("analyze must succeed");
    let raws: Vec<&str> = fingerprint
        .symbols
        .iter()
        .map(|item| item.raw.as_str())
        .collect();
    assert!(
        raws.contains(&"❤️"),
        "❤️ must survive as one grapheme cluster, got {raws:?}"
    );
}
