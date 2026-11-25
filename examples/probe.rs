use textintel::{EngineConfig, TextIntelligence};
fn main() {
    let engine = TextIntelligence::new(EngineConfig::default());
    for (a, b) in [
        ("hello", "hello"),
        ("spasibo", "spasibo"),
        ("spasibo", "спасибо"),
        ("hello", "おはう"),
        ("meeting at noon tomorrow", "はおよう"),
        ("music", "今すぐ購入"),
    ] {
        let r = engine.compare(a, b).unwrap();
        println!("{a:?} vs {b:?}: score={:.4} ni={} decoded={:?} exact={:.3} translit_sim={:?} conf={:?} phon={:?} lexval={:.2} cross_script={}",
            r.score, r.normalized_identity, r.decoded_similarity, r.exact_decode,
            r.transliteration_similarity, r.transliteration_confidence, r.phonetic, r.lexicon_validity, r.cross_script_pair);
        println!("    evidence: {}", r.evidence.join(" "));
    }
}
