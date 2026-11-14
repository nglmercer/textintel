use textintel::speech::normalization::{
    self as tn, NormalizationMode, NormalizeError, NormalizeLimits, NormalizeOptions, WarningCode,
};
use textintel::speech::{DateOrder, SpeechLocale, locales};

fn resolve(language: &str, locale: Option<&str>) -> Result<NormalizeOptions, NormalizeError> {
    NormalizeOptions::resolve(
        NormalizationMode::Auto,
        language,
        locale,
        NormalizeLimits::default(),
        true,
    )
}

#[test]
fn regional_registry_is_used_by_resolution_for_all_twenty_languages_and_italian() {
    for entry in locales::LANGUAGES {
        let default = resolve(entry.code, None).unwrap();
        assert_eq!(default.locale, entry.default_locale);
        assert!(!default.locale_explicit);
        for tag in entry.locales {
            let record = locales::for_locale(tag).unwrap();
            assert_eq!(record.language, entry.code);
            assert_eq!(record.locale, *tag);
            assert_eq!(record.decimal_separator, entry.decimal_separator);
            let explicit = resolve(entry.code, Some(&tag.to_lowercase())).unwrap();
            assert_eq!(explicit.locale, *tag);
            assert!(explicit.locale_explicit);
            assert!(tn::is_supported_locale(tag));
        }
    }
    for language in SpeechLocale::ALL {
        let entry = locales::for_language(language.code()).unwrap();
        assert!(locales::for_locale(entry.default_locale).is_some());
    }
    assert!(locales::for_locale("it-IT").is_some());
    assert_eq!(
        tn::normalize("42", &resolve("it", Some("it-IT")).unwrap())
            .unwrap()
            .spoken,
        "quarantadue"
    );
}

#[test]
fn english_regional_date_and_dollar_contrasts_come_from_the_same_record() {
    let us = locales::for_locale("en-US").unwrap();
    let gb = locales::for_locale("EN-gb").unwrap();
    assert_eq!(us.date_order, DateOrder::MonthDayYear);
    assert_eq!(gb.date_order, DateOrder::DayMonthYear);
    assert_eq!(us.dollar_currency, Some("USD"));
    assert_eq!(gb.dollar_currency, None);
    for (tag, month) in [("en-US", "March"), ("en-GB", "April")] {
        let opts = resolve("en", Some(tag)).unwrap();
        let date = tn::normalize("03/04/2024", &opts).unwrap();
        assert!(date.spoken.contains(month), "{tag}: {}", date.spoken);
    }
    assert_eq!(
        tn::normalize("$5", &resolve("en", Some("en-US")).unwrap())
            .unwrap()
            .spoken,
        "five dollars"
    );
    let gb = tn::normalize("$5", &resolve("en", Some("en-GB")).unwrap()).unwrap();
    assert_eq!(gb.spoken, "$5");
    assert!(
        gb.warnings
            .iter()
            .any(|w| w.code == WarningCode::AmbiguousCurrency)
    );
    let implicit = tn::normalize("03/04/2024", &resolve("en", None).unwrap()).unwrap();
    assert_eq!(implicit.spoken, "03/04/2024");
    assert!(
        implicit
            .warnings
            .iter()
            .any(|w| w.code == WarningCode::AmbiguousDate)
    );
}

#[test]
fn historical_year_final_cjk_policy_is_separate_from_year_first_defaults() {
    for (language, tag) in [("zh", "zh-CN"), ("ja", "ja-JP"), ("ko", "ko-KR")] {
        let record = locales::for_locale(tag).unwrap();
        assert_eq!(record.date_order, DateOrder::YearMonthDay);
        assert_eq!(record.legacy_year_final_date_order, DateOrder::MonthDayYear);
        let opts = resolve(language, Some(tag)).unwrap();
        let written = tn::normalize("03/04/2024", &opts).unwrap();
        let unambiguous = tn::normalize("2024/03/04", &opts).unwrap();
        assert_eq!(written.spoken, unambiguous.spoken, "{tag}");
        assert_ne!(written.spoken, "03/04/2024");
    }
}

#[test]
fn unsupported_regions_do_not_fall_back_and_typed_regional_scope_stays_explicit() {
    for tag in [
        "en-CA",
        "es-AR",
        "fr-CA",
        "nl-NL",
        "en-US-x-private",
        "en_US",
        "en",
        "zh-Hans",
    ] {
        assert!(locales::for_locale(tag).is_none(), "{tag}");
        assert!(!tn::is_supported_locale(tag), "{tag}");
    }
    assert!(matches!(
        resolve("en", Some("en-CA")),
        Err(NormalizeError::UnsupportedLocale)
    ));
    assert!(matches!(
        resolve("en", Some("fr-FR")),
        Err(NormalizeError::LocaleLanguageMismatch)
    ));
    for tag in ["en-GB", "pt-PT", "zh-TW", "it-IT"] {
        assert!(locales::for_locale(tag).is_some(), "{tag}");
        assert!(tag.parse::<SpeechLocale>().is_err(), "{tag}");
    }
    for tag in ["en-US", "pt-BR", "zh-Hans", "pa-Guru-IN"] {
        assert!(tag.parse::<SpeechLocale>().is_ok(), "{tag}");
    }
    let unknown = resolve("nl", None).unwrap();
    assert_eq!(unknown.locale, "nl");
    assert!(!unknown.locale_explicit);
    assert_eq!(
        tn::normalize("03/04/2024", &unknown).unwrap().spoken,
        "03/04/2024"
    );
}
