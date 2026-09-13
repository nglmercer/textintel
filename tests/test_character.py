from textintel.lexical.character import character_similarity, levenshtein


def test_identical_is_one():
    s = character_similarity("compra", "compra")
    assert s.combined == 1.0
    assert s.levenshtein == 1.0


def test_leet_near_compra():
    s = character_similarity("comprar", "c0mpr4r")
    assert s.combined > 0.4
    assert 0 <= s.jaro_winkler <= 1


def test_levenshtein_distance():
    assert levenshtein("kitten", "sitting") == 3
    assert levenshtein("", "ab") == 2
