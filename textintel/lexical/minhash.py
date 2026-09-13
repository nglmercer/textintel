from __future__ import annotations

import hashlib


def simhash(tokens: list[str], bits: int = 64) -> int:
    v = [0] * bits
    for t in tokens:
        h = int(hashlib.md5(t.encode("utf-8")).hexdigest(), 16)
        for i in range(bits):
            v[i] += 1 if (h >> i) & 1 else -1
    out = 0
    for i, x in enumerate(v):
        if x > 0:
            out |= 1 << i
    return out


def minhash_sig(tokens: list[str], k: int = 32) -> tuple[int, ...]:
    if not tokens:
        return tuple([2**32 - 1] * k)
    sig = []
    for i in range(k):
        best = 2**32 - 1
        salt = str(i).encode()
        for t in tokens:
            h = int(hashlib.md5(salt + t.encode("utf-8")).hexdigest()[:8], 16)
            if h < best:
                best = h
        sig.append(best)
    return tuple(sig)
