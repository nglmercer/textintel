from __future__ import annotations

from collections import Counter

from textintel.core.types import CharacterFeatures, CharacterSimilarity


def char_features(text: str) -> CharacterFeatures:
    letters = digits = ws = punct = other = 0
    for ch in text:
        if ch.isalpha():
            letters += 1
        elif ch.isdigit():
            digits += 1
        elif ch.isspace():
            ws += 1
        elif not ch.isalnum():
            punct += 1
        else:
            other += 1
    return CharacterFeatures(
        length=len(text),
        letters=letters,
        digits=digits,
        whitespace=ws,
        punctuation=punct,
        other=other,
        ngrams_2=_ngram_counts(text.casefold(), 2),
        ngrams_3=_ngram_counts(text.casefold(), 3),
    )


def _ngram_counts(text: str, n: int) -> dict[str, int]:
    if len(text) < n:
        return {}
    c: Counter[str] = Counter(text[i : i + n] for i in range(len(text) - n + 1))
    return dict(c)


def levenshtein(a: str, b: str) -> int:
    if a == b:
        return 0
    if not a:
        return len(b)
    if not b:
        return len(a)
    prev = list(range(len(b) + 1))
    for i, ca in enumerate(a, 1):
        cur = [i]
        for j, cb in enumerate(b, 1):
            ins = cur[j - 1] + 1
            delete = prev[j] + 1
            sub = prev[j - 1] + (ca != cb)
            cur.append(min(ins, delete, sub))
        prev = cur
    return prev[-1]


def damerau_levenshtein(a: str, b: str) -> int:
    if a == b:
        return 0
    la, lb = len(a), len(b)
    da = [[0] * (lb + 1) for _ in range(la + 1)]
    for i in range(la + 1):
        da[i][0] = i
    for j in range(lb + 1):
        da[0][j] = j
    for i in range(1, la + 1):
        for j in range(1, lb + 1):
            cost = 0 if a[i - 1] == b[j - 1] else 1
            da[i][j] = min(
                da[i - 1][j] + 1,
                da[i][j - 1] + 1,
                da[i - 1][j - 1] + cost,
            )
            if i > 1 and j > 1 and a[i - 1] == b[j - 2] and a[i - 2] == b[j - 1]:
                da[i][j] = min(da[i][j], da[i - 2][j - 2] + 1)
    return da[la][lb]


def _norm_edit(dist: int, a: str, b: str) -> float:
    m = max(len(a), len(b), 1)
    return 1.0 - dist / m


def jaro(a: str, b: str) -> float:
    if a == b:
        return 1.0
    if not a or not b:
        return 0.0
    la, lb = len(a), len(b)
    match_dist = max(la, lb) // 2 - 1
    if match_dist < 0:
        match_dist = 0
    a_match = [False] * la
    b_match = [False] * lb
    matches = 0
    for i in range(la):
        start = max(0, i - match_dist)
        end = min(i + match_dist + 1, lb)
        for j in range(start, end):
            if b_match[j] or a[i] != b[j]:
                continue
            a_match[i] = b_match[j] = True
            matches += 1
            break
    if not matches:
        return 0.0
    k = 0
    trans = 0
    for i in range(la):
        if not a_match[i]:
            continue
        while not b_match[k]:
            k += 1
        if a[i] != b[k]:
            trans += 1
        k += 1
    trans /= 2
    return (
        matches / la + matches / lb + (matches - trans) / matches
    ) / 3.0


def jaro_winkler(a: str, b: str, p: float = 0.1) -> float:
    j = jaro(a, b)
    prefix = 0
    for ca, cb in zip(a, b):
        if ca != cb or prefix == 4:
            break
        prefix += 1
    return j + prefix * p * (1 - j)


def ngram_similarity(a: str, b: str, n: int = 2) -> float:
    if not a and not b:
        return 1.0
    ca = Counter(_ngram_counts(a, n))
    cb = Counter(_ngram_counts(b, n))
    if not ca and not cb:
        return 1.0 if a == b else 0.0
    inter = sum((ca & cb).values())
    union = sum((ca | cb).values())
    return inter / union if union else 0.0


def lcs_len(a: str, b: str) -> int:
    la, lb = len(a), len(b)
    if la == 0 or lb == 0:
        return 0
    prev = [0] * (lb + 1)
    for ca in a:
        cur = [0]
        for j, cb in enumerate(b, 1):
            if ca == cb:
                cur.append(prev[j - 1] + 1)
            else:
                cur.append(max(prev[j], cur[-1]))
        prev = cur
    return prev[-1]


def lcs_similarity(a: str, b: str) -> float:
    m = max(len(a), len(b), 1)
    return lcs_len(a, b) / m


def character_similarity(a: str, b: str) -> CharacterSimilarity:
    aa, bb = a.casefold(), b.casefold()
    lev = _norm_edit(levenshtein(aa, bb), aa, bb)
    dam = _norm_edit(damerau_levenshtein(aa, bb), aa, bb)
    ja = jaro(aa, bb)
    jw = jaro_winkler(aa, bb)
    ng = 0.5 * ngram_similarity(aa, bb, 2) + 0.5 * ngram_similarity(aa, bb, 3)
    lc = lcs_similarity(aa, bb)
    combined = 0.2 * lev + 0.15 * dam + 0.15 * ja + 0.2 * jw + 0.15 * ng + 0.15 * lc
    return CharacterSimilarity(
        levenshtein=lev,
        damerau_levenshtein=dam,
        jaro=ja,
        jaro_winkler=jw,
        ngram_similarity=ng,
        lcs=lc,
        combined=combined,
    )


def combined_character_similarity(a: str, b: str) -> float:
    return character_similarity(a, b).combined
