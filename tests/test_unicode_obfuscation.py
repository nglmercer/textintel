from textintel.normalization.leetspeak import detect_leet
from textintel.obfuscation.features import obfuscation_features
from textintel.visual.unicode_features import analyze_unicode


def test_mixed_script_paypal():
    latin = "paypal"
    mixed = "pаypal"  # Cyrillic а
    assert latin != mixed
    u = analyze_unicode(mixed)
    assert u.mixed_scripts or u.confusable_characters
    assert u.confusable_skeleton is not None
    assert "a" in u.confusable_skeleton


def test_leet_detected():
    assert detect_leet("c0mpr4 ah0r4")
    ob = obfuscation_features("c0mpr4 ah0r4", analyze_unicode("c0mpr4 ah0r4"))
    assert ob.leetspeak
    assert ob.detected


def test_repetition_flag():
    u = analyze_unicode("GAAAAANAAAA DINEROOOO")
    ob = obfuscation_features("GAAAAANAAAA DINEROOOO", u)
    assert ob.repetition
