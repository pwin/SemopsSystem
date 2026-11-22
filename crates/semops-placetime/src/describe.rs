//! The RDF each URI is described with: IntervalServer's, term for term, with its bugs fixed,
//! or the same things in gist ([`gist`]).

mod gist;

use crate::calendar::{Calendar, Duration, Moment, weekday};
use crate::path::{Aligned, Set, Thing, Unit};
use crate::place::Point;
use crate::vocab::{dcterms, dgu, foaf, geo, interval, scv, sf, skos, time, void, wgs84};
use crate::{Format, Vocabulary};
use oxrdf::vocab::{rdf, rdfs, xsd};
use oxrdf::{
    BlankNode, Graph, Literal, NamedNode, NamedNodeRef, NamedOrBlankNodeRef, TermRef, TripleRef,
};
use std::collections::HashMap;

pub(crate) struct Builder<'a> {
    base: &'a str,
    vocabulary: Vocabulary,
    pub(crate) graph: Graph,
    /// gist durations already in the graph, by length in seconds: one node per length.
    durations: HashMap<i64, BlankNode>,
}

impl<'a> Builder<'a> {
    pub(crate) fn new(base: &'a str, vocabulary: Vocabulary) -> Self {
        Self {
            base,
            vocabulary,
            graph: Graph::new(),
            durations: HashMap::new(),
        }
    }

    fn id(&self, path: &str) -> NamedNode {
        NamedNode::new_unchecked(format!("{}id/{path}", self.base))
    }

    fn doc(&self, path: &str) -> NamedNode {
        NamedNode::new_unchecked(format!("{}doc/{path}", self.base))
    }

    fn aligned_id(&self, a: &Aligned) -> NamedNode {
        self.id(&format!("{}/{}", a.segment(), a.reference()))
    }

    fn instant_id(&self, at: Moment) -> NamedNode {
        self.id(&format!(
            "gregorian-instant/{}",
            at.xsd_date_time().unwrap_or_default()
        ))
    }

    fn interval_id(&self, start: Moment, duration: &Duration) -> NamedNode {
        self.id(&format!(
            "gregorian-interval/{}/{duration}",
            start.xsd_date_time().unwrap_or_default()
        ))
    }

    /// A set's path pattern below `id/`, anchored to the base: `^https://example\.org/id/…$`.
    fn uri_regex(&self, pattern: &str) -> String {
        format!("^{}id/{pattern}$", self.base.replace('.', "\\."))
    }

    fn add<'b>(
        &mut self,
        subject: impl Into<NamedOrBlankNodeRef<'b>>,
        predicate: NamedNodeRef<'b>,
        object: impl Into<TermRef<'b>>,
    ) {
        self.graph
            .insert(TripleRef::new(subject.into(), predicate, object.into()));
    }

    fn kind(&mut self, node: &NamedNode, class: NamedNodeRef<'_>) {
        self.add(node, rdf::TYPE, class);
    }

    fn link(&mut self, subject: &NamedNode, predicate: NamedNodeRef<'_>, object: &NamedNode) {
        self.add(subject, predicate, object);
    }

    fn labels(&mut self, node: &NamedNode, label: &str) {
        let label = english(label);
        self.add(node, skos::PREF_LABEL, &label);
        self.add(node, rdfs::LABEL, &label);
    }

    fn comment(&mut self, node: &NamedNode, text: &str) {
        self.add(node, rdfs::COMMENT, &english(text));
    }

    fn typed(
        &mut self,
        node: &NamedNode,
        predicate: NamedNodeRef<'_>,
        value: &str,
        datatype: NamedNodeRef<'_>,
    ) {
        self.add(
            node,
            predicate,
            &Literal::new_typed_literal(value, datatype),
        );
    }

    fn integer(&mut self, node: &NamedNode, predicate: NamedNodeRef<'_>, value: impl ToString) {
        self.typed(node, predicate, &value.to_string(), xsd::INTEGER);
    }

    /// An RDF collection of `items`, linked from `node` by `predicate`.
    fn list(&mut self, node: &NamedNode, predicate: NamedNodeRef<'_>, items: &[NamedNode]) {
        let cells: Vec<BlankNode> = items.iter().map(|_| BlankNode::default()).collect();
        match cells.first() {
            Some(head) => self.add(node, predicate, head),
            None => self.add(node, predicate, rdf::NIL),
        }
        for (i, (cell, item)) in cells.iter().zip(items).enumerate() {
            self.add(cell, rdf::FIRST, item);
            match cells.get(i + 1) {
                Some(next) => self.add(cell, rdf::REST, next),
                None => self.add(cell, rdf::REST, rdf::NIL),
            }
        }
    }
}

fn english(text: &str) -> Literal {
    Literal::new_language_tagged_literal_unchecked(text, "en")
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const WEEKDAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

fn month_name(month: u32) -> &'static str {
    MONTHS[(month as usize).saturating_sub(1) % 12]
}

fn weekday_name(day: i64) -> &'static str {
    WEEKDAYS[(weekday(day) as usize - 1) % 7]
}

/// "1st", "2nd", "3rd", "11th", "21st".
fn ordinal(n: u32) -> String {
    let suffix = match (n % 10, n % 100) {
        (1, r) if r != 11 => "st",
        (2, r) if r != 12 => "nd",
        (3, r) if r != 13 => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

/// "the 1st second of 1st minute of 1st hour of Sunday the 1st January": the phrase the
/// second, minute, instant and interval comments share.
fn second_phrase(calendar: Calendar, m: Moment) -> String {
    let d = calendar.date(m.day);
    format!(
        "{} second of {} minute of {} hour of {} the {} {}",
        ordinal(m.second_of_minute() + 1),
        ordinal(m.minute() + 1),
        ordinal(m.hour() + 1),
        weekday_name(m.day),
        ordinal(d.day),
        month_name(d.month)
    )
}

fn classes(unit: Unit) -> (NamedNodeRef<'static>, NamedNodeRef<'static>) {
    match unit {
        Unit::Year => (interval::CALENDAR_YEAR, interval::YEAR),
        Unit::Half => (interval::CALENDAR_HALF, interval::HALF),
        Unit::Quarter => (interval::CALENDAR_QUARTER, interval::QUARTER),
        Unit::Month => (interval::CALENDAR_MONTH, interval::MONTH),
        Unit::Week => (interval::ISO8601_WEEK, interval::WEEK),
        Unit::Day => (interval::CALENDAR_DAY, interval::DAY),
        Unit::Hour => (interval::CALENDAR_HOUR, interval::HOUR),
        Unit::Minute => (interval::CALENDAR_MINUTE, interval::MINUTE),
        Unit::Second => (interval::CALENDAR_SECOND, interval::SECOND),
    }
}

fn duration_description(unit: Unit) -> NamedNodeRef<'static> {
    match unit {
        Unit::Year => interval::ONE_YEAR,
        Unit::Half => interval::ONE_HALF,
        Unit::Quarter => interval::ONE_QUARTER,
        Unit::Month => interval::ONE_MONTH,
        Unit::Week => interval::ONE_WEEK,
        Unit::Day => interval::ONE_DAY,
        Unit::Hour => interval::ONE_HOUR,
        Unit::Minute => interval::ONE_MINUTE,
        Unit::Second => interval::ONE_SECOND,
    }
}

/// The typed containment property for a contained unit, and the list property for many.
fn containment(unit: Unit) -> (Option<NamedNodeRef<'static>>, Option<NamedNodeRef<'static>>) {
    match unit {
        Unit::Half => (
            Some(interval::CONTAINS_HALF),
            Some(interval::CONTAINS_HALVES),
        ),
        Unit::Quarter => (
            Some(interval::CONTAINS_QUARTER),
            Some(interval::CONTAINS_QUARTERS),
        ),
        Unit::Month => (
            Some(interval::CONTAINS_MONTH),
            Some(interval::CONTAINS_MONTHS),
        ),
        Unit::Day => (Some(interval::CONTAINS_DAY), Some(interval::CONTAINS_DAYS)),
        Unit::Hour => (
            Some(interval::CONTAINS_HOUR),
            Some(interval::CONTAINS_HOURS),
        ),
        Unit::Minute => (
            Some(interval::CONTAINS_MINUTE),
            Some(interval::CONTAINS_MINUTES),
        ),
        Unit::Second => (
            Some(interval::CONTAINS_SECOND),
            Some(interval::CONTAINS_SECONDS),
        ),
        Unit::Year | Unit::Week => (None, None),
    }
}

/// The units an interval of `unit` sits inside, outermost first.
fn containers(unit: Unit) -> &'static [Unit] {
    use Unit::{Day, Half, Hour, Minute, Month, Quarter, Week, Year};
    match unit {
        Unit::Year | Unit::Week => &[],
        Unit::Half => &[Year],
        Unit::Quarter => &[Year, Half],
        Unit::Month => &[Year, Half, Quarter],
        Unit::Day => &[Year, Half, Quarter, Month, Week],
        Unit::Hour => &[Year, Half, Quarter, Month, Day, Week],
        Unit::Minute => &[Year, Half, Quarter, Month, Day, Hour, Week],
        Unit::Second => &[Year, Half, Quarter, Month, Day, Hour, Minute, Week],
    }
}

/// The finer units an interval of `unit` is listed as made of.
fn parts(unit: Unit) -> &'static [Unit] {
    use Unit::{Day, Hour, Minute, Month, Quarter, Second};
    match unit {
        Unit::Year => &[Unit::Half, Quarter, Month],
        Unit::Half => &[Quarter, Month],
        Unit::Quarter => &[Month],
        Unit::Month | Unit::Week => &[Day],
        Unit::Day => &[Hour],
        Unit::Hour => &[Minute],
        Unit::Minute => &[Second],
        Unit::Second => &[],
    }
}

/// A calendar-aligned interval's label and the sentence describing it, as IntervalServer
/// worded them.
fn aligned_text(a: &Aligned) -> (String, String) {
    let d = a.calendar.date(a.start.day);
    let cal = a.calendar.name();
    let year = format!("{:04}", d.year);
    let comment = match a.unit {
        Unit::Year => format!("The {cal} calendar year of {year}"),
        Unit::Half => {
            let which = if d.month <= 6 { "first" } else { "second" };
            format!("The {which} half of {cal} calendar year {year}")
        }
        Unit::Quarter => {
            let which = match (d.month - 1) / 3 {
                0 => "first",
                1 => "second",
                2 => "third",
                _ => "fourth",
            };
            format!("The {which} quarter of the {cal} calendar year {year}")
        }
        Unit::Month => {
            format!(
                "The month of {} in the {cal} calendar year {year}",
                month_name(d.month)
            )
        }
        Unit::Week => {
            let (week_year, week) = a.calendar.week_of(a.start.day);
            format!("Week {week} of the {cal} calendar year {week_year:04}")
        }
        Unit::Day => format!(
            "{} the {} of {} in the {cal} calendar year {year}",
            weekday_name(a.start.day),
            ordinal(d.day),
            month_name(d.month)
        ),
        Unit::Hour => format!(
            "The {} hour of {} the {} {} in the {cal} calendar year {year}",
            ordinal(a.start.hour() + 1),
            weekday_name(a.start.day),
            ordinal(d.day),
            month_name(d.month)
        ),
        Unit::Minute => format!(
            "The {} minute of {} hour of {} the {} {} of the {cal} calendar year {year}",
            ordinal(a.start.minute() + 1),
            ordinal(a.start.hour() + 1),
            weekday_name(a.start.day),
            ordinal(d.day),
            month_name(d.month)
        ),
        Unit::Second => format!(
            "The {} of the {cal} calendar year {year}",
            second_phrase(a.calendar, a.start)
        ),
    };
    (
        format!("{cal} {}:{}", a.unit.title(), a.reference()),
        comment,
    )
}

/// An instant's label and description.
fn instant_text(at: Moment) -> (String, String) {
    let lexical = at.xsd_date_time().unwrap_or_default();
    let year = Calendar::Gregorian.date(at.day).year;
    (
        format!("Gregorian Instant:{lexical}"),
        format!(
            "The instant at start of the {} of the Gregorian calendar year {year:04}",
            second_phrase(Calendar::Gregorian, at)
        ),
    )
}

/// An interval's label and description.
fn interval_text(start: Moment, duration: &Duration) -> (String, String) {
    let reference = format!("{}/{duration}", start.xsd_date_time().unwrap_or_default());
    let year = Calendar::Gregorian.date(start.day).year;
    (
        format!("Gregorian Interval:{reference}"),
        format!(
            "An interval of {}beginning at the start of the {} {year:04} in the Gregorian Calendar",
            duration.phrase(),
            second_phrase(Calendar::Gregorian, start)
        ),
    )
}

fn point_text(point: &Point) -> String {
    format!(
        "The point at latitude {} and longitude {} in the WGS84 datum",
        point.latitude.as_str(),
        point.longitude.as_str()
    )
}

impl Builder<'_> {
    /// A calendar-aligned interval's types, labels and comment, and nothing else: how an
    /// interval appears when another one's document mentions it.
    fn aligned_labels(&mut self, a: &Aligned) -> NamedNode {
        let node = self.aligned_id(a);
        let (calendar_class, class) = classes(a.unit);
        self.kind(&node, calendar_class);
        self.kind(&node, class);
        let month = a.calendar.date(a.start.day).month;
        match a.unit {
            Unit::Half => self.kind(
                &node,
                if month <= 6 {
                    interval::H1
                } else {
                    interval::H2
                },
            ),
            Unit::Quarter => self.kind(
                &node,
                match (month - 1) / 3 {
                    0 => interval::Q1,
                    1 => interval::Q2,
                    2 => interval::Q3,
                    _ => interval::Q4,
                },
            ),
            _ => {}
        }
        let (label, comment) = aligned_text(a);
        self.labels(&node, &label);
        self.comment(&node, &comment);
        node
    }

    /// The ordinal numbers of a moment, down to `depth`, in `calendar`.
    fn ordinals(&mut self, node: &NamedNode, calendar: Calendar, depth: Unit, at: Moment) {
        let d = calendar.date(at.day);
        if depth >= Unit::Second {
            self.integer(
                node,
                interval::ORDINAL_SECOND_OF_MINUTE,
                at.second_of_minute(),
            );
        }
        if depth >= Unit::Minute {
            self.integer(node, interval::ORDINAL_MINUTE_OF_HOUR, at.minute());
        }
        if depth >= Unit::Hour {
            self.integer(node, interval::ORDINAL_HOUR_OF_DAY, at.hour());
        }
        if depth >= Unit::Week {
            let (week_year, week) = calendar.week_of(at.day);
            self.integer(node, interval::ORDINAL_WEEK_OF_YEAR, week);
            self.integer(node, interval::ORDINAL_WEEK_OF_YEAR_YEAR, week_year);
        }
        if depth == Unit::Week {
            return;
        }
        if depth >= Unit::Day {
            self.integer(node, interval::ORDINAL_DAY_OF_MONTH, d.day);
            self.integer(node, interval::ORDINAL_DAY_OF_WEEK, weekday(at.day));
            if let Some(day) = time::weekday(weekday(at.day)) {
                self.add(node, interval::DAY_OF_WEEK, day);
            }
            self.integer(
                node,
                interval::ORDINAL_DAY_OF_YEAR,
                calendar.day_of_year(at.day),
            );
        }
        if depth >= Unit::Month {
            self.integer(node, interval::ORDINAL_MONTH_OF_YEAR, d.month);
            if let Some(month) = interval::month_of_year(d.month) {
                self.add(node, interval::MONTH_OF_YEAR, month);
            }
        }
        if depth >= Unit::Quarter {
            self.integer(
                node,
                interval::ORDINAL_QUARTER_OF_YEAR,
                (d.month - 1) / 3 + 1,
            );
        }
        if depth >= Unit::Half {
            self.integer(node, interval::ORDINAL_HALF_OF_YEAR, (d.month - 1) / 6 + 1);
        }
        self.integer(node, interval::ORDINAL_YEAR, d.year);
    }

    /// An instant on the Gregorian timeline, fully: every interval's beginning and end is one.
    fn instant(&mut self, at: Moment) -> NamedNode {
        let node = self.instant_id(at);
        self.kind(&node, time::INSTANT);
        self.ordinals(&node, Calendar::Gregorian, Unit::Second, at);
        let (label, comment) = instant_text(at);
        self.comment(&node, &comment);
        self.labels(&node, &label);
        let lexical = at.xsd_date_time().unwrap_or_default();
        self.typed(&node, time::IN_XSD_DATE_TIME, &lexical, xsd::DATE_TIME);
        node
    }

    fn interval_labels(&mut self, start: Moment, duration: &Duration) -> NamedNode {
        let node = self.interval_id(start, duration);
        self.kind(&node, time::INTERVAL);
        let (label, comment) = interval_text(start, duration);
        self.labels(&node, &label);
        self.comment(&node, &comment);
        node
    }

    /// Where an interval begins and ends, as instants and as SCOVO bounds. The bounds are
    /// dates for intervals of a week or longer and date-times below that, as IntervalServer
    /// wrote them; the upper bound is the last second inside the interval.
    fn bounds(&mut self, node: &NamedNode, start: Moment, end: Moment, dates: bool) {
        let last = end.plus_seconds(-1);
        let (lower, upper, datatype) = if dates {
            (start.xsd_date(), last.xsd_date(), xsd::DATE)
        } else {
            (start.xsd_date_time(), last.xsd_date_time(), xsd::DATE_TIME)
        };
        let begins = self.instant(start);
        self.link(node, time::HAS_BEGINNING, &begins);
        let ends = self.instant(end);
        self.link(node, time::HAS_END, &ends);
        if let Some(lower) = lower {
            self.typed(node, scv::MIN, &lower, datatype);
        }
        if let Some(upper) = upper {
            self.typed(node, scv::MAX, &upper, datatype);
        }
    }

    fn set_labels(&mut self, set: &Set) -> NamedNode {
        let node = self.id(&set.path());
        self.kind(&node, void::DATASET);
        self.kind(&node, dgu::URISET);
        self.labels(&node, &set_label(set));
        node
    }

    fn connect(&mut self, container: &NamedNode, contained: &NamedNode, unit: Unit) {
        if let (Some(typed), _) = containment(unit) {
            self.link(container, typed, contained);
        }
        self.link(container, time::INTERVAL_CONTAINS, contained);
        self.link(contained, time::INTERVAL_DURING, container);
    }

    fn neighbours(
        &mut self,
        node: &NamedNode,
        next: Option<NamedNode>,
        previous: Option<NamedNode>,
    ) {
        if let Some(next) = next {
            self.link(node, interval::NEXT_INTERVAL, &next);
            self.link(node, time::INTERVAL_MEETS, &next);
            self.link(&next, time::INTERVAL_MET_BY, node);
        }
        if let Some(previous) = previous {
            self.link(node, interval::PREVIOUS_INTERVAL, &previous);
            self.link(node, time::INTERVAL_MET_BY, &previous);
            self.link(&previous, time::INTERVAL_MEETS, node);
        }
    }

    fn aligned(&mut self, a: &Aligned) -> NamedNode {
        let node = self.aligned_labels(a);
        self.kind(&node, scv::DIMENSION);
        self.ordinals(&node, a.calendar, a.unit, a.start);
        self.typed(
            &node,
            interval::HAS_XSD_DURATION_DESCRIPTION,
            &a.unit.duration().to_string(),
            xsd::DURATION,
        );
        self.add(
            &node,
            time::HAS_DURATION_DESCRIPTION,
            duration_description(a.unit),
        );
        if a.unit == Unit::Day
            && let Some(day) = time::weekday(weekday(a.start.day))
        {
            self.add(&node, time::DAY_OF_WEEK, day);
        }
        if let Some(end) = a.end() {
            self.bounds(&node, a.start, end, a.unit <= Unit::Week);
        }
        let set = self.set_labels(&Set::Unit(a.calendar, a.unit));
        self.link(&node, dgu::URI_SET, &set);
        if let Some(duration) = a.gregorian_duration() {
            let equal = self.interval_labels(a.start, &duration);
            self.link(&node, time::INTERVAL_EQUALS, &equal);
        }
        let next = a.next().map(|n| self.aligned_labels(&n));
        let previous = a.previous().map(|p| self.aligned_labels(&p));
        self.neighbours(&node, next, previous);
        for &unit in containers(a.unit) {
            if let Some(container) = Aligned::holding(a.calendar, unit, a.start) {
                let container = self.aligned_labels(&container);
                self.connect(&container, &node, a.unit);
            }
        }
        for &unit in parts(a.unit) {
            let children: Vec<NamedNode> = a
                .children(unit)
                .iter()
                .map(|child| {
                    let child = self.aligned_labels(child);
                    self.connect(&node, &child, unit);
                    child
                })
                .collect();
            if let (_, Some(list)) = containment(unit) {
                self.list(&node, list, &children);
            }
        }
        node
    }

    fn interval(&mut self, start: Moment, duration: &Duration) -> NamedNode {
        let node = self.interval_labels(start, duration);
        self.kind(&node, scv::DIMENSION);
        self.typed(
            &node,
            interval::HAS_XSD_DURATION_DESCRIPTION,
            &duration.to_string(),
            xsd::DURATION,
        );
        if let Some(end) = Calendar::Gregorian.add(start, duration) {
            self.bounds(&node, start, end, false);
        }
        let set = self.set_labels(&Set::Intervals);
        self.link(&node, dgu::URI_SET, &set);
        node
    }

    fn point(&mut self, point: &Point) -> NamedNode {
        let node = self.id(&Thing::Point(point.clone()).path());
        self.kind(&node, geo::GEOMETRY);
        self.kind(&node, sf::POINT);
        self.kind(&node, wgs84::POINT);
        self.labels(&node, &point.label());
        self.comment(&node, &point_text(point));
        self.typed(&node, wgs84::LAT, point.latitude.as_str(), xsd::DECIMAL);
        self.typed(&node, wgs84::LONG, point.longitude.as_str(), xsd::DECIMAL);
        self.typed(&node, geo::AS_WKT, &point.wkt(), geo::WKT_LITERAL);
        self.link(
            &node,
            rdfs::SEE_ALSO,
            &NamedNode::new_unchecked(point.geo_uri()),
        );
        let set = self.set_labels(&Set::Points);
        self.link(&node, dgu::URI_SET, &set);
        node
    }

    fn source(&mut self, node: &NamedNode, calendar: Calendar) {
        let (iri, label) = match calendar {
            Calendar::British => (
                "http://www.legislation.gov.uk/id/apgb/Geo2/24/23",
                "Calendar (New Style) Act 1750.",
            ),
            Calendar::Gregorian => (
                "https://en.wikipedia.org/wiki/Gregorian_calendar",
                "Wikipedia on Gregorian Calendar",
            ),
            Calendar::Julian => (
                "https://en.wikipedia.org/wiki/Julian_calendar",
                "Wikipedia on Julian Calendar",
            ),
        };
        let source = NamedNode::new_unchecked(iri);
        self.link(node, dcterms::SOURCE, &source);
        self.labels(&source, label);
    }

    fn set(&mut self, set: &Set) -> NamedNode {
        let node = self.set_labels(set);
        self.comment(&node, &set_comment(set));
        for subset in subsets(set) {
            let subset = self.set_labels(&subset);
            self.link(&node, void::SUBSET, &subset);
        }
        if let Some(item) = item_type(set) {
            self.add(&node, dgu::ITEM_TYPE, item);
        }
        if let Some(pattern) = pattern(set) {
            let regex = self.uri_regex(&pattern);
            self.typed(&node, void::URI_REGEX_PATTERN, &regex, xsd::STRING);
        }
        if let Some(example) = example(set) {
            let example = match example {
                Thing::Aligned(a) => self.aligned_labels(&a),
                Thing::Instant(at) => self.instant(at),
                Thing::Interval { start, duration } => self.interval_labels(start, &duration),
                Thing::Point(point) => {
                    let node = self.id(&Thing::Point(point.clone()).path());
                    self.labels(&node, &point.label());
                    node
                }
                Thing::Set(other) => self.set_labels(&other),
            };
            self.link(&node, void::EXAMPLE_RESOURCE, &example);
        }
        if let Set::Calendar(calendar) | Set::Unit(calendar, _) = *set {
            self.source(&node, calendar);
        }
        node
    }

    /// Describes the document itself, and the format-specific documents it is offered as.
    pub(crate) fn document(&mut self, path: &str, topic: &NamedNode, label: &str) {
        let doc = self.doc(path);
        self.kind(&doc, foaf::DOCUMENT);
        self.link(&doc, foaf::PRIMARY_TOPIC, topic);
        self.link(topic, foaf::IS_PRIMARY_TOPIC_OF, &doc);
        let label_property = self.label_property();
        let text = self.text(&format!("Document about: {label}"));
        self.add(&doc, label_property, &text);
        for format in Format::OFFERED {
            let variant = self.doc(&format!("{path}.{}", format.extension()));
            self.kind(&variant, foaf::DOCUMENT);
            let media = NamedNode::new_unchecked(format!(
                "http://www.iana.org/assignments/media-types/{}",
                format.media_type()
            ));
            self.link(&variant, dcterms::FORMAT, &media);
            let text = self.text(&format!("{} document about: {label}", format.name()));
            self.add(&variant, label_property, &text);
            self.link(&doc, dcterms::HAS_FORMAT, &variant);
        }
    }

    /// How a document is labelled: `rdfs:label` in English, as IntervalServer wrote it, or
    /// gist's `skos:prefLabel` with a plain string.
    fn label_property(&self) -> NamedNodeRef<'static> {
        match self.vocabulary {
            Vocabulary::DataGovUk => rdfs::LABEL,
            Vocabulary::Gist => skos::PREF_LABEL,
        }
    }

    fn text(&self, text: &str) -> Literal {
        match self.vocabulary {
            Vocabulary::DataGovUk => english(text),
            Vocabulary::Gist => Literal::new_simple_literal(text),
        }
    }

    /// Describes a thing; returns its node and its label.
    pub(crate) fn thing(&mut self, thing: &Thing) -> (NamedNode, String) {
        let node = match self.vocabulary {
            Vocabulary::DataGovUk => self.data_gov_uk(thing),
            Vocabulary::Gist => self.gist(thing),
        };
        let label = self
            .graph
            .object_for_subject_predicate(&node, skos::PREF_LABEL)
            .and_then(|o| match o {
                TermRef::Literal(l) => Some(l.value().to_owned()),
                _ => None,
            })
            .unwrap_or_default();
        (node, label)
    }

    fn data_gov_uk(&mut self, thing: &Thing) -> NamedNode {
        match thing {
            Thing::Aligned(a) => self.aligned(a),
            Thing::Instant(m) => {
                let node = self.instant(*m);
                let set = self.set_labels(&Set::Instants);
                self.link(&node, dgu::URI_SET, &set);
                node
            }
            Thing::Interval { start, duration } => self.interval(*start, duration),
            Thing::Point(p) => self.point(p),
            Thing::Set(s) => self.set(s),
        }
    }
}

fn set_comment(set: &Set) -> String {
    match *set {
        Set::All => "A dataset of unzoned calendar aligned intervals in the British, Gregorian \
                     and Julian calendars, and of instants and intervals of any duration on the \
                     Gregorian timeline."
            .to_owned(),
        Set::Calendar(calendar) => format!(
            "A dataset of {} calendar aligned time intervals, from years to seconds.",
            calendar.name()
        ),
        Set::Unit(calendar, unit) => format!(
            "A dataset of {} calendar aligned time intervals of {} duration.",
            calendar.name(),
            unit_phrase(unit)
        ),
        Set::Instants => {
            "A dataset of unzoned instants on the Gregorian timeline, to the second.".to_owned()
        }
        Set::Intervals => {
            "A dataset of unzoned intervals on the Gregorian timeline, each named by \
                           its start and its duration."
                .to_owned()
        }
        Set::Points => {
            "A dataset of points on the WGS84 ellipsoid, each named by its longitude (X) \
                        and latitude (Y) in decimal degrees."
                .to_owned()
        }
    }
}

/// The sets a set is divided into.
fn subsets(set: &Set) -> Vec<Set> {
    match *set {
        Set::All => Calendar::ALL.into_iter().map(Set::Calendar).collect(),
        Set::Calendar(calendar) => Unit::ALL
            .into_iter()
            .map(|unit| Set::Unit(calendar, unit))
            .chain([Set::Instants, Set::Intervals])
            .collect(),
        _ => Vec::new(),
    }
}

/// The class of a set's members, in reference.data.gov.uk's terms.
fn item_type(set: &Set) -> Option<NamedNodeRef<'static>> {
    match *set {
        Set::Unit(_, unit) => Some(classes(unit).0),
        Set::Instants => Some(time::INSTANT),
        Set::Intervals => Some(time::INTERVAL),
        Set::Points => Some(sf::POINT),
        Set::All | Set::Calendar(_) => None,
    }
}

/// The pattern a member's path below `id/` matches.
fn pattern(set: &Set) -> Option<String> {
    match *set {
        Set::Unit(calendar, unit) => Some(format!(
            "{}{}/{}",
            calendar.prefix(),
            unit.name(),
            unit_regex(unit)
        )),
        Set::Instants => Some(format!("gregorian-instant/{}", unit_regex(Unit::Second))),
        Set::Intervals => Some(format!(
            "gregorian-interval/{}/P([0-9]+Y)?([0-9]+M)?([0-9]+D)?(T([0-9]+H)?([0-9]+M)?([0-9]+S)?)?",
            unit_regex(Unit::Second)
        )),
        Set::Points => {
            Some("geopoint/wgs84/X-?[0-9]{1,3}(\\.[0-9]+)?Y-?[0-9]{1,2}(\\.[0-9]+)?".to_owned())
        }
        Set::All | Set::Calendar(_) => None,
    }
}

/// The member a set's description names as an example.
fn example(set: &Set) -> Option<Thing> {
    let new_style = Moment::midnight(crate::calendar::gregorian_days(1752, 9, 14));
    match *set {
        Set::Unit(calendar, unit) => {
            Aligned::holding(calendar, unit, new_style).map(Thing::Aligned)
        }
        Set::Instants => Some(Thing::Instant(new_style)),
        Set::Intervals => Some(Thing::Interval {
            start: Moment::midnight(crate::calendar::gregorian_days(2016, 6, 30)),
            duration: Duration::months(3),
        }),
        Set::Points => Point::parse("X-0.1276Y51.5072").map(Thing::Point),
        Set::All | Set::Calendar(_) => None,
    }
}

fn unit_phrase(unit: Unit) -> &'static str {
    match unit {
        Unit::Year => "one year",
        Unit::Half => "one half year",
        Unit::Quarter => "one quarter year",
        Unit::Month => "one month",
        Unit::Week => "one ISO 8601 numbered week",
        Unit::Day => "one day",
        Unit::Hour => "one hour",
        Unit::Minute => "one minute",
        Unit::Second => "one second",
    }
}

fn set_label(set: &Set) -> String {
    match *set {
        Set::All => "Calendar aligned intervals, instants and intervals.".to_owned(),
        Set::Calendar(c) => format!("{} calendar aligned intervals.", c.name()),
        Set::Unit(c, Unit::Week) => {
            format!(
                "{} calendar aligned ISO8601 numbered week long intervals.",
                c.name()
            )
        }
        Set::Unit(c, u) => format!(
            "{} calendar aligned {} intervals.",
            c.name(),
            unit_phrase(u)
        ),
        Set::Instants => "Arbitrary instants on the Gregorian timeline.".to_owned(),
        Set::Intervals => "Arbitrary duration intervals on the Gregorian timeline.".to_owned(),
        Set::Points => "Points on the WGS84 ellipsoid.".to_owned(),
    }
}

fn unit_regex(unit: Unit) -> String {
    let year = "([0-9]{4}|[1-9][0-9]{4,})";
    let day = format!("{year}-[0-1][0-9]-[0-3][0-9]");
    match unit {
        Unit::Year => year.to_owned(),
        Unit::Half => format!("{year}-H[1-2]"),
        Unit::Quarter => format!("{year}-Q[1-4]"),
        Unit::Month => format!("{year}-[0-1][0-9]"),
        Unit::Week => format!("{year}-W[0-5][0-9]"),
        Unit::Day => day,
        Unit::Hour => format!("{day}T[0-2][0-9]"),
        Unit::Minute => format!("{day}T[0-2][0-9]:[0-5][0-9]"),
        Unit::Second => format!("{day}T[0-2][0-9]:[0-5][0-9]:[0-5][0-9]"),
    }
}
