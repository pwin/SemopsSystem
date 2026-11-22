//! The vocabularies the descriptions use.
//!
//! The reference.data.gov.uk namespaces stay `http://`. They are the names of published
//! terms, not addresses this service answers at, and data already written with them would
//! stop matching if they changed.

use oxrdf::NamedNodeRef;

macro_rules! terms {
    ($ns:literal; $($name:ident = $local:literal),* $(,)?) => {
        pub const NS: &str = $ns;
        $(pub const $name: NamedNodeRef<'static> =
            NamedNodeRef::new_unchecked(concat!($ns, $local));)*
    };
}

/// `http://reference.data.gov.uk/def/intervals/`
pub mod interval {
    use super::NamedNodeRef;
    terms!("http://reference.data.gov.uk/def/intervals/";
        CALENDAR_YEAR = "CalendarYear", YEAR = "Year",
        CALENDAR_HALF = "CalendarHalf", HALF = "Half", H1 = "H1", H2 = "H2",
        CALENDAR_QUARTER = "CalendarQuarter", QUARTER = "Quarter",
        Q1 = "Q1", Q2 = "Q2", Q3 = "Q3", Q4 = "Q4",
        CALENDAR_MONTH = "CalendarMonth", MONTH = "Month",
        ISO8601_WEEK = "Iso8601Week", WEEK = "Week",
        CALENDAR_DAY = "CalendarDay", DAY = "Day",
        CALENDAR_HOUR = "CalendarHour", HOUR = "Hour",
        CALENDAR_MINUTE = "CalendarMinute", MINUTE = "Minute",
        CALENDAR_SECOND = "CalendarSecond", SECOND = "Second",
        HAS_XSD_DURATION_DESCRIPTION = "hasXsdDurationDescription",
        NEXT_INTERVAL = "nextInterval", PREVIOUS_INTERVAL = "previousInterval",
        CONTAINS_HALF = "intervalContainsHalf", CONTAINS_HALVES = "intervalContainsHalves",
        CONTAINS_QUARTER = "intervalContainsQuarter",
        CONTAINS_QUARTERS = "intervalContainsQuarters",
        CONTAINS_MONTH = "intervalContainsMonth", CONTAINS_MONTHS = "intervalContainsMonths",
        CONTAINS_DAY = "intervalContainsDay", CONTAINS_DAYS = "intervalContainsDays",
        CONTAINS_HOUR = "intervalContainsHour", CONTAINS_HOURS = "intervalContainsHours",
        CONTAINS_MINUTE = "intervalContainsMinute",
        CONTAINS_MINUTES = "intervalContainsMinutes",
        CONTAINS_SECOND = "intervalContainsSecond",
        CONTAINS_SECONDS = "intervalContainsSeconds",
        ORDINAL_YEAR = "ordinalYear", ORDINAL_HALF_OF_YEAR = "ordinalHalfOfYear",
        ORDINAL_QUARTER_OF_YEAR = "ordinalQuarterOfYear",
        ORDINAL_MONTH_OF_YEAR = "ordinalMonthOfYear", MONTH_OF_YEAR = "monthOfYear",
        ORDINAL_WEEK_OF_YEAR = "ordinalWeekOfYear",
        ORDINAL_WEEK_OF_YEAR_YEAR = "ordinalWeekOfYearYear",
        ORDINAL_DAY_OF_MONTH = "ordinalDayOfMonth", ORDINAL_DAY_OF_WEEK = "ordinalDayOfWeek",
        ORDINAL_DAY_OF_YEAR = "ordinalDayOfYear", DAY_OF_WEEK = "dayOfWeek",
        ORDINAL_HOUR_OF_DAY = "ordinalHourOfDay", ORDINAL_MINUTE_OF_HOUR = "ordinalMinuteOfHour",
        ORDINAL_SECOND_OF_MINUTE = "ordinalSecondOfMinute",
        ONE_YEAR = "one-year", ONE_HALF = "one-half", ONE_QUARTER = "one-quarter",
        ONE_MONTH = "one-month", ONE_WEEK = "one-week", ONE_DAY = "one-day",
        ONE_HOUR = "one-hour", ONE_MINUTE = "one-minute", ONE_SECOND = "one-second",
    );

    /// `interval:January` … `interval:December`.
    #[must_use]
    pub fn month_of_year(month: u32) -> Option<NamedNodeRef<'static>> {
        const MONTHS: [NamedNodeRef<'static>; 12] = [
            NamedNodeRef::new_unchecked(concat!(
                "http://reference.data.gov.uk/def/intervals/",
                "January"
            )),
            NamedNodeRef::new_unchecked(concat!(
                "http://reference.data.gov.uk/def/intervals/",
                "February"
            )),
            NamedNodeRef::new_unchecked(concat!(
                "http://reference.data.gov.uk/def/intervals/",
                "March"
            )),
            NamedNodeRef::new_unchecked(concat!(
                "http://reference.data.gov.uk/def/intervals/",
                "April"
            )),
            NamedNodeRef::new_unchecked(concat!(
                "http://reference.data.gov.uk/def/intervals/",
                "May"
            )),
            NamedNodeRef::new_unchecked(concat!(
                "http://reference.data.gov.uk/def/intervals/",
                "June"
            )),
            NamedNodeRef::new_unchecked(concat!(
                "http://reference.data.gov.uk/def/intervals/",
                "July"
            )),
            NamedNodeRef::new_unchecked(concat!(
                "http://reference.data.gov.uk/def/intervals/",
                "August"
            )),
            NamedNodeRef::new_unchecked(concat!(
                "http://reference.data.gov.uk/def/intervals/",
                "September"
            )),
            NamedNodeRef::new_unchecked(concat!(
                "http://reference.data.gov.uk/def/intervals/",
                "October"
            )),
            NamedNodeRef::new_unchecked(concat!(
                "http://reference.data.gov.uk/def/intervals/",
                "November"
            )),
            NamedNodeRef::new_unchecked(concat!(
                "http://reference.data.gov.uk/def/intervals/",
                "December"
            )),
        ];
        MONTHS
            .get(usize::try_from(month).ok()?.checked_sub(1)?)
            .copied()
    }
}

/// `http://reference.data.gov.uk/def/reference/`: the URI set vocabulary.
pub mod dgu {
    use super::NamedNodeRef;
    terms!("http://reference.data.gov.uk/def/reference/";
        URI_SET = "uriSet", URISET = "URIset", ITEM_TYPE = "itemType",
    );
}

/// OWL-Time, the 2006 namespace IntervalServer wrote.
pub mod time {
    use super::NamedNodeRef;
    terms!("http://www.w3.org/2006/time#";
        INSTANT = "Instant", INTERVAL = "Interval",
        HAS_BEGINNING = "hasBeginning", HAS_END = "hasEnd",
        IN_XSD_DATE_TIME = "inXSDDateTime",
        HAS_DURATION_DESCRIPTION = "hasDurationDescription",
        INTERVAL_EQUALS = "intervalEquals", INTERVAL_MEETS = "intervalMeets",
        INTERVAL_MET_BY = "intervalMetBy", INTERVAL_CONTAINS = "intervalContains",
        INTERVAL_DURING = "intervalDuring", DAY_OF_WEEK = "dayOfWeek",
    );

    /// `time:Monday` … `time:Sunday`, for a weekday numbered Monday 1.
    #[must_use]
    pub fn weekday(day: u32) -> Option<NamedNodeRef<'static>> {
        const DAYS: [NamedNodeRef<'static>; 7] = [
            NamedNodeRef::new_unchecked("http://www.w3.org/2006/time#Monday"),
            NamedNodeRef::new_unchecked("http://www.w3.org/2006/time#Tuesday"),
            NamedNodeRef::new_unchecked("http://www.w3.org/2006/time#Wednesday"),
            NamedNodeRef::new_unchecked("http://www.w3.org/2006/time#Thursday"),
            NamedNodeRef::new_unchecked("http://www.w3.org/2006/time#Friday"),
            NamedNodeRef::new_unchecked("http://www.w3.org/2006/time#Saturday"),
            NamedNodeRef::new_unchecked("http://www.w3.org/2006/time#Sunday"),
        ];
        DAYS.get(usize::try_from(day).ok()?.checked_sub(1)?)
            .copied()
    }
}

/// SCOVO, which the statistical publications of the time used for dimensions.
pub mod scv {
    use super::NamedNodeRef;
    terms!("http://purl.org/NET/scovo#"; DIMENSION = "Dimension", MIN = "min", MAX = "max");
}

pub mod skos {
    use super::NamedNodeRef;
    terms!("http://www.w3.org/2004/02/skos/core#"; PREF_LABEL = "prefLabel");
}

pub mod foaf {
    use super::NamedNodeRef;
    terms!("http://xmlns.com/foaf/0.1/";
        DOCUMENT = "Document", PRIMARY_TOPIC = "primaryTopic",
        IS_PRIMARY_TOPIC_OF = "isPrimaryTopicOf",
    );
}

pub mod dcterms {
    use super::NamedNodeRef;
    terms!("http://purl.org/dc/terms/";
        HAS_FORMAT = "hasFormat", FORMAT = "format", SOURCE = "source",
    );
}

pub mod void {
    use super::NamedNodeRef;
    terms!("http://rdfs.org/ns/void#";
        DATASET = "Dataset", SUBSET = "subset", URI_REGEX_PATTERN = "uriRegexPattern",
        EXAMPLE_RESOURCE = "exampleResource",
    );
}

/// GeoSPARQL 1.1.
pub mod geo {
    use super::NamedNodeRef;
    terms!("http://www.opengis.net/ont/geosparql#";
        GEOMETRY = "Geometry", AS_WKT = "asWKT", WKT_LITERAL = "wktLiteral",
    );
}

/// The Simple Features classes GeoSPARQL uses for geometry types.
pub mod sf {
    use super::NamedNodeRef;
    terms!("http://www.opengis.net/ont/sf#"; POINT = "Point");
}

/// The W3C Basic Geo (WGS84 lat/long) vocabulary.
pub mod wgs84 {
    use super::NamedNodeRef;
    terms!("http://www.w3.org/2003/01/geo/wgs84_pos#"; POINT = "Point", LAT = "lat", LONG = "long");
}

/// Prefixes for serialisations that use them, so the Turtle a person reads is legible.
pub const PREFIXES: &[(&str, &str)] = &[
    ("rdf", "http://www.w3.org/1999/02/22-rdf-syntax-ns#"),
    ("rdfs", "http://www.w3.org/2000/01/rdf-schema#"),
    ("xsd", "http://www.w3.org/2001/XMLSchema#"),
    ("interval", interval::NS),
    ("dgu", dgu::NS),
    ("time", time::NS),
    ("scv", scv::NS),
    ("skos", skos::NS),
    ("foaf", foaf::NS),
    ("dcterms", dcterms::NS),
    ("void", void::NS),
    ("geo", geo::NS),
    ("sf", sf::NS),
    ("wgs84", wgs84::NS),
];
