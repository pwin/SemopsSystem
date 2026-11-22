//! The vocabularies the descriptions use: reference.data.gov.uk's, with OWL-Time, SCOVO,
//! WGS84 and GeoSPARQL, as IntervalServer wrote them, or gist and the placetime ontology.
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

/// gist 14.1.0, Semantic Arts' upper ontology.
pub mod gist {
    use super::NamedNodeRef;
    terms!("https://w3id.org/semanticarts/ns/ontology/gist/";
        TIME_INTERVAL = "TimeInterval", MAGNITUDE = "Magnitude", COLLECTION = "Collection",
        GEO_POINT = "GeoPoint",
        START_DATE_TIME = "startDateTime", END_DATE_TIME = "endDateTime",
        HAS_MAGNITUDE = "hasMagnitude", HAS_ASPECT = "hasAspect",
        HAS_UNIT_OF_MEASURE = "hasUnitOfMeasure", NUMERIC_VALUE = "numericValue",
        IS_PART_OF = "isPartOf", IS_DIRECT_PART_OF = "isDirectPartOf",
        PRECEDES_DIRECTLY = "precedesDirectly", IS_CATEGORIZED_BY = "isCategorizedBy",
        CONFORMS_TO = "conformsTo", IS_MEMBER_OF = "isMemberOf",
        LATITUDE = "latitude", LONGITUDE = "longitude", DESCRIPTION = "description",
    );
}

/// gist's reference data: the duration aspect, and the second, named as gist names its units.
pub mod gistd {
    use super::NamedNodeRef;
    terms!("https://w3id.org/semanticarts/ns/data/gist/";
        ASPECT_DURATION = "_Aspect_duration", UNIT_OF_MEASURE_SECOND = "_UnitOfMeasure_second",
    );
}

/// The placetime ontology's classes (`ontology/placetime.ttl`).
pub mod pt {
    use super::NamedNodeRef;
    terms!("https://www.semantechs.co.uk/ns/ontology/placetime/";
        CALENDAR = "Calendar", CALENDAR_INTERVAL = "CalendarInterval",
        CALENDAR_YEAR = "CalendarYear", CALENDAR_HALF_YEAR = "CalendarHalfYear",
        CALENDAR_QUARTER = "CalendarQuarter", CALENDAR_MONTH = "CalendarMonth",
        ISO8601_WEEK = "Iso8601Week", CALENDAR_DAY = "CalendarDay",
        CALENDAR_HOUR = "CalendarHour", CALENDAR_MINUTE = "CalendarMinute",
        CALENDAR_SECOND = "CalendarSecond", INSTANT = "Instant",
    );
}

/// The placetime ontology's calendars.
pub mod ptd {
    use super::NamedNodeRef;
    terms!("https://www.semantechs.co.uk/ns/data/placetime/";
        CALENDAR_BRITISH = "_Calendar_British", CALENDAR_GREGORIAN = "_Calendar_Gregorian",
        CALENDAR_JULIAN = "_Calendar_Julian",
    );
}

/// The placetime ontology's categories: days of the week, months, quarters and halves.
pub mod ptx {
    use super::NamedNodeRef;
    terms!("https://www.semantechs.co.uk/ns/taxonomy/placetime/";
        MONDAY = "_DayOfWeek_Monday", TUESDAY = "_DayOfWeek_Tuesday",
        WEDNESDAY = "_DayOfWeek_Wednesday", THURSDAY = "_DayOfWeek_Thursday",
        FRIDAY = "_DayOfWeek_Friday", SATURDAY = "_DayOfWeek_Saturday",
        SUNDAY = "_DayOfWeek_Sunday",
        JANUARY = "_MonthOfYear_January", FEBRUARY = "_MonthOfYear_February",
        MARCH = "_MonthOfYear_March", APRIL = "_MonthOfYear_April", MAY = "_MonthOfYear_May",
        JUNE = "_MonthOfYear_June", JULY = "_MonthOfYear_July",
        AUGUST = "_MonthOfYear_August", SEPTEMBER = "_MonthOfYear_September",
        OCTOBER = "_MonthOfYear_October", NOVEMBER = "_MonthOfYear_November",
        DECEMBER = "_MonthOfYear_December",
        Q1 = "_QuarterOfYear_Q1", Q2 = "_QuarterOfYear_Q2", Q3 = "_QuarterOfYear_Q3",
        Q4 = "_QuarterOfYear_Q4", H1 = "_HalfOfYear_H1", H2 = "_HalfOfYear_H2",
    );

    /// The day of the week, numbered Monday 1.
    #[must_use]
    pub fn weekday(day: u32) -> Option<NamedNodeRef<'static>> {
        [
            MONDAY, TUESDAY, WEDNESDAY, THURSDAY, FRIDAY, SATURDAY, SUNDAY,
        ]
        .get(usize::try_from(day).ok()?.checked_sub(1)?)
        .copied()
    }

    /// The month, numbered January 1.
    #[must_use]
    pub fn month(month: u32) -> Option<NamedNodeRef<'static>> {
        [
            JANUARY, FEBRUARY, MARCH, APRIL, MAY, JUNE, JULY, AUGUST, SEPTEMBER, OCTOBER, NOVEMBER,
            DECEMBER,
        ]
        .get(usize::try_from(month).ok()?.checked_sub(1)?)
        .copied()
    }

    /// The quarter of the year, 1 to 4.
    #[must_use]
    pub fn quarter(quarter: u32) -> Option<NamedNodeRef<'static>> {
        [Q1, Q2, Q3, Q4]
            .get(usize::try_from(quarter).ok()?.checked_sub(1)?)
            .copied()
    }

    /// The half of the year, 1 or 2.
    #[must_use]
    pub fn half(half: u32) -> Option<NamedNodeRef<'static>> {
        [H1, H2]
            .get(usize::try_from(half).ok()?.checked_sub(1)?)
            .copied()
    }
}

/// Prefixes for gist descriptions.
pub const GIST_PREFIXES: &[(&str, &str)] = &[
    ("rdf", "http://www.w3.org/1999/02/22-rdf-syntax-ns#"),
    ("rdfs", "http://www.w3.org/2000/01/rdf-schema#"),
    ("xsd", "http://www.w3.org/2001/XMLSchema#"),
    ("gist", gist::NS),
    ("gistd", gistd::NS),
    ("pt", pt::NS),
    ("ptd", ptd::NS),
    ("ptx", ptx::NS),
    ("skos", skos::NS),
    ("foaf", foaf::NS),
    ("dcterms", dcterms::NS),
    ("void", void::NS),
    ("geo", geo::NS),
];

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
