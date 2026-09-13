"""MVP tests driving shipped analyze / compare / decode (plan.md §35 / §42)."""

from textintel import TextIntelligence
from textintel.core.config import SimilarityWeights

CYRILLIC_PAYPAL = "pаypal"  # visually paypal; а is Cyrillic U+0430


def engine():
    return TextIntelligence(semantic=False, phonetic=False)


def test_raw_preserved():
    e = engine()
    x = "Fr4🏠do!!!"
    assert e.analyze(x).raw == x


def test_case_spacing_high_similarity():
    r = engine().compare("COMPRA AHORA", "compra ahora")
    assert 0.0 <= r.score <= 1.0
    assert r.character is not None and r.character > 0.7
    assert r.semantic is None
    assert r.phonetic is None


def test_leetspeak_obfuscation_and_similarity():
    e = engine()
    fp = e.analyze("c0mpr4 ah0r4")
    assert fp.obfuscation_features.leetspeak
    r = e.compare("c0mpr4 ah0r4", "compra ahora")
    assert r.decoded_similarity is not None
    assert r.score > 0.5


def test_emoji_rebus_fracasado_among_candidates():
    e = engine()
    cands = e.decode("Fra🏠do")
    texts = [c.text.casefold() for c in cands]
    assert any("fracasado" in t or t == "fracasado" for t in texts)
    r = e.compare("Fra🏠do", "fracasado")
    assert r.decoded_similarity is not None
    assert r.score > 0.45


def test_numeric_rebus_saludos():
    cands = engine().decode("salU2")
    texts = [c.text.casefold() for c in cands]
    assert any("saludos" in t or t == "saludos" for t in texts)


def test_homoglyph_paypal():
    e = engine()
    fp = e.analyze(CYRILLIC_PAYPAL)
    assert fp.unicode_features.mixed_scripts or fp.unicode_features.confusable_characters
    assert fp.obfuscation_features.confusables or fp.obfuscation_features.mixed_scripts
    r = e.compare("paypal", CYRILLIC_PAYPAL)
    assert r.visual is not None and r.visual > 0.7


def test_repetition_gana_dinero():
    e = engine()
    fp = e.analyze("GAAAAANAAAA DINEROOOO")
    assert fp.obfuscation_features.repetition
    r = e.compare("GAAAAANAAAA DINEROOOO", "gana dinero")
    assert r.score > 0.4


def test_code_switching_not_one_language():
    fp = engine().analyze("bro compra NOW")
    langs = {c.language for c in fp.language_candidates}
    types = {s.segment_type for s in fp.segments}
    assert "unknown" in langs or len(langs) >= 2 or any(
        len(s.language_candidates) > 1 for s in fp.segments if s.segment_type == "text"
    )
    r = engine().compare("bro compra NOW", "bro compra ahora")
    assert r.score > 0.3
    assert types  # segmented


def test_negative_fracasado_vs_ferrocarril():
    e = engine()
    pos = e.compare("Fra🏠do", "fracasado")
    neg = e.compare("Fra🏠do", "ferrocarril")
    assert pos.score > neg.score + 0.12
    assert neg.score < 0.7


def test_ambiguous_fire_emoji_multiple_readings():
    fp = engine().analyze("🔥")
    readings = []
    for s in fp.symbols:
        readings.extend(r[0] for r in s.readings)
    cands = engine().decode("🔥")
    surfaces = {c.text.casefold() for c in cands}
    # multiple concepts or unknown — never a single forced meaning
    assert len(readings) > 1 or "unknown" in readings or len(surfaces) > 1


def test_weights_from_config_not_only_literals():
    w = SimilarityWeights(
        character=1.0,
        lexical=0.0,
        visual=0.0,
        decoded=0.0,
        obfuscation=0.0,
        semantic=0.0,
        phonetic=0.0,
    )
    e = TextIntelligence(semantic=False, phonetic=False, similarity_weights=w)
    r = e.compare("abc", "abd")
    assert "character" in r.weights_used
    assert abs(r.weights_used["character"] - 1.0) < 1e-9
    assert r.score == r.character


def test_no_hardcoded_full_string_decode_in_library():
    import pathlib

    root = pathlib.Path(__file__).resolve().parents[1] / "textintel"
    forbidden = ("Fra🏠do", "fracasado", "salU2", "saludos")
    hits = []
    for path in root.rglob("*.py"):
        src = path.read_text(encoding="utf-8")
        # knowledge wordlist may include lexical items; ban control-flow shortcuts
        if path.name == "knowledge.py":
            continue
        for needle in forbidden:
            if needle in src:
                hits.append(f"{path}: {needle}")
    assert hits == []
