use textintel::speech::{DateOrder, SpeechLocale, SpeechOptions};

#[test]
fn locale_variants_are_explicit_and_do_not_silently_change_script_or_number_scale() {
    assert_eq!("es-PE".parse::<SpeechLocale>().unwrap(), SpeechLocale::Es);
    assert_eq!("PT_br".parse::<SpeechLocale>().unwrap(), SpeechLocale::Pt);
    assert_eq!(
        "pa-Guru-IN".parse::<SpeechLocale>().unwrap(),
        SpeechLocale::Pa
    );
    assert_eq!("zh-Hans".parse::<SpeechLocale>().unwrap(), SpeechLocale::Zh);
    for tag in ["pt-PT", "pa-PK", "zh-Hant", "fr-CH", "es-unknown", "xx", ""] {
        assert!(tag.parse::<SpeechLocale>().is_err(), "{tag}");
    }
    let options = SpeechOptions::new(SpeechLocale::Es);
    assert_eq!(options.date_order, DateOrder::RejectAmbiguous);
    assert_eq!(options.decimal_separator, None);
}
