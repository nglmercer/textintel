# Spam corpus v2

Realistic SMS spam/ham for training (`v2-train.json`, 240 items) and
held-out evaluation (`v2-eval.json`, 160 items) of `models/spam-v2.json`.

## Source

Subset of the SMS Spam Collection (Almeida, Hidalgo, Yamakami):

- T. A. Almeida, J. M. G. Hidalgo, A. Yamakami,
  "Contributions to the study of SMS spam filtering: new collection and
  benchmark results", ACM SAC 2011.
- UCI Machine Learning Repository, SMS Spam Collection (2011),
  https://archive.ics.uci.edu/dataset/228/sms+spam+collection
- Original labels by the collection authors (Grumbletext/NUS corpora).

The full collection (5,574 messages, 747 spam / 4,827 ham) is public for
research use. This subset (400 messages) is redistributed with attribution
for the same purpose.

## Sampling (deterministic, seed 20260915)

1. Exact-duplicate texts removed (5,160 unique remain).
2. Stratified shuffle; first 120 spam + 120 ham to train, next 80 + 80 to
   eval (seed 20260915).
3. Campaign de-duplication: messages sharing a digit-normalized template
   (same text modulo phone numbers, amounts, codes) collapse to one
   template per split, and no template appears in both splits
   (backfill seed 777). Train and eval are disjoint by text AND campaign,
   so eval measures generalization, not campaign memorization.

## Human review

Every eval item (160) and every train spam item was read by the dataset
author. Four items were excluded with reasons (see below); eight
backfilled replacements were read and approved. Automated checks enforce:
valid JSON schema, non-empty text, disjoint texts/templates across splits,
balanced labels (50/50 per split).

Exclusions:

- `dating:i have had two of these...` (spam) — ambiguous: a complaint
  ABOUT spam, not spam itself. Dropped rather than relabeled.
- `Men always needs a beautiful...` (ham) — taste: gratuitous joke, no
  coverage value. Replaced with another ham.
- `Fuck babe ...` (ham) — taste: profanity with no signal value.
  Replaced with another ham.
- `Come to me, slave...` (ham) — taste: edgy roleplay with no signal
  value. Replaced with another ham.

Kept deliberately (adversarial but fair): ham with money/exclamation
signals (`$700 or $900 for 5 nights...`, `Hey! I want you!...`), chain
prayer text, truncated spam, `?`-mojibake currency, and annotation
artifacts (`ROMCAPspam`, `Phony ...`) — all realistic noise a filter
must handle.

## Format

```json
{
  "version": "2.0.0",
  "source": "uci-sms-spam-collection-subset",
  "items": [{"id": "spam-tr-001", "text": "...", "label": "spam"}]
}
```

Labels are `spam` or `ham` (maps to benign). Item ids are stable.

## Usage

Train: `textintel-train spam --corpus data/spam/v2-train.json
--output models/spam-v2.json`

Evaluate (automatic in production eval): the evaluator scores
`data/spam/v2-eval.json` (sibling of the dataset directory) with the
production spam scorer and reports ROC-AUC / F1 / Brier / ECE, which the
production quality gates enforce.
