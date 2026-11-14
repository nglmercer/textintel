//! Gregorian calendar validation shared by entity extraction and speech.
/// Number of days in a month, including Gregorian leap-year rules.
pub fn days_in_month(year: u32, month: u32) -> Option<u32> {
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => Some(31),
        4 | 6 | 9 | 11 => Some(30),
        2 => Some(if leap { 29 } else { 28 }),
        _ => None,
    }
}
/// A real calendar date with a positive year representable in four digits.
pub fn valid_date(year: u32, month: u32, day: u32) -> bool {
    (1..=9999).contains(&year)
        && day >= 1
        && days_in_month(year, month).is_some_and(|max| day <= max)
}
