//! Public holiday calendars hosted by Thunderbird. They are ordinary public
//! iCalendar links, read through the same path and limits as any link.
//! Checked against https://www.thunderbird.net/en-US/calendar/holidays/ in
//! November 2026: every file below answered 200 with text/calendar.

/// Where Thunderbird keeps its holiday calendars.
const BASE: &str = "https://www.thunderbird.net/media/caldata/autogen/";
/// A muted gray, so holidays sit behind the person's own events.
pub const COLOR: &str = "#8a9099";

/// The country shown in Settings, and the file name stem Thunderbird uses.
/// The stems are not always the country's name (`German`, `Dutch`, `US`).
pub const COUNTRIES: &[(&str, &str)] = &[
    ("Albania", "Albania"),
    ("Algeria (French)", "Algeria"),
    ("Argentina", "Argentina"),
    ("Armenia", "Armenia"),
    ("Australia", "Australia"),
    ("Austria", "Austrian"),
    ("Belgium (Dutch)", "Belgian"),
    ("Bolivia", "Bolivia"),
    ("Brazil", "Brazil"),
    ("Bulgaria", "Bulgarian"),
    ("Canada (English)", "Canada"),
    ("Chile", "Chile"),
    ("China", "China"),
    ("Colombia", "Colombian"),
    ("Costa Rica", "CostaRica"),
    ("Croatia", "Croatia"),
    ("Czechia", "Czech"),
    ("Denmark", "Denmark"),
    ("Dominican Republic", "DominicanRepublic"),
    ("Estonia", "Estonia"),
    ("Finland (Finnish)", "Finland"),
    ("France", "French"),
    ("Germany", "German"),
    ("Greece", "Greece"),
    ("Guyana", "Guyana"),
    ("Haiti", "Haiti"),
    ("Hong Kong", "HongKong"),
    ("Hungary", "Hungarian"),
    ("Iceland", "Iceland"),
    ("India", "India"),
    ("Indonesia", "Indonesia"),
    ("Ireland (English)", "Ireland"),
    ("Israel", "Israel"),
    ("Italy", "Italian"),
    ("Japan", "Japan"),
    ("Kenya", "Kenya"),
    ("Latvia", "Latvia"),
    ("Lebanon", "Lebanon"),
    ("Liechtenstein", "Liechtenstein"),
    ("Lithuania", "Lithuanian"),
    ("Malaysia", "Malaysia"),
    ("Malta", "Malta"),
    ("Mexico", "Mexico"),
    ("Morocco", "Morocco"),
    ("Namibia", "Namibia"),
    ("Netherlands (Dutch)", "Dutch"),
    ("New Zealand", "NewZealand"),
    ("Nicaragua", "Nicaragua"),
    ("Norway", "Norwegian"),
    ("Pakistan", "Pakistan"),
    ("Peru", "Peru"),
    ("Philippines", "Philippines"),
    ("Poland", "Polish"),
    ("Portugal", "Portugal"),
    ("Puerto Rico", "PuertoRico"),
    ("Romania", "Romania"),
    ("Russia", "Russia"),
    ("Singapore", "Singapore"),
    ("Slovakia", "Slovak"),
    ("Slovenia", "Slovenian"),
    ("South Africa", "SouthAfrica"),
    ("South Korea", "SouthKorea"),
    ("Spain", "Spain"),
    ("Sri Lanka", "SriLanka"),
    ("Sweden", "Swedish"),
    ("Switzerland", "Swiss"),
    ("Taiwan", "Taiwan"),
    ("Thailand", "Thailand"),
    ("Trinidad and Tobago", "TrinidadandTobago"),
    ("Turkey", "Turkey"),
    ("Ukraine", "Ukraine"),
    ("United Kingdom", "UK"),
    ("United States", "US"),
    ("Uruguay", "Uruguay"),
    ("Vietnam", "Vietnam"),
];

/// Country labels for the picker, in table order.
pub static LABELS: std::sync::LazyLock<Vec<&'static str>> =
    std::sync::LazyLock::new(|| COUNTRIES.iter().map(|(label, _)| *label).collect());
/// The feed address for a country in the table.
pub fn url(index: usize) -> Option<String> {
    COUNTRIES
        .get(index)
        .map(|(_, stem)| format!("{BASE}{stem}Holidays.ics"))
}
/// The calendar's name in the list: "Germany holidays", without a language
/// note such as "(Dutch)".
pub fn name(index: usize) -> Option<String> {
    COUNTRIES.get(index).map(|(label, _)| {
        let country = label.split(" (").next().unwrap_or(label);
        format!("{country} holidays")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_matches_thunderbird_names() {
        assert_eq!(COUNTRIES.len(), 75);
        let germany = COUNTRIES.iter().position(|(l, _)| *l == "Germany").unwrap();
        assert_eq!(
            url(germany).unwrap(),
            "https://www.thunderbird.net/media/caldata/autogen/GermanHolidays.ics"
        );
        let dutch = COUNTRIES.iter().position(|(_, s)| *s == "Dutch").unwrap();
        assert_eq!(name(dutch).unwrap(), "Netherlands holidays");
        assert!(url(COUNTRIES.len()).is_none());
        let mut labels = LABELS.clone();
        let sorted = {
            let mut s = labels.clone();
            s.sort();
            s
        };
        assert_eq!(labels, sorted, "the picker is alphabetical");
        labels.dedup();
        assert_eq!(labels.len(), COUNTRIES.len());
        for (_, stem) in COUNTRIES {
            assert!(stem.chars().all(|c| c.is_ascii_alphanumeric()), "{stem}");
        }
        // Every address passes the link rules: https, no credentials.
        for index in 0..COUNTRIES.len() {
            assert!(super::super::http::parse(&url(index).unwrap(), false).is_ok());
        }
    }
}
