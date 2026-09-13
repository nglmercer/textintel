# textintel

Multilingual text-intelligence library. Independent analysis channels produce a `MessageFingerprint`; `compare` and `decode` combine those views without treating any single channel as truth.

```python
from textintel import TextIntelligence

engine = TextIntelligence(semantic=False, phonetic=False)
fp = engine.analyze("Fra🏠do")
result = engine.compare("Fra🏠do", "fracasado")
candidates = engine.decode("salU2")
```

Core path needs no large models. Optional extras: `textintel[semantic]`, `textintel[phonetic]`, `textintel[ml]`.
