# UDHR translation profile attribution and permission

The seven profile files `languages/{bn,mr,pa,ta,te,ur,vi}.json` contain selected
paragraphs of the Universal Declaration of Human Rights (UDHR) translations.
The declaration and translations are attributed to the United Nations / Office
of the United Nations High Commissioner for Human Rights (OHCHR) and the
translation contributors acknowledged by the source project.

Source and reproduction permission:
[OHCHR UDHR Translation Project](https://www.ohchr.org/en/human-rights/universal-declaration/universal-declaration-human-rights/about-universal-declaration-human-rights-translation-project).
Its Permissions section states:

> If UDHR translations or materials are reproduced, users should make reference to this website as a source by providing a link.

The XML representation is from the
[UDHR in XML project](https://github.com/eric-muller/udhr), revision
`588b3f4b2d0467aff54842a4b926551b69d5a66a`. That project describes its purpose
as dissemination in a uniform, repurposable representation, and describes the
UDHR and its translations as part of the commons. The historical
`unicode-org/udhr` URL now redirects to this repository. Unicode stopped hosting
the project in January 2024. These translations are recorded under the UDHR
reproduction permission above; they are **not** relabelled as MIT or as Unicode
License v3 merely because the project was historically hosted by Unicode.

The pinned source history acknowledges these contributions:

- Bengali: Kumari Co Ltd. via the Language Observatory; corrections by Swarup,
  Atul Thakur and Steve White.
- Marathi: Kumari Co Ltd. via the Language Observatory; corrections by Sridatta A.
- Eastern Punjabi, Gurmukhi: complete OHCHR source converted to XML in 2008.
- Tamil, India variant: K. Kalyanasundaram; spelling corrections by Shriramana
  Sharma and Geetha Sambasivam; India-variant update by Elango Cheran via Sascha
  Brawer.
- Telugu: Unicode conversion by Theo Jayaraj Samuel via tradcom.de.
- Urdu: M Hasan Jowher and Werner Fröhlich; corrections by Saadat Mateen.
- Vietnamese: XML conversion and corrections by Adobe Systems.

Exact source/status paths, byte lengths and SHA-256 hashes are recorded in
[`udhr-provenance.json`](udhr-provenance.json). No United Nations emblem or
illustrations are included, and no endorsement is implied. OHCHR's project
notes that it does not guarantee the accuracy of translations outside the six
official UN languages; these profiles are not a fresh native-speaker audit.

## Fixed selection and held-out data

Selection was fixed before evaluating the new profiles:

1. Select every `para` under the preamble, then every `para` descendant of
   articles numbered 1–20, in source order. Exclude titles and headings.
2. Decode XML text and collapse whitespace to single spaces; otherwise retain
   the source spelling and script. Use paragraphs as `LanguagePack.examples`.
3. Keep articles 21–30 exclusively in
   [`data/language/udhr-heldout.json`](../data/language/udhr-heldout.json).
4. Retain the existing seven handwritten unseen prose challenges as held-out
   queries. No query-specific text is injected into the profiles.

This adds authentic connected prose and common grammatical words to the
existing profile mechanism. It does not introduce model files, neural code,
training code, a new detector, or a confidence-threshold adjustment. The UDHR
is a single legal domain; held-out articles from the same document cannot
establish general conversational or cross-domain language accuracy.
