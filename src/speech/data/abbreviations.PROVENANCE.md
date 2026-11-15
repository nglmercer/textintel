# Speech abbreviation catalog

`abbreviations.json` is a deliberately small, reviewed spelling catalog. Each
entry contains its exact written form, written expansion, activation context,
source URL and a note describing what that source establishes. Context names
are conservative library policies, not claims that the source specifies a
complete text parser. An expansion is not a phonetic transcription or evidence
of native speech accuracy.

The 20 entries marked `legacy` reproduce the original English (3), Spanish (7),
French (1), German (2), Portuguese (5) and Italian (2) tables at commit `afa57c9`.
Their source-code links establish compatibility provenance; they do not claim a
new linguistic review. Italian remains an extra versioned language, outside the
20-language typed registry. The separate sentence-boundary vocabulary contains
the original 22 spellings and does not authorize expansions.

New entries use primary institutional sources: OSEPA and University of Kota
(Hindi); Bangladesh MOPA's 2025 administrative glossary (Bengali); LPU's Punjabi-I
textbook; RMVS's Marathi manual and answer key; Tamil Lexicon hosted by Tamil
Virtual Academy; NUML's bibliographic notation table (Urdu); EYD V (Indonesian);
TDK (Turkish); Japan's Consumer Affairs Agency; Korea's National Institute of
Korean Language; UNISSA's Arabic thesis manual; an official Russian administrative
instruction; and Quy Nhon University's Vietnamese abbreviation table. Exact
documents and page/section notes remain attached to every entry.

The Bengali MOPA glossary, printed page 75, was checked visually: `ড.` expands to
`ডক্টর`, whereas `ডা.` expands to `ডাক্তার`. A text extraction dropped a vowel,
so OCR alone was not used to resolve this distinction. Punjabi forms use the
documented colon (`ਡਾ:`, `ਪ੍ਰਿੰ:`); dot spellings are not inferred. Indonesian
`dr.` and `Dr.` have distinct expansions. These scripts' casing and punctuation
are never globally folded or transliterated.

Urdu `ص` is restricted to `Bibliographic` policy followed by a complete decimal
number. NUML defines its use as `صفحہ نمبر کے لیے`; the catalog deliberately
reads the documented label `صفحہ نمبر`. This is an explicit reference-reading
policy, not a universal claim that the letter always expands that way. Arabic
publication metadata abbreviations likewise require bibliographic opt-in.

Telugu and Chinese currently have zero general abbreviation entries. Telugu
institutional usage examples did not establish an explicit native spelling/full
form pair suitable for this catalog. Chinese quantity/unit symbols belong to
the existing quantity parser and are not duplicated as generic abbreviations.
Zero is a declared data gap, not proof that a language has no abbreviations.

Company abbreviations alone permit attachment to name letters. The matcher does
not validate corporate registration, infer a person's academic/medical title,
guess grammatical inflections, add undocumented punctuation aliases, or implement
chat substitutions. Code, tags, URL/email/path fields and complete structured
JSON remain protected. Unit tests cover context boundaries and source byte
offsets; they are authored regressions, not independent precision measurements.
