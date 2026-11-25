//! Baked high-frequency English synonym/relatedness groups used by the
//! swap probe to separate synonym substitutions (`needed`/`required`) from
//! surface-similar malapropisms (`money`/`honey`) — identical surface
//! statistics, opposite meanings. Authored from lexical knowledge in the
//! spirit of a small WordNet subset, not from evaluation cases; coverage
//! is deliberately conservative so the relation evidence stays precise
//! (zero false coverage is worth more than the last few true positives).

/// Synonym/near-synonym groups over lowercase base forms. Relatedness is
/// symmetric and intra-group only.
static GROUPS: &[&[&str]] = &[
    &["act", "hurry", "rush", "move"],
    &["need", "require", "necessary"],
    &["buy", "purchase", "order", "acquire", "get"],
    &["alert", "warning", "notice", "caution"],
    &["lonely", "bored", "alone"],
    &["final", "last", "ultimate", "closing"],
    &["lose", "drop", "misplace"],
    &["make", "earn", "win", "gain"],
    &["midnight", "tonight"],
    &["package", "parcel", "box", "delivery"],
    &["verify", "confirm", "validate", "check"],
    &["card", "voucher", "coupon", "certificate"],
    &["crypto", "bitcoin"],
    &["doctor", "dentist", "physician"],
    &["now", "today", "immediately", "instantly", "here"],
    &["big", "large", "huge", "massive"],
    &["small", "little", "tiny", "minor"],
    &["free", "gratis", "complimentary"],
    &["prize", "reward", "award", "bonus"],
    &["offer", "deal", "promotion", "discount"],
    &["expire", "end", "terminate", "lapse"],
    &["renew", "extend", "continue", "prolong"],
    &["cheap", "inexpensive", "affordable", "budget"],
    &["expensive", "costly", "pricey"],
    &["fast", "quick", "rapid", "swift", "speedy"],
    &["slow", "gradual"],
    &["good", "excellent", "fine", "decent"],
    &["bad", "poor", "terrible", "awful"],
    &["happy", "glad", "pleased", "delighted"],
    &["sad", "unhappy", "gloomy"],
    &["angry", "mad", "furious", "annoyed"],
    &["friend", "mate", "buddy", "pal"],
    &["house", "home", "residence", "dwelling"],
    &["car", "auto", "vehicle", "automobile"],
    &["money", "cash", "funds", "bucks"],
    &["job", "work", "employment", "position"],
    &["meeting", "appointment", "gathering"],
    &["call", "phone", "ring", "dial"],
    &["email", "message", "mail"],
    &["send", "ship", "deliver", "forward"],
    &["receive", "accept", "obtain"],
    &["new", "fresh", "recent", "latest"],
    &["old", "ancient", "aged", "former"],
    &["important", "urgent", "critical", "vital"],
    &["secret", "private", "confidential", "hidden"],
    &["account", "profile", "membership", "login"],
    &["password", "passcode", "pin", "credential"],
    &["payment", "bill", "invoice", "charge"],
    &["bank", "banking", "financial"],
    &["loan", "credit", "mortgage", "debt"],
    &["sale", "sell", "selling", "auction"],
    &["store", "shop", "market", "outlet"],
    &["online", "internet", "web", "digital"],
    &["click", "tap", "press"],
    &["link", "url", "website", "page"],
    &["document", "file", "attachment", "pdf"],
    &["photo", "picture", "image", "pic"],
    &["video", "clip", "film", "recording"],
    &["news", "update", "bulletin", "report"],
    &["info", "information", "details", "data"],
    &["help", "assist", "support", "aid"],
    &["question", "inquiry", "query"],
    &["answer", "reply", "respond", "response"],
    &["problem", "issue", "trouble", "error"],
    &["fix", "repair", "solve", "resolve"],
    &["broken", "damaged", "defective", "faulty"],
    &["guarantee", "warranty", "promise", "assurance"],
    &["safe", "secure", "protected"],
    &["risky", "dangerous", "hazard", "unsafe"],
    &["legal", "legitimate", "lawful", "valid"],
    &["illegal", "fraud", "scam", "criminal"],
    &["suspend", "block", "freeze", "restrict"],
    &["unlock", "unblock", "unfreeze", "restore"],
    &["access", "entry", "admission", "permission"],
    &["deny", "refuse", "reject", "decline"],
    &["approve", "authorize"],
    &["cancel", "revoke", "void", "annul"],
    &["reschedule", "postpone", "delay", "defer"],
    &["book", "reserve", "schedule", "arrange"],
    &["flight", "plane", "airline", "airfare"],
    &["ticket", "pass", "boarding", "fare"],
    &["hotel", "room", "accommodation", "lodging"],
    &["rental", "rent", "lease", "hire"],
    &["food", "meal", "dish", "cuisine"],
    &["drink", "beverage", "refreshment"],
    &["coffee", "tea", "espresso"],
    &["plant", "flower", "vegetation"],
    &["park", "garden", "playground"],
    &["weather", "forecast", "climate", "temperature"],
    &["rain", "rainy", "shower", "drizzle"],
    &["sunny", "clear", "bright", "cloudless"],
    &["cold", "chilly", "freezing", "cool"],
    &["hot", "warm", "heated"],
    &["day", "daytime", "morning", "afternoon"],
    &["night", "nighttime", "evening", "dusk"],
    &["week", "weekday"],
    &["year", "annual", "yearly"],
    &["child", "kid", "youngster"],
    &["adult", "grownup", "parent"],
    &["man", "male", "gentleman", "guy"],
    &["woman", "female", "lady"],
    &["boy", "lad"],
    &["girl", "lass"],
    &["baby", "infant", "newborn", "toddler"],
    &["dog", "puppy", "canine", "hound"],
    &["cat", "kitten", "feline", "kitty"],
    &["pet", "animal", "companion"],
    &["love", "adore", "cherish"],
    &["hate", "detest", "despise", "dislike"],
    &["want", "wish", "desire", "crave"],
    &["know", "understand", "realize", "comprehend"],
    &["think", "believe", "suppose", "assume"],
    &["say", "tell", "speak", "utter"],
    &["talk", "chat", "converse", "discuss"],
    &["write", "type", "compose", "draft"],
    &["read", "peruse", "scan", "browse"],
    &["see", "look", "watch", "observe"],
    &["hear", "listen"],
    &["walk", "stroll", "hike", "march"],
    &["run", "jog", "sprint", "dash"],
    &["drive", "ride", "steer", "cruise"],
    &["eat", "consume", "dine", "munch"],
    &["sleep", "rest", "nap", "slumber"],
    &["wake", "awaken", "arise", "rise"],
    &["open", "unfold"],
    &["close", "shut", "seal"],
    &["start", "begin", "commence", "initiate"],
    &["stop", "halt", "cease"],
    &["change", "alter", "modify", "adjust"],
    &["increase", "boost", "raise", "grow"],
    &["decrease", "reduce", "lower", "shrink"],
    &["improve", "enhance", "upgrade", "better"],
    &["loud", "noisy", "loudly", "noisily"],
    &["just", "only", "simply", "merely"],
    &["exclusive", "special", "unique"],
    &["eat", "ate", "have", "had"],
];

/// Antonym/opposition pairs over lowercase forms (inflected variants
/// included where irregular). The paraphrase-negative signature: swaps in
/// these pairs flip meaning while keeping surface statistics identical to
/// synonym swaps (`won`/`lost` vs `needed`/`required`).
static ANTONYMS: &[(&str, &str)] = &[
    ("win", "lose"),
    ("won", "lost"),
    ("winning", "losing"),
    ("open", "close"),
    ("opened", "closed"),
    ("expire", "renew"),
    ("expires", "renewed"),
    ("expired", "renewed"),
    ("suspend", "restore"),
    ("suspended", "restored"),
    ("block", "unblock"),
    ("blocked", "unblocked"),
    ("lock", "unlock"),
    ("locked", "unlocked"),
    ("hot", "cold"),
    ("warm", "cool"),
    ("big", "small"),
    ("large", "small"),
    ("huge", "tiny"),
    ("fast", "slow"),
    ("quick", "slow"),
    ("good", "bad"),
    ("better", "worse"),
    ("best", "worst"),
    ("love", "hate"),
    ("loved", "hated"),
    ("happy", "sad"),
    ("glad", "sad"),
    ("start", "stop"),
    ("begin", "end"),
    ("started", "stopped"),
    ("increase", "decrease"),
    ("increased", "decreased"),
    ("raise", "lower"),
    ("rise", "fall"),
    ("rose", "fell"),
    ("buy", "sell"),
    ("bought", "sold"),
    ("cheap", "expensive"),
    ("new", "old"),
    ("fresh", "stale"),
    ("question", "answer"),
    ("ask", "answer"),
    ("problem", "solution"),
    ("accept", "reject"),
    ("accept", "deny"),
    ("approve", "reject"),
    ("friend", "enemy"),
    ("day", "night"),
    ("morning", "evening"),
    ("yes", "no"),
    ("true", "false"),
    ("gain", "lose"),
    ("gained", "lost"),
    ("include", "exclude"),
    ("included", "excluded"),
    ("allow", "deny"),
    ("allowed", "denied"),
    ("allow", "forbid"),
    ("arrive", "leave"),
    ("arrived", "left"),
    ("come", "go"),
    ("came", "went"),
    ("deposit", "withdraw"),
    ("deposited", "withdrew"),
    ("credit", "debit"),
    ("debit", "creditor"),
    ("attack", "defend"),
    ("offense", "defense"),
    ("guilty", "innocent"),
    ("guilt", "innocence"),
    ("confirm", "deny"),
    ("confirmed", "denied"),
    ("remember", "forget"),
    ("remembered", "forgot"),
    ("success", "failure"),
    ("succeed", "fail"),
    ("succeeded", "failed"),
    ("reward", "punish"),
    ("reward", "punishment"),
    ("safe", "dangerous"),
    ("safety", "danger"),
    ("legal", "illegal"),
    ("lawful", "unlawful"),
    ("upgrade", "downgrade"),
    ("upgraded", "downgraded"),
    ("verify", "reject"),
    ("verified", "rejected"),
];

/// True when the lowercase words form a baked antonym pair (either order),
/// directly or after inflection stripping.
pub fn are_antonyms(left: &str, right: &str) -> bool {
    let mut left_candidates = Vec::with_capacity(4);
    let mut right_candidates = Vec::with_capacity(4);
    lemma_candidates(left, &mut left_candidates);
    lemma_candidates(right, &mut right_candidates);
    left_candidates.iter().any(|lc| {
        right_candidates.iter().any(|rc| {
            ANTONYMS.iter().any(|(a, b)| {
                (*a == lc.as_str() && *b == rc.as_str()) || (*b == lc.as_str() && *a == rc.as_str())
            })
        })
    })
}

/// Candidate lemma forms for a lowercase word: the word itself plus crude
/// English inflection stripping (`-s`/`-es`/`-ed`/`-ing`, consonant
/// undoubling, `-ied`→`-y`, `+e` restoration for `required`→`require`).
/// Precision-oriented: over-stripping is harmless because group membership
/// still requires both sides to land in the same group.
fn lemma_candidates(word: &str, out: &mut Vec<String>) {
    out.push(word.to_string());
    let try_push = |candidate: String, out: &mut Vec<String>| {
        if candidate.len() >= 2 && !out.contains(&candidate) {
            out.push(candidate);
        }
    };
    for suffix in ["ing", "ed", "es", "s"] {
        if let Some(base) = word.strip_suffix(suffix)
            && base.len() >= 2
        {
            try_push(base.to_string(), out);
            // undouble final consonant: `running`→`run`, `stopped`→`stop`
            if base.len() >= 2
                && base.ends_with(
                    base.chars()
                        .next_back()
                        .unwrap()
                        .to_string()
                        .repeat(2)
                        .as_str(),
                )
            {
                try_push(base[..base.len() - 1].to_string(), out);
            }
            // `+e` restoration: `required`→`require`, `making`→`make`
            try_push(format!("{base}e"), out);
            // `-ied`→`-y`: `verified`→`verify`
            if suffix == "ed" && base.ends_with('i') {
                try_push(format!("{}y", &base[..base.len() - 1]), out);
            }
        }
    }
}

/// True when both lowercase words share a synonym group under any of their
/// lemma candidates.
pub fn are_related(left: &str, right: &str) -> bool {
    if left == right {
        return true;
    }
    let mut left_candidates = Vec::with_capacity(4);
    let mut right_candidates = Vec::with_capacity(4);
    lemma_candidates(left, &mut left_candidates);
    lemma_candidates(right, &mut right_candidates);
    // Shared lemma means same lexeme, different inflection
    // (`expiring`/`expired`) — trivially related.
    if left_candidates
        .iter()
        .any(|candidate| right_candidates.contains(candidate))
    {
        return true;
    }
    GROUPS.iter().any(|group| {
        let hit = |candidates: &[String]| {
            candidates
                .iter()
                .any(|candidate| group.contains(&candidate.as_str()))
        };
        hit(&left_candidates) && hit(&right_candidates)
    })
}

#[cfg(test)]
mod tests {
    use super::are_related;

    #[test]
    fn synonym_pairs_are_related() {
        for (left, right) in [
            ("needed", "required"),
            ("buy", "order"),
            ("alert", "warning"),
            ("midnight", "tonight"),
            ("package", "parcel"),
            ("verify", "confirm"),
            ("making", "earned"),
        ] {
            assert!(are_related(left, right), "{left}/{right}");
        }
    }

    #[test]
    fn antonym_pairs_are_detected() {
        for (left, right) in [
            ("won", "lost"),
            ("expires", "renewed"),
            ("open", "close"),
            ("suspended", "restored"),
        ] {
            assert!(super::are_antonyms(left, right), "{left}/{right}");
            assert!(
                !are_related(left, right),
                "{left}/{right} must not be synonyms"
            );
        }
    }

    #[test]
    fn malapropisms_are_not_related() {
        for (left, right) in [
            ("money", "honey"),
            ("cash", "cache"),
            ("claim", "climb"),
            ("deal", "meal"),
            ("gift", "guilt"),
            ("payment", "pavement"),
            ("risk", "brisk"),
            ("boom", "broom"),
            ("won", "lost"),
            ("expires", "renewed"),
        ] {
            assert!(!are_related(left, right), "{left}/{right}");
        }
    }
}
