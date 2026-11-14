//! Shared language registry for speech parsing and versioned normalization.
use super::types::DateOrder;

/// Default reading conventions and validated aliases for one language.
/// Regional callers use [`for_locale`] rather than assuming every alias has
/// the language default's date or currency convention.
#[derive(Debug, Clone, Copy)]
pub struct Language {
    pub code: &'static str,
    pub locales: &'static [&'static str],
    pub default_locale: &'static str,
    /// Aliases implemented by the typed renderer, including script aliases.
    pub typed_aliases: &'static [&'static str],
    pub decimal_separator: char,
    pub date_order: DateOrder,
    /// Retain this language's established versioned formatting adapter.
    pub has_versioned_adapter: bool,
}
macro_rules! language {
    (shared $code:literal, $locales:tt, $aliases:tt, $decimal:literal, $order:ident) => {
        language!(@entry false, $code, $locales, $aliases, $decimal, $order)
    };
    ($code:literal, [$($locale:literal),+], [$($alias:literal),*], $decimal:literal, $order:ident) => {
        language!(@entry true, $code, [$($locale),+], [$($alias),*], $decimal, $order)
    };
    (@entry $adapter:literal, $code:literal, [$($locale:literal),+], [$($alias:literal),*], $decimal:literal, $order:ident) => {
        Language {
            code: $code,
            locales: &[$($locale),+],
            default_locale: language!(@first $($locale),+),
            typed_aliases: &[$($alias),*],
            decimal_separator: $decimal,
            date_order: DateOrder::$order,
            has_versioned_adapter: $adapter,
        }
    };
    (@first $first:literal $(,$rest:literal)*) => {$first};
}
/// The twenty typed languages and the additional Italian versioned rules.
pub const LANGUAGES: &[Language] = &[
    language!(
        "es",
        ["es-ES", "es-MX", "es-PE"],
        ["es-ES", "es-MX", "es-PE"],
        ',',
        DayMonthYear
    ),
    language!("en", ["en-US", "en-GB"], ["en-US"], '.', MonthDayYear),
    language!("pt", ["pt-BR", "pt-PT"], ["pt-BR"], ',', DayMonthYear),
    language!("fr", ["fr-FR"], ["fr-FR"], ',', DayMonthYear),
    language!("de", ["de-DE"], ["de-DE"], ',', DayMonthYear),
    language!("it", ["it-IT"], [], ',', DayMonthYear),
    language!("ru", ["ru-RU"], ["ru-RU"], ',', DayMonthYear),
    language!(
        "zh",
        ["zh-CN", "zh-TW"],
        ["zh-CN", "zh-Hans", "zh-Hans-CN"],
        '.',
        YearMonthDay
    ),
    language!("ja", ["ja-JP"], ["ja-JP"], '.', YearMonthDay),
    language!("ko", ["ko-KR"], ["ko-KR"], '.', YearMonthDay),
    language!(shared "hi", ["hi-IN"], ["hi-IN"], '.', DayMonthYear),
    language!(shared "ar", ["ar-SA"], ["ar-SA"], '.', DayMonthYear),
    language!(shared "bn", ["bn-BD"], ["bn-BD"], '.', DayMonthYear),
    language!(shared "ur", ["ur-PK"], ["ur-PK"], '.', DayMonthYear),
    language!(shared "id", ["id-ID"], ["id-ID"], ',', DayMonthYear),
    language!(shared "pa",
        ["pa-IN"],
        ["pa-IN", "pa-Guru", "pa-Guru-IN"],
        '.',
        DayMonthYear
    ),
    language!(shared "mr", ["mr-IN"], ["mr-IN"], '.', DayMonthYear),
    language!(shared "te", ["te-IN"], ["te-IN"], '.', DayMonthYear),
    language!(shared "tr", ["tr-TR"], ["tr-TR"], ',', DayMonthYear),
    language!(shared "ta", ["ta-IN"], ["ta-IN"], '.', DayMonthYear),
    language!(shared "vi", ["vi-VN"], ["vi-VN"], ',', DayMonthYear),
];

pub fn for_language(code: &str) -> Option<&'static Language> {
    LANGUAGES
        .iter()
        .find(|entry| entry.code.eq_ignore_ascii_case(code))
}

/// Resolved reading conventions for one supported, concrete regional locale.
/// These are library interpretation policies, not a general locale database.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocaleConventions {
    pub locale: &'static str,
    pub language: &'static str,
    pub decimal_separator: char,
    pub grouping_separator: char,
    /// Default numeric calendar order for the locale, including year-first
    /// Chinese/Japanese/Korean conventions.
    pub date_order: DateOrder,
    /// Compatibility policy for ambiguous year-final slash dates in the
    /// versioned normalizer. CJK historically reads 01/02/2024 as MDY; this
    /// does not override its year-first calendar default.
    pub legacy_year_final_date_order: DateOrder,
    /// Whether a single three-digit separator group is unambiguous in prose.
    /// False retains the versioned API's conservative warning policy.
    pub separators_unambiguous: bool,
    /// ISO currency resolved by a bare `$`, when explicitly supported. This
    /// never implies that the country only uses this currency.
    pub dollar_currency: Option<&'static str>,
}

/// Look up a concrete locale without guessing from an unknown region or a
/// base language. Matching is ASCII case-insensitive; script-only typed
/// aliases are separate from the versioned API's concrete locale contract.
pub fn for_locale(tag: &str) -> Option<LocaleConventions> {
    let (language, locale) = LANGUAGES.iter().find_map(|entry| {
        entry
            .locales
            .iter()
            .find(|locale| locale.eq_ignore_ascii_case(tag))
            .map(|locale| (entry, *locale))
    })?;
    let date_order = if locale == "en-GB" {
        DateOrder::DayMonthYear
    } else {
        language.date_order
    };
    Some(LocaleConventions {
        locale,
        language: language.code,
        decimal_separator: language.decimal_separator,
        grouping_separator: match language.code {
            // Preserve the existing narrow-space grouping policy. Whitespace
            // field joining belongs to the front end, not this metadata.
            "fr" | "ru" => '\u{202f}',
            _ if language.decimal_separator == ',' => '.',
            _ => ',',
        },
        date_order,
        legacy_year_final_date_order: if matches!(language.code, "zh" | "ja" | "ko") {
            DateOrder::MonthDayYear
        } else {
            date_order
        },
        separators_unambiguous: matches!(language.code, "en" | "zh" | "ja" | "ko"),
        dollar_currency: (locale == "en-US").then_some("USD"),
    })
}
