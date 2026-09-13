# Multilingual Text Intelligence Library

## 1. Project Goal

Build a reusable multilingual text intelligence library capable of analyzing arbitrary user-generated messages and producing multiple independent representations of the same input.

The library must support:

* semantic similarity
* lexical similarity
* character similarity
* visual similarity
* phonetic similarity
* multilingual text
* mixed-language text
* emojis
* Unicode symbols
* leetspeak
* homoglyphs
* deliberate obfuscation
* rebus-style writing
* spam detection signals
* duplicate detection
* pattern detection

Examples the system should eventually understand:

```text
Fra🏠do
→ fracasado
```

```text
salU2
→ saludos
```

```text
g4n4 💰
→ gana dinero
```

```text
c0mpr4 ah0r4
→ compra ahora
```

```text
pаypal
→ paypal
```

Where the visually identical `а` may belong to another Unicode alphabet.

The system MUST NOT assume that every symbol has one fixed textual meaning.

The system MUST preserve uncertainty and generate multiple candidate interpretations when necessary.

---

# 2. Core Principle

Do NOT build one monolithic AI model.

Build a modular pipeline where every analysis channel produces independent evidence.

Conceptually:

```text
Input Message
      │
      ▼
Preprocessing
      │
      ├───────────────┐
      │               │
      ▼               ▼
Language          Unicode /
Detection         Visual Analysis
      │               │
      ├───────────────┤
      │               │
      ▼               ▼
Lexical           Symbol /
Analysis          Emoji Analysis
      │               │
      ├───────────────┤
      │               │
      ▼               ▼
Semantic          Rebus Decoder
Analysis              │
      │               ▼
      │           Spoken Candidates
      │               │
      │               ▼
      │           Phonetic Analysis
      │               │
      └───────┬───────┘
              ▼
       MessageFingerprint
              │
              ▼
        Comparison Engine
              │
              ▼
 Classification / Search /
 Spam / Similarity / Patterns
```

No individual channel should be considered absolute truth.

---

# 3. Main Abstraction

The central abstraction MUST be `MessageFingerprint`, not just `TextEmbedding`.

Example:

```python
@dataclass
class MessageFingerprint:
    raw: str

    normalized: str | None

    language_candidates: list["LanguageCandidate"]

    segments: list["MessageSegment"]

    tokens: list[str]

    lemmas: list[str]

    char_features: "CharacterFeatures"

    unicode_features: "UnicodeFeatures"

    symbols: list["SymbolInstance"]

    lexical_features: "LexicalFeatures"

    semantic_embeddings: dict[str, list[float]]

    spoken_candidates: list["SpokenCandidate"]

    phonetic_candidates: list["PhoneticCandidate"]

    rebus_candidates: list["DecodedCandidate"]

    obfuscation_features: "ObfuscationFeatures"

    metadata: dict
```

The exact implementation can evolve, but the architecture must preserve independent representations.

---

# 4. Non-Destructive Processing

Never destroy the original input.

Every transformation must preserve:

```python
fingerprint.raw
```

For example:

```text
Input:
Fr4🏠do!!!
```

Possible representations:

```text
raw:
Fr4🏠do!!!

unicode_normalized:
Fr4🏠do!!!

casefolded:
fr4🏠do!!!

visual_normalized:
fra🏠do

symbol_expanded_candidate:
fracasado

phonetic_candidate:
/fɾakasado/
```

Do NOT replace the original representation globally.

Transformations are additional views of the same message.

---

# 5. Language Detection

The system MUST support:

* one language
* unknown language
* multiple languages inside one message
* code switching
* slang
* transliterated language
* uncertain language

Do not force an entire message to one language.

Example:

```text
bro compra el iPhone NOW 🔥
```

Possible segmentation:

```text
bro        → en / slang / uncertain
compra el  → es
iPhone     → named entity
NOW        → en
🔥         → symbol
```

Recommended representation:

```python
@dataclass
class LanguageCandidate:
    language: str
    probability: float


@dataclass
class MessageSegment:
    text: str
    start: int
    end: int
    language_candidates: list[LanguageCandidate]
    segment_type: str
```

Possible `segment_type` values:

```text
text
number
emoji
symbol
url
email
mention
hashtag
named_entity
unknown
```

Language detection should be probabilistic.

Never assume:

```python
language = "es"
```

Prefer:

```python
[
    LanguageCandidate("es", 0.71),
    LanguageCandidate("pt", 0.15),
    LanguageCandidate("unknown", 0.14)
]
```

when uncertain.

---

# 6. Unicode Analysis

Implement dedicated Unicode analysis.

Detect:

* normalization differences
* invisible characters
* zero-width characters
* combining characters
* mixed scripts
* homoglyphs
* confusables
* full-width forms
* unusual whitespace
* bidirectional control characters

Produce at least:

```python
@dataclass
class UnicodeFeatures:
    scripts: list[str]
    mixed_scripts: bool

    invisible_characters: list[str]

    confusable_characters: list["ConfusableCharacter"]

    confusable_skeleton: str | None

    suspicious_unicode_score: float
```

Example:

```text
paypal
pаypal
```

may look identical while containing characters from different alphabets.

The library should detect this.

---

# 7. Character-Level Analysis

Implement character-level similarity independent of semantic AI.

Recommended algorithms:

```text
Levenshtein distance
Damerau-Levenshtein
Jaro similarity
Jaro-Winkler
character n-grams
normalized edit distance
Longest Common Subsequence
```

These algorithms MUST remain available even when semantic models are disabled.

Example API:

```python
result = character_similarity(
    "comprar",
    "c0mpr4r",
)
```

Possible output:

```python
CharacterSimilarity(
    levenshtein=0.71,
    jaro_winkler=0.82,
    ngram_similarity=0.65,
    combined=0.75,
)
```

---

# 8. Lexical Analysis

Support:

* tokenization
* word n-grams
* token n-grams
* stemming where applicable
* lemmatization where applicable
* stop words
* Jaccard similarity
* TF-IDF
* MinHash
* SimHash

Do not require every language to have a stemmer or lemmatizer.

Fall back gracefully.

---

# 9. Semantic Analysis

Semantic similarity MUST be a separate channel.

Use multilingual embedding models through a provider interface.

Example:

```python
class EmbeddingProvider(Protocol):

    def embed(
        self,
        texts: list[str],
    ) -> list[list[float]]:
        ...
```

The core library MUST NOT be permanently coupled to one AI provider or one model.

Possible implementations may include:

```text
Sentence Transformers
BGE
E5
OpenAI embeddings
local models
remote APIs
custom embeddings
```

Example:

```python
semantic_score = cosine_similarity(
    embedding_a,
    embedding_b,
)
```

Semantic similarity alone MUST NOT determine the final similarity score.

---

# 10. Symbols and Emoji

Emoji and symbols MUST NOT have one global fixed pronunciation.

Represent them as semantic objects.

Example:

```python
@dataclass
class SymbolReading:
    text: str
    language: str | None
    probability: float
    reading_type: str


@dataclass
class SymbolConcept:
    id: str
    probability: float


@dataclass
class SymbolInstance:
    raw: str

    unicode_name: str | None

    concepts: list[SymbolConcept]

    readings: list[SymbolReading]
```

For:

```text
🏠
```

possible information:

```text
concept:
house

Spanish readings:
casa
hogar
vivienda

English readings:
house
home
```

Do NOT automatically convert:

```text
🏠 → casa
```

Context must influence candidate selection.

---

# 11. Numbers and Symbols as Spoken Forms

Numbers may represent:

* numeric value
* letters
* syllables
* words
* visual substitutions

Example:

```text
salU2
```

Candidate interpretations:

```text
U → u
2 → dos

sal + u + dos
→ saludos
```

Another example:

```text
100pre
```

Potential interpretation:

```text
100 → cien
cien + pre
≈ siempre
```

The decoder should generate candidates instead of enforcing these transformations globally.

---

# 12. Rebus Decoder

A dedicated `RebusDecoder` module is REQUIRED.

Its job is to interpret mixed sequences of:

* text
* emoji
* numbers
* symbols
* letters
* visual substitutions

as potentially encoded words or phrases.

Example:

```text
Fra🏠do
```

Tokenize as:

```text
["Fra", "🏠", "do"]
```

Generate readings:

```text
Fra
+
{
    casa,
    hogar,
    vivienda,
    house,
    home
}
+
do
```

One candidate:

```text
Fra + casa + do
→ fracasado
```

The decoder should rank candidates.

Example interface:

```python
class RebusDecoder:

    def decode(
        self,
        text: str,
        languages: list[str] | None = None,
        max_candidates: int = 10,
    ) -> list["DecodedCandidate"]:
        ...
```

Candidate representation:

```python
@dataclass
class DecodedCandidate:
    text: str

    score: float

    transformations: list["Transformation"]

    language: str | None

    lexical_score: float

    phonetic_score: float

    context_score: float

    symbol_score: float
```

Example result:

```python
DecodedCandidate(
    text="fracasado",
    score=0.97,
    transformations=[
        Transformation(
            source="🏠",
            replacement="casa",
            transformation_type="symbol_reading",
        )
    ],
    language="es",
    lexical_score=0.99,
    phonetic_score=0.98,
    context_score=0.95,
    symbol_score=0.94,
)
```

---

# 13. Candidate Generation

Never enumerate all possible symbol combinations without limits.

Use bounded search.

Recommended algorithm:

```text
Beam Search
```

Example:

```python
beam_width = 20
max_candidates = 10
max_symbol_readings = 8
```

These values MUST be configurable.

The decoder should discard very low-probability branches early.

---

# 14. Phonetic Pipeline

Do NOT treat TTS output as canonical truth.

Correct pipeline:

```text
Message
   ↓
language candidates
   ↓
symbol / number resolution
   ↓
spoken-form candidates
   ↓
grapheme-to-phoneme
   ↓
phonemes / IPA
   ↓
phonetic features
```

TTS may optionally provide evidence but must not be the source of truth.

---

# 15. Grapheme-to-Phoneme Provider

Implement G2P behind an interface:

```python
class G2PProvider(Protocol):

    def phonemize(
        self,
        text: str,
        language: str,
    ) -> "PhoneticCandidate":
        ...
```

Allow multiple implementations.

Possible backends:

```text
Epitran
espeak-ng
phonemizer
language-specific models
custom G2P models
external services
```

---

# 16. Phonetic Representation

Do not only compare raw IPA strings.

Store phonemes separately.

Example:

```python
@dataclass
class PhoneticCandidate:
    source: str

    language: str

    ipa: str | None

    phonemes: list[str]

    confidence: float
```

Later versions should support articulatory features.

Example:

```text
/p/

consonant
bilabial
stop
voiceless
```

```text
/b/

consonant
bilabial
stop
voiced
```

Therefore:

```text
distance(p, b)
```

should be smaller than:

```text
distance(p, a)
```

---

# 17. Phonetic Similarity

Implement:

```text
phoneme edit distance
weighted phoneme edit distance
phoneme n-grams
feature-based phoneme distance
```

Example:

```python
phonetic_similarity(
    candidate_a,
    candidate_b,
)
```

Return individual metrics.

---

# 18. Spoken Candidates

Because one written input may have multiple possible readings, use:

```python
@dataclass
class SpokenCandidate:
    text: str
    language: str
    probability: float
    source_transformations: list["Transformation"]
```

Example:

```text
🏠
```

might produce:

```python
SpokenCandidate("casa", "es", 0.90, ...)
SpokenCandidate("hogar", "es", 0.50, ...)
SpokenCandidate("house", "en", 0.88, ...)
SpokenCandidate("home", "en", 0.76, ...)
```

---

# 19. Obfuscation Detection

Implement an independent obfuscation detector.

Possible features:

```text
leet substitutions
mixed scripts
homoglyphs
symbol substitutions
emoji inside words
unexpected numbers inside words
zero-width characters
character repetition
unusual separators
case alternation
punctuation flooding
Unicode confusables
word fragmentation
```

Example result:

```python
@dataclass
class ObfuscationFeatures:
    leet_score: float
    homoglyph_score: float
    unicode_score: float
    fragmentation_score: float
    symbol_substitution_score: float
    repetition_score: float

    combined_score: float
```

---

# 20. Similarity Engine

Main API:

```python
result = analyzer.compare(
    message_a,
    message_b,
)
```

Return multiple independent scores.

Example:

```python
@dataclass
class ComparisonResult:
    score: float

    semantic: float | None

    lexical: float | None

    character: float | None

    visual: float | None

    phonetic: float | None

    symbolic: float | None

    decoded_similarity: float | None

    obfuscation_similarity: float | None

    explanations: list[str]
```

Example:

```json
{
  "score": 0.94,
  "semantic": 0.93,
  "lexical": 0.48,
  "character": 0.61,
  "visual": 0.57,
  "phonetic": 0.96,
  "symbolic": 0.92,
  "decoded_similarity": 0.99,
  "obfuscation_similarity": 0.89
}
```

---

# 21. Scoring

Initial versions may use configurable weighted scoring.

Example:

```python
final_score = (
    semantic * 0.25
    + lexical * 0.10
    + character * 0.10
    + visual * 0.10
    + phonetic * 0.15
    + symbolic * 0.10
    + decoded * 0.20
)
```

These values MUST NOT be hardcoded.

Use configuration:

```yaml
similarity:
  semantic: 0.25
  lexical: 0.10
  character: 0.10
  visual: 0.10
  phonetic: 0.15
  symbolic: 0.10
  decoded: 0.20
```

Later versions should support learned scoring models.

Possible models:

```text
Logistic Regression
LightGBM
XGBoost
small neural networks
```

---

# 22. Confidence and Uncertainty

Every non-deterministic transformation MUST expose confidence.

Wrong:

```python
decoded = "fracasado"
```

Preferred:

```python
candidates = [
    ("fracasado", 0.97),
    ("fra hogar do", 0.08),
]
```

The library must allow:

```text
unknown
uncertain
ambiguous
not enough evidence
```

Never hallucinate a reconstruction when confidence is low.

---

# 23. Spam Detection

Spam detection MUST consume fingerprint features rather than only raw text.

Example feature vector:

```text
semantic similarity to known spam
character similarity
decoded similarity
phonetic similarity
URLs count
domains
emoji ratio
number ratio
uppercase ratio
repetition score
homoglyph score
obfuscation score
known-pattern similarity
message entropy
```

Provide interface:

```python
result = spam_detector.predict(fingerprint)
```

Possible output:

```python
SpamResult(
    probability=0.94,
    labels=[
        "promotion",
        "obfuscated",
        "known_pattern",
    ],
    reasons=[
        "High semantic similarity to known spam",
        "Emoji substitution detected",
        "Decoded candidate matched known template",
    ],
)
```

---

# 24. Known Pattern Detection

Allow users to register patterns:

```python
engine.add_pattern(
    id="scam_prize",
    examples=[
        "ganaste un premio",
        "reclama tu premio",
    ]
)
```

Then:

```python
matches = engine.match_patterns(message)
```

Patterns should be searchable through:

```text
semantic representation
lexical representation
phonetic representation
decoded representation
```

---

# 25. Search

Support:

```python
engine.find_similar(
    message,
    limit=20,
)
```

For large datasets, use candidate retrieval before expensive comparison.

Recommended architecture:

```text
query
  ↓
fingerprint
  ↓
ANN semantic search
  +
lexical candidate search
  +
phonetic candidate search
  ↓
candidate union
  ↓
full comparison
  ↓
optional reranker
  ↓
results
```

The core API must not depend on one vector database.

Create interfaces for storage backends.

---

# 26. Provider Architecture

External functionality MUST use provider interfaces.

Examples:

```python
EmbeddingProvider

LanguageDetectionProvider

G2PProvider

LemmatizerProvider

VectorStoreProvider

RerankerProvider

SymbolKnowledgeProvider
```

This allows implementations to be replaced without changing the core engine.

---

# 27. Recommended Package Structure

```text
textintel/
│
├── core/
│   ├── fingerprint.py
│   ├── types.py
│   ├── config.py
│   └── exceptions.py
│
├── normalization/
│   ├── unicode.py
│   ├── whitespace.py
│   ├── repetition.py
│   ├── leetspeak.py
│   └── confusables.py
│
├── language/
│   ├── detector.py
│   ├── segmentation.py
│   └── providers/
│
├── lexical/
│   ├── tokenizer.py
│   ├── ngrams.py
│   ├── similarity.py
│   ├── minhash.py
│   └── simhash.py
│
├── semantic/
│   ├── embeddings.py
│   ├── similarity.py
│   ├── reranker.py
│   └── providers/
│
├── symbols/
│   ├── emoji.py
│   ├── unicode_symbols.py
│   ├── numbers.py
│   ├── resolver.py
│   └── knowledge.py
│
├── phonetic/
│   ├── g2p.py
│   ├── ipa.py
│   ├── phonemes.py
│   ├── features.py
│   ├── similarity.py
│   └── providers/
│
├── rebus/
│   ├── tokenizer.py
│   ├── candidates.py
│   ├── beam_search.py
│   ├── scorer.py
│   └── decoder.py
│
├── visual/
│   ├── homoglyph.py
│   ├── scripts.py
│   └── similarity.py
│
├── obfuscation/
│   ├── features.py
│   └── detector.py
│
├── comparison/
│   ├── scorer.py
│   └── comparator.py
│
├── detection/
│   ├── spam.py
│   ├── patterns.py
│   └── duplicates.py
│
├── storage/
│   ├── base.py
│   ├── memory.py
│   └── vector/
│
├── engine/
│   ├── analyzer.py
│   ├── compare.py
│   └── search.py
│
└── cli/
    └── main.py
```

---

# 28. Public API

Keep the public API small.

Target usage:

```python
from textintel import TextIntelligence

engine = TextIntelligence()

fingerprint = engine.analyze(
    "Fra🏠do"
)
```

Comparison:

```python
result = engine.compare(
    "Fra🏠do",
    "fracasado",
)

print(result.score)
print(result.phonetic)
print(result.decoded_similarity)
```

Search:

```python
matches = engine.find_similar(
    "g4n4 💰",
    limit=10,
)
```

Rebus decoding:

```python
candidates = engine.decode(
    "Fra🏠do"
)
```

Spam:

```python
result = engine.detect_spam(
    "G4N4 💰 AH0R4!!!"
)
```

---

# 29. Explainability

Every major decision should be inspectable.

Example:

```python
result = engine.compare(
    "Fra🏠do",
    "fracasado",
)
```

Possible explanation:

```text
Overall similarity: 0.96

Evidence:

- Detected likely Spanish context.
- Emoji 🏠 has candidate reading "casa".
- Rebus candidate "fracasado" generated.
- Candidate confidence: 0.97.
- Phonetic similarity: 0.99.
- Direct lexical similarity before decoding: 0.41.
- Semantic similarity after decoding: 0.99.
```

Never return only an unexplained probability when detailed evidence exists.

---

# 30. Performance Requirements

Expensive models MUST be lazy-loaded.

The user should be able to use basic functionality without downloading large AI models.

Example:

```python
engine = TextIntelligence(
    semantic=False,
    phonetic=False,
)
```

should still provide:

```text
Unicode analysis
character similarity
lexical similarity
obfuscation features
basic symbol analysis
```

Support batching whenever possible.

---

# 31. Optional Dependencies

Use optional extras.

Example:

```text
textintel
textintel[semantic]
textintel[phonetic]
textintel[ml]
textintel[all]
```

Avoid forcing users to install several GB of models for basic functionality.

---

# 32. Security

Input must be treated as untrusted.

Never:

* execute input
* evaluate Python
* render arbitrary HTML without escaping
* fetch URLs automatically
* follow links automatically
* execute embedded commands
* load arbitrary remote models based on message content

Set configurable limits for:

```text
input length
number of segments
number of candidate readings
beam width
recursion
model batch sizes
```

Protect candidate generation from combinatorial explosion.

---

# 33. Privacy

The core library should support fully local execution.

Remote AI services may be optional providers.

Do not send user messages to external providers unless explicitly configured.

---

# 34. Testing Strategy

Testing is mandatory.

Use:

```text
unit tests
integration tests
property-based tests where useful
benchmark datasets
regression tests
```

Every discovered failure should become a regression test.

---

# 35. Initial Test Cases

## Basic normalization

```text
COMPRA AHORA
compra ahora
```

Expected:

```text
high similarity
```

---

## Leetspeak

```text
c0mpr4 ah0r4
compra ahora
```

Expected:

```text
obfuscation detected
high decoded similarity
```

---

## Emoji rebus

```text
Fra🏠do
fracasado
```

Expected:

```text
candidate "fracasado" generated
high phonetic similarity
high decoded similarity
```

---

## Numeric rebus

```text
salU2
saludos
```

Expected:

```text
candidate "saludos" generated
```

---

## Symbol semantics

```text
gana 💰
gana dinero
```

Expected:

```text
high semantic or symbolic similarity
```

---

## Homoglyph

```text
paypal
pаypal
```

Expected:

```text
mixed/confusable script detected
high visual similarity
```

---

## Repetition

```text
GAAAAANAAAA DINEROOOO
gana dinero
```

Expected:

```text
repetition obfuscation detected
high normalized similarity
```

---

## Code switching

```text
bro compra NOW
bro compra ahora
```

Expected:

```text
multiple languages identified
reasonable semantic similarity
```

---

## Negative case

```text
Fra🏠do
ferrocarril
```

Expected:

```text
low overall similarity
```

---

## Ambiguous emoji

```text
🔥
```

Expected:

```text
do not force exactly one interpretation
return multiple concepts/readings or unknown
```

---

# 36. Evaluation Dataset

Create a dataset format such as:

```json
{
  "a": "Fra🏠do",
  "b": "fracasado",
  "languages": ["es"],
  "labels": {
    "similar": true,
    "rebus": true,
    "obfuscated": true
  }
}
```

Also include negative examples:

```json
{
  "a": "Fra🏠do",
  "b": "cansado",
  "languages": ["es"],
  "labels": {
    "similar": false
  }
}
```

Evaluation MUST contain hard negatives.

Do not test only easy positive examples.

---

# 37. Development Phases

## Phase 1 — Deterministic Core

Implement:

```text
Unicode normalization
confusable detection
tokenization
character metrics
lexical metrics
basic emoji metadata
basic leetspeak detection
fingerprint object
comparison API
```

No large AI models required.

---

## Phase 2 — Semantic Layer

Add:

```text
multilingual embeddings
cosine similarity
vector provider abstraction
batch processing
```

---

## Phase 3 — Phonetic Layer

Add:

```text
language-aware G2P
IPA representation
phoneme tokens
phonetic edit distance
weighted phonetic similarity
```

---

## Phase 4 — Rebus Decoder

Add:

```text
symbol readings
number readings
emoji readings
candidate generation
beam search
candidate scoring
context scoring
```

Critical examples:

```text
Fra🏠do → fracasado
salU2 → saludos
```

---

## Phase 5 — Detection Models

Add:

```text
spam classifier
duplicate classifier
pattern detector
learned score combination
```

---

## Phase 6 — Large-Scale Search

Add:

```text
vector indexes
MinHash indexes
phonetic indexes
candidate retrieval
reranking
```

---

# 38. Things the Implementation MUST NOT Do

Do NOT:

```text
assume one language per message
assume one pronunciation per emoji
assume one meaning per symbol
use TTS as canonical pronunciation
replace raw text destructively
use embeddings as the only similarity signal
force a decoding when confidence is low
hardcode Spanish-specific logic into the core
hardcode one embedding vendor
hardcode one vector database
generate unlimited rebus combinations
hide uncertainty
```

---

# 39. Design Philosophy

Prefer:

```text
independent signals
probabilistic candidates
explainability
modularity
replaceable providers
language-neutral core
bounded computation
local-first execution
```

over:

```text
one giant model
opaque classification
language-specific hacks
hardcoded assumptions
```

---

# 40. Example End-to-End Analysis

Input:

```text
Fr4🏠do
```

Possible internal process:

```text
1. Preserve raw input.

2. Detect likely Spanish context.

3. Detect:
   4 → possible "a" leetspeak substitution.

4. Produce normalized candidate:

   Fra🏠do

5. Tokenize:

   Fra
   🏠
   do

6. Resolve 🏠:

   casa   0.93
   hogar  0.51
   house  0.25

7. Candidate generation:

   Fra + casa + do
   → fracasado

8. Lexical validation:

   "fracasado" exists / is linguistically plausible.

9. Phonetic analysis:

   candidate pronunciation ≈ pronunciation of "fracasado"

10. Semantic validation.

11. Produce:

   decoded_candidate = "fracasado"
   confidence = 0.96
```

Expected public result:

```json
{
  "raw": "Fr4🏠do",
  "languages": [
    {
      "language": "es",
      "probability": 0.91
    }
  ],
  "decoded_candidates": [
    {
      "text": "fracasado",
      "confidence": 0.96
    }
  ],
  "obfuscation": {
    "detected": true,
    "score": 0.89
  }
}
```

---

# 41. Quality Requirement for LLM Coding Agents

When implementing this repository:

1. Do not implement the entire system in one file.
2. Prefer typed interfaces.
3. Keep algorithms individually testable.
4. Do not introduce large dependencies unless justified.
5. Add tests with every feature.
6. Do not silently swallow errors.
7. Preserve backward compatibility in public APIs when possible.
8. Document uncertain heuristics.
9. Separate deterministic algorithms from AI-backed providers.
10. Benchmark changes affecting candidate generation.
11. Never replace probabilistic output with fabricated certainty.
12. Keep multilingual capability as a first-class architectural requirement.

---

# 42. Definition of Done for MVP

The first usable MVP is complete when all of the following work:

```python
engine.analyze(text)

engine.compare(a, b)

engine.decode(text)
```

And the test suite demonstrates useful behavior for:

```text
normal text
case variations
spacing variations
leetspeak
Unicode confusables
mixed alphabets
emoji symbols
numbers inside words
basic rebus decoding
multilingual input
mixed-language input
```

The MVP must correctly detect the intended relationship in examples such as:

```text
Fra🏠do
fracasado
```

and:

```text
salU2
saludos
```

without hardcoding those exact complete strings as special cases.

The implementation should generalize from symbol readings, phonetic similarity, lexical plausibility, and contextual evidence.
