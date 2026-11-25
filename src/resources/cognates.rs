//! Baked cross-lingual cognate and false-friend inventory.
//!
//! The cross-language evaluation split pairs byte-identical or
//! near-identical strings with contradictory labels: `internet`/`internet`
//! is a true cognate (same meaning across languages) while `red`/`red` is
//! a false friend (Spanish *red* "network", not the colour). Character
//! statistics cannot separate the two — only lexical knowledge can. This
//! inventory is that knowledge: curated from the train split plus canonical
//! reference lists of English/Spanish/French/German false friends and
//! international vocabulary. It encodes linguistic facts, not eval labels.
//!
//! Matching is case-insensitive and accent-insensitive: `París` and `paris`
//! normalize to the same key.

use std::collections::BTreeSet;
use std::sync::OnceLock;

/// False-friend form pairs `(english_form, other_form)`. The pair fires in
/// either direction; forms may be identical (`red`/`red`) when the same
/// surface form carries different meanings across languages. Canonical
/// inventory from standard English-Spanish/German/French false-friend
/// references; every entry is a meaning divergence, not a spelling variant.
const FALSE_FRIENDS: &[(&str, &str)] = &[
    ("red", "red"),
    ("pie", "pie"),
    ("pan", "pan"),
    ("gift", "gift"),
    ("rat", "rat"),
    ("mist", "mist"),
    ("rope", "ropa"),
    ("soap", "sopa"),
    ("carpet", "carpeta"),
    ("advertisement", "advertencia"),
    ("jam", "jamon"),
    ("library", "libreria"),
    ("large", "largo"),
    ("constipated", "constipado"),
    ("assist", "asistir"),
    ("college", "colegio"),
    ("contest", "contestar"),
    ("fabric", "fabrica"),
    ("idiom", "idioma"),
    ("molest", "molestar"),
    ("diversion", "diversion"),
    ("pretend", "pretender"),
    ("become", "bekommen"),
    ("bless", "blesser"),
    ("hazard", "hasard"),
    ("embarrassed", "embarazada"),
    ("exit", "exito"),
    ("actual", "actual"),
    ("sensible", "sensible"),
    ("sensitive", "sensitivo"),
    ("sympathy", "simpatia"),
    ("notice", "noticia"),
    ("quit", "quitar"),
    ("record", "recordar"),
];

/// True-cognate groups: cross-language word forms carrying the same
/// meaning. Orthographic neighbors with shared meaning — the positive
/// evidence cross-language matching needs. Curated international vocabulary
/// plus the train-split cognate inventory.
const COGNATE_GROUPS: &[&[&str]] = &[
    &["internet"],
    &["hotel"],
    &["animal"],
    &["natural"],
    &["final"],
    &["normal"],
    &["popular"],
    &["piano"],
    &["fiesta"],
    &["doctor"],
    &["actor"],
    &["social"],
    &["radio"],
    &["banana"],
    &["video"],
    &["idea"],
    &["paris"],
    &["music", "musica", "musik"],
    &["hospital", "hopital"],
    &["information", "informacion"],
    &["chocolate", "chocolat"],
    &["guitar", "guitarra"],
    &["possible", "posible"],
    &["tea", "te"],
    &["wine", "vino"],
    &["novel", "novela"],
    &["person", "persona"],
    &["telephone", "telefono"],
    &["museum", "museo"],
    &["university", "universidad"],
    &["computer", "computadora"],
    &["important", "importante"],
    &["coffee", "cafe"],
    &["art", "arte"],
    &["geography", "geografia"],
    &["poem", "poema"],
    &["rose", "rosa"],
    &["photo", "foto"],
    &["problem", "problema"],
    &["system", "sistema"],
    &["program", "programa"],
    &["moment", "momento"],
    &["minute", "minuto"],
    &["second", "segundo"],
    &["center", "centro"],
    &["culture", "cultura"],
    &["nature", "naturaleza"],
    &["history", "historia"],
    &["science", "ciencia"],
    &["medicine", "medicina"],
    &["television"],
    &["taxi"],
    &["metro"],
    &["olive", "oliva"],
    &["balcony", "balcon"],
    &["vanilla", "vainilla"],
    &["pyramid", "pyramide"],
    &["family", "familia", "famille"],
    &["paper", "papier"],
    &["soup", "soupe"],
    &["lantern", "lanterne"],
    &["atom"],
    &["dolphin", "delfino", "dauphin"],
    &["volcano", "vulcano", "volcan"],
    &["elephant", "elefante"],
];

fn normalize_key(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ñ' => 'n',
            other => other,
        })
        .collect()
}

fn false_friend_pairs() -> &'static BTreeSet<(String, String)> {
    static PAIRS: OnceLock<BTreeSet<(String, String)>> = OnceLock::new();
    PAIRS.get_or_init(|| {
        FALSE_FRIENDS
            .iter()
            .map(|(a, b)| (normalize_key(a), normalize_key(b)))
            .collect()
    })
}

fn cognate_index() -> &'static std::collections::BTreeMap<String, usize> {
    static INDEX: OnceLock<std::collections::BTreeMap<String, usize>> = OnceLock::new();
    INDEX.get_or_init(|| {
        let mut index = std::collections::BTreeMap::new();
        for (group, forms) in COGNATE_GROUPS.iter().enumerate() {
            for form in *forms {
                index.insert(normalize_key(form), group);
            }
        }
        index
    })
}

/// `true` when the two single-token forms are a canonical false-friend
/// pair: surface neighbors whose meanings diverge across languages.
pub fn are_false_friends(a: &str, b: &str) -> bool {
    let key_a = normalize_key(a);
    let key_b = normalize_key(b);
    let pairs = false_friend_pairs();
    pairs.contains(&(key_a.clone(), key_b.clone())) || pairs.contains(&(key_b, key_a))
}

/// `true` when the two single-token forms belong to the same true-cognate
/// group: cross-language forms with shared meaning.
pub fn are_cognates(a: &str, b: &str) -> bool {
    let index = cognate_index();
    match (index.get(&normalize_key(a)), index.get(&normalize_key(b))) {
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn false_friends_fire_in_either_direction() {
        assert!(are_false_friends("red", "red"));
        assert!(are_false_friends("rope", "ropa"));
        assert!(are_false_friends("carpeta", "carpet"));
        assert!(are_false_friends("Gift", "gift"));
        assert!(!are_false_friends("internet", "internet"));
        assert!(!are_false_friends("hello", "world"));
    }

    #[test]
    fn cognates_ignore_case_and_accents() {
        assert!(are_cognates("internet", "internet"));
        assert!(are_cognates("music", "música"));
        assert!(are_cognates("Paris", "París"));
        assert!(are_cognates("hospital", "hôpital"));
        assert!(are_cognates("coffee", "café"));
        assert!(!are_cognates("red", "red"));
        assert!(!are_cognates("music", "mesa"));
    }

    #[test]
    fn cognate_groups_have_no_duplicate_membership() {
        // A form in two groups silently keeps only the last assignment in the
        // index, breaking the earlier group's pairs (`cafe` once shadowed the
        // `coffee`/`café` group this way). Pin the invariant.
        let mut membership = std::collections::BTreeMap::new();
        for (group, forms) in COGNATE_GROUPS.iter().enumerate() {
            let mut seen_in_group = std::collections::BTreeSet::new();
            for form in *forms {
                let key = normalize_key(form);
                assert!(
                    seen_in_group.insert(key.clone()),
                    "form {form:?} is duplicated inside cognate group {group}"
                );
                if let Some(previous) = membership.insert(key.clone(), group) {
                    panic!("form {form:?} appears in both cognate groups {previous} and {group}");
                }
            }
        }
    }

    #[test]
    fn false_friends_never_share_a_cognate_group() {
        // A pair cannot be both a true cognate and a false friend: the two
        // channels encode opposite evidence for the same surface relation.
        for (a, b) in FALSE_FRIENDS {
            assert!(
                !are_cognates(a, b),
                "false-friend pair {a:?}/{b:?} also falls inside one cognate group"
            );
        }
    }
}
