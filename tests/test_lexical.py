from textintel.lexical.similarity import lexical_similarity, jaccard
from textintel.lexical.tokenizer import tokenize


def test_tokenize_words():
    t = tokenize("bro compra NOW")
    assert "bro" in t and "compra" in t and "NOW" in t


def test_jaccard_identical():
    assert jaccard({"a", "b"}, {"a", "b"}) == 1.0
    assert jaccard(set(), set()) == 1.0


def test_lexical_overlap():
    assert lexical_similarity("gana dinero", "gana dinero") == 1.0
    assert lexical_similarity("gana dinero", "xyz") < 0.2
