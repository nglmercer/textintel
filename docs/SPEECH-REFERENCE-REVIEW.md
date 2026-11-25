# Speech reference review

This review separates byte-for-byte reference agreement, bounded wording variants,
reference defects, and remaining library limitations. It does not change the
upstream gold, evaluation policy, or implementation. It is a source-based review,
not a native-speaker certification of all twenty supported languages.

## Reproducible snapshot

The tables below review `target/coverage/speech-nemo-reviewed.json`, SHA-256
`9c009dd673f59175766c4f82f18dad50c1e11ea9ae436c9a4704bcd6cbc90d81`,
read on 2026-11-14. The report was produced with
[`reference-policy.json`](../data/speech/reference-policy.json), including explicit
category hints, currency-symbol assignments, and the Arabic `f` unit alias.
These results therefore describe that caller policy, not unconstrained `Auto`
interpretation.

| Measurement | Cases |
| --- | ---: |
| Selected spoken references | 544 |
| Exact spoken matches | 464 |
| Additional bounded variant matches | 49 |
| Exact or bounded variant matches | 513 |
| Strict mismatches, including accepted variants | 80 |
| Mismatches not accepted by a variant rule | 31 |
| Implementation reports fully supported | 540 |

This table describes the strict reference profile, not the separate literal-
component profile described below. Its pinned hash and measurements are retained;
literal-reading examples do not adjust this score. Successfully producing speech
for an input does not by itself establish calendar validity or native accuracy.

`fully_supported` means that rendering completed; it is not an independent
correctness judgment. The corpus has no annotated expected spans, so these figures
do not establish extraction precision, recall, or code coverage. The selection is
the first ten usable upstream rows per available locale/category, retaining all
alternatives, rather than a representative multilingual test distribution.

The source is [NeMo text processing at revision
ddadfb2a38d2bc6b8cc6232c4f915eb60f500688](https://github.com/NVIDIA/NeMo-text-processing/tree/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688).
Every case below links to its physical upstream line. Selection metadata, input
variants, source-file hashes, and the original alternatives remain in
[`reference-nemo.json`](../data/speech/reference-nemo.json). Upstream material is
Apache-2.0; its [license](../data/speech/LICENSE-NEMO-APACHE-2.0.txt) is retained.
Parallel implementation work can produce a newer report; the hash above fixes the
scope of this review.

## Review of reference differences

The category table counts only still-unaccepted cases. Separately accepted
variants are already included in 513; these review categories add no passes.

| Review category | Cases | Decision |
| --- | ---: | --- |
| Reference spelling, grammatical, or pronunciation defect with source evidence | 16 | Preserve gold; record a proposed correction separately. |
| Arabic standard-language agreement versus unreviewed reference register | 8 | Prefer the documented formal reading; do not claim every dialectal reference is universally invalid. |
| Invalid Gregorian date correctly left unresolved | 3 | Keep the calendar validation and unresolved status. |
| Unresolved wording/interpretation policy | 2 | Require a declared reading convention or more context. |
| Spanish mixed-fraction word order under reference review | 1 | Keep the independently supported canonical phrase; do not add a broad reordering equivalence. |
| Mixed clock notation outside the strict accepted domain | 1 | Preserve the input under the unchanged reference policy; an opt-in TTS source convention normalizes coherent redundancy without matching the original wording. |

### A1: Arabic thousands spelling — nine reference defects

The reference uses `اَلاف`; the implementation uses `آلاف`. These are different
characters, not merely composed/decomposed encodings of the same spelling:
U+0622 decomposes to U+0627 U+0653, whereas the reference has U+0627 U+064E.
The Arabic thousands rule in [CLDR 48 Arabic
RBNF](https://raw.githubusercontent.com/unicode-org/cldr/release-48/common/rbnf/ar.xml)
uses the latter implementation spelling. See also the
[Unicode normalization specification](https://www.unicode.org/reports/tr15/).
The proposed reference correction is limited to this word, not arbitrary removal
of Arabic marks, hamza, or madda. All nine affected case IDs appear in the table.

### A2: Arabic count agreement — two bounded variants and eight pending differences

The implementation chooses formal Arabic quantity grammar: plural counted nouns
for 3–10, singular accusative for 11–99, and the appropriate numeral gender.
The [Al-Mustaqbal University lecture on numbers and counted nouns, pp. 2 and
5](https://uomus.edu.iq/img/lectures21/MUCLecture_2025_6129126.pdf)
states these rules. This explains `سبعة دولارات`, `خمسة دنانير كويتية`,
and the minor-unit endings in the table. The selected Fahrenheit interpretation
also expands the implicit degree noun: `تسع درجات فهرنهايت`. The `f` interpretation
is an explicit caller alias, not a locale guess.

The original currency references often leave a borrowed currency noun invariant.
That can reflect a colloquial or annotation convention; the general formal rule
alone does not certify every dialectal money expression. No unrestricted
singular/plural equivalence or omitted accusative-ending equivalence is warranted
for TTS. Keep the remaining eight differences unaccepted until their complete register
and construction are supported. The implementation is not a complete
Arabic sentence-case analyzer.

There is positive dialect evidence for parts of these constructions, so a
singular currency noun is not universally an error. [Gadalla (2000), §1,
example (9)](https://www.academia.edu/64245424/Numerals_in_Standard_Arabic_and_Egyptian_Colloquial_Arabic_A_comparative_study)
documents masculine singular monetary units with long 3–10 numerals in Egyptian
speech. [Holes (2016), §3.8, p.137](https://www.academia.edu/105868007/Dialect_Culture_and_Society_in_Eastern_Arabia_Volume_III_Phonology_Morphology_Syntax_Style)
documents singular dinar among Baḥārna speakers in Bahrain; that study principally
describes older speakers recorded in the 1970s, not a general modern Kuwaiti norm.
[Hallberg (2016), §10.3(iv), p.241](https://lup.lub.lu.se/search/publication/530e5fe6-ec77-4e84-9a45-0935598e86a8)
reports counted nouns without accusative marking in spoken standard Arabic,
while cautioning against generalization from limited data.

The documented productive construction does justify a bounded integer-only
variant: a long 3–10 numeral followed by a reviewed masculine singular currency
unit. Composing that rule with the shared noun/qualifier lexicon accepts
`nemo-ar-money-0001` and `nemo-ar-money-0010`; the sources do not directly quote
every generated currency phrase. The rule preserves value, sign and naming
policy and excludes all decimal and minor-unit compositions, including `.00`.
This is a regional spoken alternative, not the canonical formal reading or a
claim of one pan-Arabic dialect.
For example, the Egyptian/Bahraini tens differ from the nominative tens retained
in some golds; removing a cent noun's final alif alone does not establish a
coherent dialectal reading. Review must specify the register, complete numeral,
minor unit and qualifier together. The Fahrenheit ellipsis needs separate
adjudication. The other eight cases remain outside the accepted score.

### R1: Russian reference spelling and agreement — five cases

`nemo-ru-measure-0001` and `0002` have `килограм` with one final м. The
[Russian orthographic dictionary hosted by Gramota](https://gramota.ru/storage/public/normdicts/orfograficheskij_slovar.pdf)
lists `килограмм`, with counting forms `килограмм` and `килограммов`. These two
valid counting forms do not license the reference misspelling.

`nemo-ru-measure-0008` has masculine `два` modifying the feminine fractional
denominator; the correct numeric component is `четыре целых две десятых`.
[CLDR 48 Russian RBNF](https://raw.githubusercontent.com/unicode-org/cldr/release-48/common/rbnf/ru.xml)
uses the feminine cardinal rules for these fractions. This assessment corrects
the numeric component only: the remaining written adjectival phrase is preserved,
not certified as polished Russian orthography.

`nemo-ru-money-0003` mixes nominative tens with genitive units in
`тридцать одного рубля`. The isolated nominative amount is
`тридцать один рубль`; a contextual genitive amount would instead start
`тридцати одного`. Gramota's [answer on inflecting compound
numerals](https://gramota.ru/spravka/vopros/296550) requires the components to
decline together. Choosing a case variant does not justify this mixed form.

`nemo-ru-money-0007` compares a normal major/minor reading,
`два рубля три копейки`, with `две целых три сотых рублей`. A fractional fixed-unit
reading would end in singular genitive `рубля`. Gramota's [decimal-fraction
answer](https://gramota.ru/spravka/vopros/285780) supports singular genitive after
such fractions. Its [numeral grammar
overview](https://gramota.ru/biblioteka/spravochniki/russkij-yazyk-kratkij-teoreticheskij-kurs-dlya-shkolnikov/grammaticheskie-priznaki-kolichestvennykh-chislitelnykh)
does allow plural genitive when a fraction denotes part of a collection, as
opposed to part of one unit. This isolated ruble amount supplies no such collection
context. Do not accept the plural reference just to improve the score. A future
decimal-currency reading option could generate the correctly agreed singular
form without copying this reference.

### R2: Russian explicit US-dollar qualifier — loss fixed, pronunciation differs

`nemo-ru-money-0004` formerly preserved USD identity in the typed result but
dropped the explicitly written source qualifier `США` from speech. That was a
real library limitation, and is now fixed: the output is
`восемнадцать тысяч долларов сэ шэ а`. The source-aware renderer retains the
country qualifier once, after the complete amount, including negative, minor-unit,
and scaled amounts. It replaces an already present CLDR `США` suffix instead of
duplicating it. Independent examples and CAD/RUB/unqualified-USD contrasts are in
[`speech_quantities.rs`](../tests/speech_quantities.rs).

The original `сэ ш а` differs from the normative spelled-out pronunciation.
Gramota's [dictionary of Russian
difficulties](https://gramota.ru/biblioteka/slovari/slovar-trudnostej-russkogo-yazyka)
records the conventional pronunciation `сэ-шэ-а` and the word-like alternative.
The implementation chooses the spelled-out reading; it does not copy the
reference's missing vowel in the middle letter name. The strict mismatch remains
visible as a reference-pronunciation defect and contributes no additional accepted
pass. Under the default `CurrencyNaming::Style`, this bounded fix applies to
Russian USD amounts with an explicit source token `США`. The opt-in
`CurrencyNaming::Full` also retains the localized country qualifier for an
unqualified ISO `USD` amount, using the shared CLDR names. It is not enabled in
this reference policy and does not change these scores. Full lexical names are
available across the twenty-language path; the shared Arabic currency paradigms
are reviewed and inflected rather than copied as invariant CLDR labels.

### D1: Invalid Gregorian dates — three deliberate refusals

`nemo-es-date-0009`, `nemo-es-date-0010`, and `nemo-hi-date-0002` request February
30 or June 31. The implementation preserves the source rather than verbalizing a
nonexistent Gregorian calendar date. The month/day table in
[RFC 3339 §5.7](https://datatracker.ietf.org/doc/html/rfc3339#section-5.7)
gives February at most 29 days and June 30. This is not missing lexical coverage.
Applications can explicitly select `InvalidDatePolicy::ReadComponents` to read
eligible atomic components, with `Preserve` still the default. The result uses
`TemporalValue::InvalidCalendarDate` and evaluation kind `invalid_date`, never
a valid `Date`. It retains source and UTF-8 offsets and reports
`reason: Some("invalid calendar date")` even when `supported` is true.
Resolved date order and bounded components are required: month 13, year 0,
ambiguous order, and invalid composite date lists/sequences are not repaired or
partially read. Ordinary invalid public `Date` values remain rejected.

For the yearless Hindi case, Natural `Auto` can instead classify `३१-०६` as a
range. A literal date application must supply its context explicitly:

```sh
textintel normalize-speech '३१-०६' --locale hi --reading-style natural --invalid-date-policy read-components --interpret-as date --json
```

This reads “इकतीस जून” while retaining invalid-calendar semantics. Serde spells
the option `read_components`. Such a comparison needs a separate literal-profile
report; it does not change the original gold, the three default-policy refusals
in this ledger, or the 513 accepted comparisons in this snapshot.

`SpeechEvalReport.diagnostics` now retains supported literal warnings even when
spoken output matches exactly and the case is absent from `failures`.
`diagnostics_by_reason` counts all retained reasons;
`unresolved_by_reason` remains restricted to unsupported readings. Text evaluation
output shows diagnostic and supported-with-diagnostic counts alongside match
counts. No reviewed variant is applied when a reading has `reason: Some(...)`;
this includes literal invalid dates that generated speech successfully.

### E1: English 24-hour whole hours — corrected renderer, reference retained

`nemo-en-time-0006` supplies `23:00` with the reference `twenty three o'clock`.
The earlier Natural renderer copied that convention. Cambridge's [official
grammar excerpt](https://assets.cambridge.org/97813166/36176/excerpt/9781316636176_excerpt.pdf)
explicitly rejects that phrase. The [US Department of Defense time table](https://media.defense.gov/2014/Jun/20/2002655870/-1/-1/1/140620-N-ZZ182-6514.pdf)
documents the 24-hour `twenty-three-hundred` reading. The shared renderer now
uses `twenty three hundred`, retaining the 24-hour value instead of guessing
an unqualified twelve-hour time. Whole-hour `o'clock` remains limited to 1–12
without an explicit AM/PM marker. This correction removes an old exact match;
the original gold and denominator are unchanged and no equivalence rule accepts
the erroneous phrase.

### P1: English unit symbols — one unresolved output convention

For `nemo-en-measure-0007`, `3 cc/s`, the implementation expands the units to
`three cubic centimeters per second`; the reference retains letter-like
`three CC per S`. The [US National Cancer Institute abbreviation
table](https://training.seer.cancer.gov/abstracting/additional/abbreviations/c.html)
attests cubic centimeter for CC, but also shows that the abbreviation is
context-dependent. The source is already interpreted as a measure here. A
reference containing capitals does not specify whether a speech engine should
spell their letters or interpret them as units. Define that output convention
explicitly before accepting it. Do not globally case-fold SI symbols: capital S
and lower-case s need not denote the same unit.

### P2: French decade loanword — context remains insufficient

For `nemo-fr-date-0009`, `les 80s`, `les eighties` and
`les années quatre-vingt` can be ordinary French ways to refer to the 1980s.
[Larousse](https://www.larousse.fr/dictionnaires/francais/eighties/28097)
attests the borrowing; the [Académie française](https://www.academie-francaise.fr/les-sixties-les-seventies)
prefers the French expression. However, the bare numeric decade does not specify
a century. The bounded loanword variant requires an explicitly interpreted 1980s
decade and does not infer that century merely to accept this case. Keep the
remaining interpretation uncertainty visible.

### S1: Spanish mixed half — corrected canonical wording, gold retained

For `nemo-es-measure-0010`, `1 1/2 mph`, the implementation now says
`una milla y media por hora`, whereas the original says
`una y media milla por hora`. The current renderer places the whole-unit noun
before the adjectival half and retains the rate denominator's scope.
[FundéuRAE's quantity example](https://www.fundeu.es/consulta/el-punto-o-la-coma-en-la-escritura-de-las-horas-432/)
explicitly uses the analogous `una hora y media`, while
[Universidad de Piura's grammatical explanation](https://www.udep.edu.pe/castellanoactual/media-loca-o-medio-loca/)
requires adjectival medio/media to agree with the noun. These support the selected
canonical output. They do not establish that every alternative ordering in
regional or mathematical usage is universally invalid, so the original remains
under reference review rather than receiving an automatic equivalence rule.
The [RAE's DPD, medio §1](https://www.rae.es/dpd/medio) documents anteposed
`y medio/media` in American usage, but its examples do not establish the singular
noun agreement after `una y media` in this particular reference. The separate
[DPD fractional-numeral entry](https://www.rae.es/dpd/fraccionarios) requires
adjectival agreement without resolving this mixed-number ellipsis. These support
keeping the regional-order question separate from the noun-agreement question;
neither licenses a blanket rejection or acceptance of the original phrase.

Contemporary regional evidence does attest the singular construction in another
domain: Conaculta's 2014 Hñähñu cookbook, in a recipe attributed to Andrea
Delgado, uses “una y media taza de agua” ([original work, accessible
copy](https://www.academia.edu/79684470/Recetario_H%C3%B1%C3%A4h%C3%B1u)).
This establishes Mexican culinary usage, not a normative rule for the speed
expression in NeMo. The case therefore remains a documented regional
construction whose transfer to this measure needs review, rather than a
demonstrated universal spelling error or an automatically accepted reordering.

This change accompanied genuine fixes outside this corpus case: `3 1/2 h`
formerly produced singular `tres y media hora`, and `3/2 h` incorrectly used
adjectival feminine `tres medias de hora` for a mathematical fraction noun.
The corrected independent readings are `tres horas y media` and
`tres medios de hora`. See the distinction between fraction nouns and adjectives
in [ULPGC IATEXT's numeral grammar](https://tulengua.iatext.ulpgc.es/es/convertir-numeros-a-texto-letras-fraccionarios.html).
Neither numerical value nor raw corpus gold was changed. Better-supported
quantity grammar decreases this snapshot's exact score by one; that decrease is
retained rather than hidden by a new case-specific variant.

### D2: Chinese mixed clock notation — strict-domain refusal

`nemo-zh-time-0009` supplies `13:10pm`. The implementation now preserves that
entire source as ambiguous under its consistent **12-hour AM/PM** validation
contract. The original verbalizes the hour as `十三点十分pm`, leaving the Latin
marker in the spoken result. This is a distinct issue from the three impossible
Gregorian dates above: 13:10 itself is a valid 24-hour time.

The [Unicode date field table](https://www.unicode.org/reports/tr35/tr35-79/tr35-dates.html#Date_Field_Symbol_Table)
distinguishes the 1–12 `h` cycle from the 0–23 `H` cycle. That distinction supports
the library's declared strict contract; it does not prove that every human or
lenient parser must reject a redundant afternoon marker after a 24-hour value.
A caller seeking tolerant transcription might interpret the source as 13:10
with redundant `pm`, while another might require correcting the source first.
The typed API now exposes this bounded source convention through
`ClockPolicy::NormalizeRedundantPeriod`
(`"clock_policy": "normalize_redundant_period"` in JSON, or
`--clock-policy normalize-redundant-period` in the CLI). The default remains
`Strict`. Opt-in parsing converts 13–23 PM to 1–11 PM and 0 AM to 12 AM, while
contradictory periods, 24 with AM/PM, and invalid fields remain unresolved.
The original source and offsets are preserved, and rendering validates canonical
clock IR even with this option enabled.

This option can make `13:10pm` fully supported for TTS: Natural retains
`TemporalValue::Clock` at hour 1 PM and yields `一点十分pm`. Precise may instead
represent the same time as `SpeechValue::Time` at 13:10; the source remains
`13:10pm` in both cases. The Natural output does not reproduce the reference `十三点十分pm`,
and it does not add an accepted comparison or change the 513 accepted cases.
The reference policy and original gold remain unchanged and do not select this
permissive source convention.
Consequently this row is a documented domain disagreement and remains
unaccepted, not a newly claimed extraction success or a universal judgment on
Chinese mathematical/time-language usage.

## Remaining accuracy work beyond these reference rows

The default `CurrencyNaming::Style` can retain a short dollar noun even for
explicit `US$7`, depending on the language and reading style. The new opt-in
`CurrencyNaming::Full` supplies localized, country-qualified CLDR names through
the shared twenty-language path and retains currency identity in minor-only
amounts. Independent contrast tests cover full names and currency distinctions;
the default wording and this reference policy stay unchanged. Full naming is
not a certification of every source qualifier or register. The shared Arabic
noun/qualifier paradigms are now reviewed, but broader native-reviewed currency
contrasts are still needed. CLDR display-name data alone does not establish
every inflected speech construction.

The formal measured-expression templates also need native linguistic review.
In particular, Arabic `ثلاثة أمتار قسمة اثنان` preserves the exact rational
operands in the current test but has not been independently certified as an
idiomatic spoken operator construction. The [Jordanian Ministry of Education
division guide](https://www.moe.gov.jo/sites/default/files/newsfiles/mt_te04.ch03.pdf)
provides primary teaching material for division; it does not validate this exact
generated string. Abstract mathematical number labels can behave differently
from numerals governed inside an ordinary sentence, so changing اثنان to اثنين
by intuition alone is not a justified correction. Record that uncertainty and
obtain a native-reviewed operator convention and minimal pairs. The existing
twenty-language template tests verify deterministic outputs and exact typed
composition, rather than complete native agreement or natural TTS wording.

## Accepted variants still reported as strict mismatches

The 49 entries marked **variant** below are accepted by named rules in
[`variants.rs`](../src/speech/variants.rs); their original strings remain unequal.
The rules operate on typed values and specified numeric components, not a general
case/accent/punctuation fold. Rule matches can overlap, so counts by rule must not
be summed as independent cases.

German compounds and capitalization are orthographic differences, not failures
to pronounce the numeric value. [Duden's number-spelling
guide](https://www.duden.de/sprachwissen/sprachratgeber/Schreibung-von-Zahlen)
supports writing number compounds below a million together. This does not license
arbitrary word joining outside typed number/unit components. The Japanese
initial-thousand variant is similarly bounded; [MEXT's numbers
table](https://tsunagarujp.mext.go.jp/assets/download/numbers_ver202404.pdf)
supports the implementation's standard 千/せん reading, without proving the exact
orthographic preference in every financial context.

The Spanish `hora/horas` alternative is valid here under an **exact-hour clock**
interpretation. The [RAE entry on clock
expressions](https://www.rae.es/dpd/hora) describes clock readings with cardinal
hour values. The same bare source `3 h` may instead be a duration in another
application; omitting `horas` from a duration would lose the unit. This bounded
clock variant must not be generalized to duration values.

Vietnamese unit spelling variants are backed by both shared CLDR metadata and
the [Vietnamese Ministry of Education mathematics curriculum](https://moet.gov.vn/content/vanban/Lists/VBDT/Attachments/1559/2.%20Ch%C6%B0%C6%A1ng%20tr%C3%ACnh%20m%C3%B4n%20To%C3%A1n.pdf),
which explicitly names centimeter as xăng-ti-mét. CLDR's
[Vietnamese locale data](https://raw.githubusercontent.com/unicode-org/cldr/release-48/common/main/vi.xml)
also supplies xentimét. This is a documented typed unit alias, not an unrestricted
phonetic similarity rule. Other rule rationales, primary teaching references,
and limits are recorded alongside each rule in `variants.rs`.

## Conflicting references and prior regressions

`nemo-ru-time-0001` and `nemo-ru-time-0002` have identical input `13:05` and policy
but disjoint gold strings: one omits the conjunction and the other includes it.
The latter already matches exactly. A deterministic renderer cannot exactly match
both alternatives simultaneously under that fixed policy. The report therefore
records a strict exact ceiling of 543/544, independent of other defects. Both
wordings can communicate the same time; retaining one case in the variant count
is more honest than removing the duplicate or rewriting the reference.

Two previously observed **actual** regressions are already exact in this
snapshot: `nemo-en-measure-0008` (`100 million kg`) and
`nemo-ru-measure-0004` (`3,2 млн л`). They previously left scale/unit labels
unexpanded. Their correction is an implementation improvement, not a spelling
variant or reference adjudication. They are excluded from the 80-row mismatch
table because they no longer fail.

## Complete strict-mismatch ledger

Each row retains every original reference alternative and the implementation's
actual output. `A1`, `A2`, `R1`, `R2`, `D1`, `E1`, `P1`, and `P2` refer to the evidence
and limits above; **variant** lists the exact evaluation rules. Strings are JSON
quoted so trailing spaces, signs, and separators remain visible. Links on IDs
point to pinned upstream physical rows, including any input-alternative index
retained in the local corpus.

| Case / upstream line | Input | Original reference(s) | Actual output | Review |
| --- | --- | --- | --- | --- |
| [nemo-ar-cardinal-0001](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/ar/data_text_normalization/test_cases_cardinal.txt#L1) | `"3006"` | `"ثلاثة اَلاف وستة"` | `"ثلاثة آلاف وستة"` | A1: reference spelling |
| [nemo-ar-cardinal-0003](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/ar/data_text_normalization/test_cases_cardinal.txt#L3) | `"9837"` | `"تسعة اَلاف وثمان مئة وسبعة وثلاثون"` | `"تسعة آلاف وثمان مئة وسبعة وثلاثون"` | A1: reference spelling |
| [nemo-ar-cardinal-0004](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/ar/data_text_normalization/test_cases_cardinal.txt#L4) | `"3917"` | `"ثلاثة اَلاف وتسع مئة وسبعة عشر"` | `"ثلاثة آلاف وتسع مئة وسبعة عشر"` | A1: reference spelling |
| [nemo-ar-cardinal-0005](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/ar/data_text_normalization/test_cases_cardinal.txt#L5) | `"5520"` | `"خمسة اَلاف وخمس مئة وعشرون"` | `"خمسة آلاف وخمس مئة وعشرون"` | A1: reference spelling |
| [nemo-ar-cardinal-0006](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/ar/data_text_normalization/test_cases_cardinal.txt#L6) | `"8721"` | `"ثمانية اَلاف وسبع مئة وواحد وعشرون"` | `"ثمانية آلاف وسبع مئة وواحد وعشرون"` | A1: reference spelling |
| [nemo-ar-cardinal-0007](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/ar/data_text_normalization/test_cases_cardinal.txt#L7) | `"8900"` | `"ثمانية اَلاف وتسع مئة"` | `"ثمانية آلاف وتسع مئة"` | A1: reference spelling |
| [nemo-ar-measure-0002](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/ar/data_text_normalization/test_cases_measure.txt#L2) | `"f9"` | `"تسعة فهرنهايت"` | `"تسع درجات فهرنهايت"` | A2: formal agreement / register review |
| [nemo-ar-measure-0008](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/ar/data_text_normalization/test_cases_measure.txt#L8) | `"%4.9876"` | `"أربعة وتسعة اَلاف وثمان مئة وستة وسبعون من عشرة آلاف في المائة"` | `"أربعة وتسعة آلاف وثمان مئة وستة وسبعون من عشرة آلاف في المائة"` | A1: reference spelling |
| [nemo-ar-measure-0009](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/ar/data_text_normalization/test_cases_measure.txt#L9) | `"%6,8944"` | `"ستة وثمانية اَلاف وتسع مئة وأربعة وأربعون من عشرة آلاف في المائة"` | `"ستة وثمانية آلاف وتسع مئة وأربعة وأربعون من عشرة آلاف في المائة"` | A1: reference spelling |
| [nemo-ar-measure-0010](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/ar/data_text_normalization/test_cases_measure.txt#L10) | `"%6798,9"` | `"ستة اَلاف وسبع مئة وثمانية وتسعون وتسعة من عشرة في المائة"` | `"ستة آلاف وسبع مئة وثمانية وتسعون وتسعة من عشرة في المائة"` | A1: reference spelling |
| [nemo-ar-money-0001](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/ar/data_text_normalization/test_cases_money.txt#L1) | `"$7"` | `"سبعة دولار"` | `"سبعة دولارات"` | variant: arabic_regional_currency_unit_singular |
| [nemo-ar-money-0002](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/ar/data_text_normalization/test_cases_money.txt#L2) | `"$7.50"` | `"سبعة دولار وخمسون سنت"` | `"سبعة دولارات وخمسون سنتا"` | A2: formal agreement / register review |
| [nemo-ar-money-0003](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/ar/data_text_normalization/test_cases_money.txt#L3) | `"$7.05"` | `"سبعة دولار وخمسة سنتات"` | `"سبعة دولارات وخمسة سنتات"` | A2: formal agreement / register review |
| [nemo-ar-money-0004](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/ar/data_text_normalization/test_cases_money.txt#L4) | `"$7.99"` | `"سبعة دولار وتسعة وتسعون سنت"` | `"سبعة دولارات وتسعة وتسعون سنتا"` | A2: formal agreement / register review |
| [nemo-ar-money-0005](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/ar/data_text_normalization/test_cases_money.txt#L5) | `"$7.10"` | `"سبعة دولار وعشرة سنتات"` | `"سبعة دولارات وعشرة سنتات"` | A2: formal agreement / register review |
| [nemo-ar-money-0006](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/ar/data_text_normalization/test_cases_money.txt#L6) | `"$0.99"` | `"تسعة وتسعون سنت"` | `"تسعة وتسعون سنتا"` | A2: formal agreement / register review |
| [nemo-ar-money-0007](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/ar/data_text_normalization/test_cases_money.txt#L7) | `"€0.99"` | `"تسعة وتسعون سنت"` | `"تسعة وتسعون سنتا"` | A2: formal agreement / register review |
| [nemo-ar-money-0009](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/ar/data_text_normalization/test_cases_money.txt#L9) | `"$9.09"` | `"تسعة دولار وتسعة سنتات"` | `"تسعة دولارات وتسعة سنتات"` | A2: formal agreement / register review |
| [nemo-ar-money-0010](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/ar/data_text_normalization/test_cases_money.txt#L10) | `"5د.ك"` | `"خمسة دينار كويتي"` | `"خمسة دنانير كويتية"` | variant: arabic_regional_currency_unit_singular |
| [nemo-de-date-0001](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/de/data_text_normalization/test_cases_date.txt#L1) | `"24. juli 2013"` | `"vier und zwanzigster juli zwei tausend dreizehn"` | `"vierundzwanzigster Juli zweitausenddreizehn"` | variant: `german_numeric_case`, `german_compound_spacing` |
| [nemo-de-date-0002](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/de/data_text_normalization/test_cases_date.txt#L2) | `"14. januar"` | `"vierzehnter januar"` | `"vierzehnter Januar"` | variant: `german_numeric_case` |
| [nemo-de-date-0003](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/de/data_text_normalization/test_cases_date.txt#L3) | `"1. januar"` | `"erster januar"` | `"erster Januar"` | variant: `german_numeric_case` |
| [nemo-de-date-0004](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/de/data_text_normalization/test_cases_date.txt#L4) | `"30. juni"` | `"dreißigster juni"` | `"dreißigster Juni"` | variant: `german_numeric_case` |
| [nemo-de-date-0005](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/de/data_text_normalization/test_cases_date.txt#L5) | `"02.03.2003"` | `"zweiter märz zwei tausend drei"` | `"zweiter März zweitausenddrei"` | variant: `german_numeric_case`, `german_compound_spacing` |
| [nemo-de-date-0006](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/de/data_text_normalization/test_cases_date.txt#L6) | `"2.3.2003"` | `"zweiter märz zwei tausend drei"` | `"zweiter März zweitausenddrei"` | variant: `german_numeric_case`, `german_compound_spacing` |
| [nemo-de-date-0007](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/de/data_text_normalization/test_cases_date.txt#L7) | `"2.3"` | `"zweiter märz"` | `"zweiter März"` | variant: `german_numeric_case` |
| [nemo-de-date-0008](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/de/data_text_normalization/test_cases_date.txt#L8) | `"02.03"` | `"zweiter märz"` | `"zweiter März"` | variant: `german_numeric_case` |
| [nemo-de-date-0009](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/de/data_text_normalization/test_cases_date.txt#L9) | `"2. März 2003"` | `"zweiter märz zwei tausend drei"` | `"zweiter März zweitausenddrei"` | variant: `german_numeric_case`, `german_compound_spacing` |
| [nemo-de-date-0010](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/de/data_text_normalization/test_cases_date.txt#L10) | `"2. Mär. 2003"` | `"zweiter märz zwei tausend drei"` | `"zweiter März zweitausenddrei"` | variant: `german_numeric_case`, `german_compound_spacing` |
| [nemo-de-measure-0001](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/de/data_text_normalization/test_cases_measure.txt#L1) | `"200 m"` | `"zwei hundert meter"` | `"zweihundert Meter"` | variant: `german_numeric_case`, `german_compound_spacing` |
| [nemo-de-measure-0002](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/de/data_text_normalization/test_cases_measure.txt#L2) | `"56,3 /km²"` | `"sechs und fünfzig komma drei pro quadrat kilometer"` | `"sechsundfünfzig Komma drei pro Quadratkilometer"` | variant: `german_numeric_case`, `german_compound_spacing`, `german_unit_compound_spacing` |
| [nemo-de-measure-0003](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/de/data_text_normalization/test_cases_measure.txt#L3) | `"200 km/h"` | `"zwei hundert kilometer pro stunde"` | `"zweihundert Kilometer pro Stunde"` | variant: `german_numeric_case`, `german_compound_spacing` |
| [nemo-de-measure-0004](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/de/data_text_normalization/test_cases_measure.txt#L4) | `"42259 /m²"` | `"zwei und vierzig tausend zwei hundert neun und fünfzig pro quadrat meter"` | `"zweiundvierzigtausendzweihundertneunundfünfzig pro Quadratmeter"` | variant: `german_numeric_case`, `german_compound_spacing`, `german_unit_compound_spacing` |
| [nemo-de-measure-0005](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/de/data_text_normalization/test_cases_measure.txt#L5) | `"42259/m²"` | `"zwei und vierzig tausend zwei hundert neun und fünfzig pro quadrat meter"` | `"zweiundvierzigtausendzweihundertneunundfünfzig pro Quadratmeter"` | variant: `german_numeric_case`, `german_compound_spacing`, `german_unit_compound_spacing` |
| [nemo-de-measure-0006](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/de/data_text_normalization/test_cases_measure.txt#L6) | `"-66 kg"` | `"minus sechs und sechzig kilogramm"` | `"minus sechsundsechzig Kilogramm"` | variant: `german_numeric_case`, `german_compound_spacing` |
| [nemo-de-measure-0007](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/de/data_text_normalization/test_cases_measure.txt#L7) | `"2 kwh"` | `"zwei kilowattstunden"` | `"zwei Kilowattstunden"` | variant: `german_numeric_case` |
| [nemo-de-measure-0008](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/de/data_text_normalization/test_cases_measure.txt#L8) | `"1,000028 cm³"` | `"eins komma null null null null zwei acht kubik zentimeter"` | `"eins Komma null null null null zwei acht Kubikzentimeter"` | variant: `german_numeric_case`, `german_unit_compound_spacing` |
| [nemo-de-measure-0009](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/de/data_text_normalization/test_cases_measure.txt#L9) | `"1,1 cm"` | `"eins komma eins zentimeter"` | `"eins Komma eins Zentimeter"` | variant: `german_numeric_case` |
| [nemo-de-measure-0010](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/de/data_text_normalization/test_cases_measure.txt#L10) | `"3 h"` | `"drei stunden"` | `"drei Stunden"` | variant: `german_numeric_case` |
| [nemo-de-money-0005](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/de/data_text_normalization/test_cases_money.txt#L5) | `"$2,0001"` | `"zwei komma null null null eins dollar"` | `"zwei Komma null null null eins dollar"` | variant: `german_numeric_case` |
| [nemo-de-time-0007](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/de/data_text_normalization/test_cases_time.txt#L7) | `"24:00 Uhr"` | `"vier und zwanzig uhr"` | `"vierundzwanzig uhr"` | variant: `german_compound_spacing` |
| [nemo-en-cardinal-0007](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/en/data_text_normalization/test_cases_cardinal.txt#L7) | `"13000"` | `"one three zero zero zero"` | `"thirteen thousand"` | variant: `alternative_integer_digit_policy` |
| [nemo-en-cardinal-0010](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/en/data_text_normalization/test_cases_cardinal.txt#L10) | `"123,000"` | `"one hundred twenty three thousand"` | `"one hundred and twenty three thousand"` | variant: `english_hundred_and` |
| [nemo-en-measure-0007](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/en/data_text_normalization/test_cases_measure.txt#L7) | `"3 cc/s"` | `"three CC per S"` | `"three cubic centimeters per second"` | P1: unit output convention |
| [nemo-en-time-0006](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/en/data_text_normalization/test_cases_time.txt#L6) | `"23:00"` | `"twenty three o'clock"` | `"twenty three hundred"` | E1: reference clock wording |
| [nemo-en-measure-0010](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/en/data_text_normalization/test_cases_measure.txt#L10) | `"covid-19.5"` | `"covid- nineteen point five"` | `"covid nineteen point five"` | variant: `english_literal_label_separator` |
| [nemo-en-money-0003](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/en/data_text_normalization/test_cases_money.txt#L3) | `"₩460 billion"` | `"four hundred sixty billion won"` | `"four hundred and sixty billion won"` | variant: `english_hundred_and` |
| [nemo-es-date-0009](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/es/data_text_normalization/test_cases_date.txt#L9) | `"30-2-1990"` | `"treinta de febrero de mil novecientos noventa"` | `"30-2-1990"` | D1: invalid calendar date |
| [nemo-es-date-0010](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/es/data_text_normalization/test_cases_date.txt#L10) | `"30/2/1990"` | `"treinta de febrero de mil novecientos noventa"` | `"30/2/1990"` | D1: invalid calendar date |
| [nemo-es-measure-0010](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/es/data_text_normalization/test_cases_measure.txt#L10) | `"1 1/2 mph"` | `"una y media milla por hora"` | `"una milla y media por hora"` | S1: corrected mixed-half wording / reference review |
| [nemo-es-time-0005](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/es/data_text_normalization/test_cases_time.txt#L5) | `"3 h"` | `"tres horas"` | `"tres"` | variant: `spanish_whole_hour_noun` |
| [nemo-es-time-0006](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/es/data_text_normalization/test_cases_time.txt#L6) | `"1 h"` | `"una hora"` | `"una"` | variant: `spanish_whole_hour_noun` |
| [nemo-fr-date-0009](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/fr/data_text_normalization/test_cases_date.txt#L9) | `"les 80s"` | `"les eighties"` | `"les années quatre-vingt"` | P2: decade context / loanword |
| [nemo-hi-date-0002](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/hi/data_text_normalization/test_cases_date.txt#L2) | `"३१-०६"` | `"इकतीस जून"` | `"३१-०६"` | D1: invalid calendar date |
| [nemo-hi-measure-0003](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/hi/data_text_normalization/test_cases_measure.txt#L3) | `"20 km²"` | `"बीस वर्ग किलोमीटर "` | `"बीस वर्ग किलोमीटर"` | variant: `outer_whitespace` |
| [nemo-hi-time-0007](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/hi/data_text_normalization/test_cases_time.txt#L7) | `"8:00"` | `"आठ बजे "` | `"आठ बजे"` | variant: `outer_whitespace` |
| [nemo-ja-cardinal-0010](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/ja/data_text_normalization/test_cases_cardinal.txt#L10) | `"12345000"` | `"一千二百三十四万五千"` | `"千二百三十四万五千"` | variant: `japanese_initial_one_thousand` |
| [nemo-ko-date-0001](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/ko/data_text_normalization/test_cases_date.txt#L1) | `"2024년"` | `"이천이십사년 "` | `"이천이십사년"` | variant: `outer_whitespace` |
| [nemo-ko-date-0002](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/ko/data_text_normalization/test_cases_date.txt#L2) | `"3월"` | `"삼월 "` | `"삼월"` | variant: `outer_whitespace` |
| [nemo-ko-date-0010](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/ko/data_text_normalization/test_cases_date.txt#L10) | `"기원전128년"` | `"기원전 백이십팔년 "` | `"기원전 백이십팔년"` | variant: `outer_whitespace` |
| [nemo-ru-measure-0001](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/ru/data_text_normalization/test_cases_measure.txt#L1) | `"12 кг"` | `"двенадцать килограм"` | `"двенадцать килограмм"` | R1: reference spelling/agreement |
| [nemo-ru-measure-0002](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/ru/data_text_normalization/test_cases_measure.txt#L2) | `"12кг"` | `"двенадцать килограм"` | `"двенадцать килограмм"` | R1: reference spelling/agreement |
| [nemo-ru-measure-0006](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/ru/data_text_normalization/test_cases_measure.txt#L6) | `"4-х килограммовый мешок"` | `"четырех килограммовый мешок"` | `"четырёх килограммовый мешок"` | variant: `russian_numeric_yo` |
| [nemo-ru-measure-0008](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/ru/data_text_normalization/test_cases_measure.txt#L8) | `"4,2-килограммовый мешок"` | `"четыре целых два десятых килограммовый мешок"` | `"четыре целых две десятых килограммовый мешок"` | R1: reference spelling/agreement |
| [nemo-ru-money-0003](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/ru/data_text_normalization/test_cases_money.txt#L3) | `"31 руб."` | `"тридцать одного рубля"` | `"тридцать один рубль"` | R1: reference spelling/agreement |
| [nemo-ru-money-0004](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/ru/data_text_normalization/test_cases_money.txt#L4) | `"18000 долл. США"` | `"восемнадцать тысяч долларов сэ ш а"` | `"восемнадцать тысяч долларов сэ шэ а"` | R2: qualifier fixed / reference pronunciation |
| [nemo-ru-money-0007](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/ru/data_text_normalization/test_cases_money.txt#L7) | `"2,03 руб."` | `"две целых три сотых рублей"` | `"два рубля три копейки"` | R1: reference spelling/agreement |
| [nemo-ru-time-0001](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/ru/data_text_normalization/test_cases_time.txt#L1) | `"13:05"` | `"тринадцать часов пять минут"` | `"тринадцать часов и пять минут"` | variant: `russian_clock_conjunction` |
| [nemo-ru-time-0004](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/ru/data_text_normalization/test_cases_time.txt#L4) | `"01:00"` | `"один час ноль минут"` | `"один час"` | variant: `russian_clock_zero_minutes` |
| [nemo-ru-time-0006](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/ru/data_text_normalization/test_cases_time.txt#L6) | `"05:45"` | `"без пятнадцати минут шесть"` | `"пять часов и сорок пять минут"` | variant: `russian_clock_relative` |
| [nemo-ru-time-0007](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/ru/data_text_normalization/test_cases_time.txt#L7) | `"09:59"` | `"без одной минуты десять"` | `"девять часов и пятьдесят девять минут"` | variant: `russian_clock_relative` |
| [nemo-ru-time-0009](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/ru/data_text_normalization/test_cases_time.txt#L9) | `"08:07"` | `"семь минут девятого"` | `"восемь часов и семь минут"` | variant: `russian_clock_relative` |
| [nemo-vi-measure-0001](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/vi/data_text_normalization/test_cases_measure.txt#L1) | `"204m"` | `"hai trăm linh bốn mét"` | `"hai trăm lẻ bốn mét"` | variant: `vietnamese_zero_tens_linker` |
| [nemo-vi-measure-0002](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/vi/data_text_normalization/test_cases_measure.txt#L2) | `"12kg"` | `"mười hai ki lô gam"` | `"mười hai kilôgam"` | variant: `vietnamese_unit_component_spacing` |
| [nemo-vi-measure-0003](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/vi/data_text_normalization/test_cases_measure.txt#L3) | `"1kg"` | `"một ki lô gam"` | `"một kilôgam"` | variant: `vietnamese_unit_component_spacing` |
| [nemo-vi-measure-0005](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/vi/data_text_normalization/test_cases_measure.txt#L5) | `"500mg"` | `"năm trăm mi li gam"` | `"năm trăm miligam"` | variant: `vietnamese_unit_component_spacing` |
| [nemo-vi-measure-0006](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/vi/data_text_normalization/test_cases_measure.txt#L6) | `"175cm"` | `"một trăm bảy mươi lăm xăng ti mét"` | `"một trăm bảy mươi lăm xentimét"` | variant: `vietnamese_typed_unit_alias` |
| [nemo-vi-measure-0008](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/vi/data_text_normalization/test_cases_measure.txt#L8) | `"100mm"` | `"một trăm mi li mét"` | `"một trăm milimét"` | variant: `vietnamese_unit_component_spacing` |
| [nemo-vi-measure-0009](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/vi/data_text_normalization/test_cases_measure.txt#L9) | `"5km"` | `"năm ki lô mét"` | `"năm kilômét"` | variant: `vietnamese_unit_component_spacing` |
| [nemo-zh-time-0009](https://github.com/NVIDIA/NeMo-text-processing/blob/ddadfb2a38d2bc6b8cc6232c4f915eb60f500688/tests/nemo_text_processing/zh/data_text_normalization/test_cases_time.txt#L9) | `"13:10pm"` | `"十三点十分pm"` | `"13:10pm"` | D2: outside strict 12-hour AM/PM domain |
