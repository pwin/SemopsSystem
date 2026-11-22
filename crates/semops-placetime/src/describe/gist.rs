//! The same things described in gist 14.1.0 and the placetime ontology built on it
//! (`ontology/placetime.ttl`).
//!
//! Every interval is a `gist:TimeInterval` with the start date time, end date time and
//! duration gist requires of one, and every interval a document names carries those three,
//! so a document's intervals can be checked and queried alike. A calendar interval also
//! says which calendar defines it, and, for halves, quarters, months and days, which half,
//! quarter, month or day of the week it is. The topic of a document adds what it is part
//! of, what it has as parts, what precedes and follows it, and the URI set it belongs to.
//!
//! gist defines no inverse properties, so containment and order are each stated one way:
//! the part `gist:isDirectPartOf` (or, further up, `gist:isPartOf`) the whole, and the
//! earlier interval `gist:precedesDirectly` the later.

use super::{
    Builder, aligned_text, containers, example, instant_text, interval_text, parts, pattern,
    point_text, set_comment, set_label, subsets,
};
use crate::calendar::{Calendar, Duration, Moment, weekday};
use crate::path::{Aligned, Set, Thing, Unit};
use crate::place::Point;
use crate::vocab::{geo, gist, gistd, pt, ptd, ptx, skos, void};
use oxrdf::vocab::{rdf, rdfs, xsd};
use oxrdf::{BlankNode, Literal, NamedNode, NamedNodeRef};

fn class(unit: Unit) -> NamedNodeRef<'static> {
    match unit {
        Unit::Year => pt::CALENDAR_YEAR,
        Unit::Half => pt::CALENDAR_HALF_YEAR,
        Unit::Quarter => pt::CALENDAR_QUARTER,
        Unit::Month => pt::CALENDAR_MONTH,
        Unit::Week => pt::ISO8601_WEEK,
        Unit::Day => pt::CALENDAR_DAY,
        Unit::Hour => pt::CALENDAR_HOUR,
        Unit::Minute => pt::CALENDAR_MINUTE,
        Unit::Second => pt::CALENDAR_SECOND,
    }
}

fn calendar(calendar: Calendar) -> (NamedNodeRef<'static>, &'static str) {
    match calendar {
        Calendar::British => (ptd::CALENDAR_BRITISH, "British calendar"),
        Calendar::Gregorian => (ptd::CALENDAR_GREGORIAN, "Gregorian calendar"),
        Calendar::Julian => (ptd::CALENDAR_JULIAN, "Julian calendar"),
    }
}

/// The units an interval of `unit` is directly part of: the rest of [`containers`] hold it
/// through one of these.
fn directly_within(unit: Unit) -> &'static [Unit] {
    match unit {
        Unit::Year | Unit::Week => &[],
        Unit::Half => &[Unit::Year],
        Unit::Quarter => &[Unit::Half],
        Unit::Month => &[Unit::Quarter],
        Unit::Day => &[Unit::Month, Unit::Week],
        Unit::Hour => &[Unit::Day],
        Unit::Minute => &[Unit::Hour],
        Unit::Second => &[Unit::Minute],
    }
}

fn part_of(part: Unit, whole: Unit) -> NamedNodeRef<'static> {
    if directly_within(part).contains(&whole) {
        gist::IS_DIRECT_PART_OF
    } else {
        gist::IS_PART_OF
    }
}

impl Builder<'_> {
    fn pref_label(&mut self, node: &NamedNode, label: &str) {
        self.add(node, skos::PREF_LABEL, &Literal::new_simple_literal(label));
    }

    fn description(&mut self, node: &NamedNode, text: &str) {
        self.add(node, gist::DESCRIPTION, &Literal::new_simple_literal(text));
    }

    /// A duration of `seconds`, as a gist magnitude: one node for each length in a document,
    /// shared by every interval that long.
    fn duration(&mut self, seconds: i64) -> BlankNode {
        if let Some(node) = self.durations.get(&seconds) {
            return node.clone();
        }
        let node = BlankNode::default();
        self.add(&node, rdf::TYPE, gist::MAGNITUDE);
        self.add(&node, gist::HAS_ASPECT, gistd::ASPECT_DURATION);
        self.add(
            &node,
            gist::HAS_UNIT_OF_MEASURE,
            gistd::UNIT_OF_MEASURE_SECOND,
        );
        self.add(
            &node,
            gist::NUMERIC_VALUE,
            &Literal::new_typed_literal(seconds.to_string(), xsd::INTEGER),
        );
        self.durations.insert(seconds, node.clone());
        node
    }

    /// The start, end and duration gist requires of a time interval. The end is the moment
    /// the interval stops, so the duration is the end less the start.
    fn span(&mut self, node: &NamedNode, start: Moment, end: Moment) {
        if let Some(lexical) = start.xsd_date_time() {
            self.typed(node, gist::START_DATE_TIME, &lexical, xsd::DATE_TIME);
        }
        if let Some(lexical) = end.xsd_date_time() {
            self.typed(node, gist::END_DATE_TIME, &lexical, xsd::DATE_TIME);
        }
        let magnitude = self.duration(end.seconds() - start.seconds());
        self.add(node, gist::HAS_MAGNITUDE, &magnitude);
    }

    fn categorise(&mut self, node: &NamedNode, category: Option<NamedNodeRef<'_>>, label: &str) {
        if let Some(category) = category {
            self.add(node, gist::IS_CATEGORIZED_BY, category);
            self.add(
                category,
                skos::PREF_LABEL,
                &Literal::new_simple_literal(label),
            );
        }
    }

    /// A calendar interval as any document names it: what it is, its calendar, its
    /// categories, and its span.
    fn gist_aligned_core(&mut self, a: &Aligned) -> NamedNode {
        let node = self.aligned_id(a);
        self.kind(&node, class(a.unit));
        let (label, text) = aligned_text(a);
        self.pref_label(&node, &label);
        self.description(&node, &text);
        let (cal, cal_label) = calendar(a.calendar);
        self.add(&node, gist::CONFORMS_TO, cal);
        self.add(
            cal,
            skos::PREF_LABEL,
            &Literal::new_simple_literal(cal_label),
        );
        let month = a.calendar.date(a.start.day).month;
        match a.unit {
            Unit::Half => {
                let half = (month - 1) / 6 + 1;
                let label = if half == 1 {
                    "first half"
                } else {
                    "second half"
                };
                self.categorise(&node, ptx::half(half), label);
            }
            Unit::Quarter => {
                let quarter = (month - 1) / 3 + 1;
                let label = match quarter {
                    1 => "first quarter",
                    2 => "second quarter",
                    3 => "third quarter",
                    _ => "fourth quarter",
                };
                self.categorise(&node, ptx::quarter(quarter), label);
            }
            Unit::Month => {
                self.categorise(&node, ptx::month(month), super::month_name(month));
            }
            Unit::Day => {
                let day = weekday(a.start.day);
                self.categorise(&node, ptx::weekday(day), super::weekday_name(a.start.day));
            }
            Unit::Year | Unit::Week | Unit::Hour | Unit::Minute | Unit::Second => {}
        }
        if let Some(end) = a.end() {
            self.span(&node, a.start, end);
        }
        node
    }

    fn gist_aligned(&mut self, a: &Aligned) -> NamedNode {
        let node = self.gist_aligned_core(a);
        let set = self.gist_set_labels(&Set::Unit(a.calendar, a.unit));
        self.link(&node, gist::IS_MEMBER_OF, &set);
        if let Some(next) = a.next() {
            let next = self.gist_aligned_core(&next);
            self.link(&node, gist::PRECEDES_DIRECTLY, &next);
        }
        if let Some(previous) = a.previous() {
            let previous = self.gist_aligned_core(&previous);
            self.link(&previous, gist::PRECEDES_DIRECTLY, &node);
        }
        for &unit in containers(a.unit) {
            if let Some(whole) = Aligned::holding(a.calendar, unit, a.start) {
                let whole = self.gist_aligned_core(&whole);
                self.link(&node, part_of(a.unit, unit), &whole);
            }
        }
        for &unit in parts(a.unit) {
            for child in a.children(unit) {
                let child = self.gist_aligned_core(&child);
                self.link(&child, part_of(unit, a.unit), &node);
            }
        }
        node
    }

    fn gist_instant(&mut self, at: Moment) -> NamedNode {
        let node = self.instant_id(at);
        self.kind(&node, pt::INSTANT);
        let (label, text) = instant_text(at);
        self.pref_label(&node, &label);
        self.description(&node, &text);
        self.span(&node, at, at);
        node
    }

    fn gist_interval(&mut self, start: Moment, duration: &Duration) -> NamedNode {
        let node = self.interval_id(start, duration);
        self.kind(&node, gist::TIME_INTERVAL);
        let (label, text) = interval_text(start, duration);
        self.pref_label(&node, &label);
        self.description(&node, &text);
        if let Some(end) = Calendar::Gregorian.add(start, duration) {
            self.span(&node, start, end);
        }
        node
    }

    /// A point, as gist describes one, with its WKT beside for GeoSPARQL.
    fn gist_point(&mut self, point: &Point) -> NamedNode {
        let node = self.id(&Thing::Point(point.clone()).path());
        self.kind(&node, gist::GEO_POINT);
        self.pref_label(&node, &point.label());
        self.description(&node, &point_text(point));
        self.typed(&node, gist::LATITUDE, point.latitude.as_str(), xsd::DOUBLE);
        self.typed(
            &node,
            gist::LONGITUDE,
            point.longitude.as_str(),
            xsd::DOUBLE,
        );
        self.typed(&node, geo::AS_WKT, &point.wkt(), geo::WKT_LITERAL);
        self.link(
            &node,
            rdfs::SEE_ALSO,
            &NamedNode::new_unchecked(point.geo_uri()),
        );
        node
    }

    fn gist_set_labels(&mut self, set: &Set) -> NamedNode {
        let node = self.id(&set.path());
        self.kind(&node, gist::COLLECTION);
        self.kind(&node, void::DATASET);
        self.pref_label(&node, &set_label(set));
        node
    }

    fn gist_set(&mut self, set: &Set) -> NamedNode {
        let node = self.gist_set_labels(set);
        self.description(&node, &set_comment(set));
        for subset in subsets(set) {
            let subset = self.gist_set_labels(&subset);
            self.link(&node, void::SUBSET, &subset);
        }
        if let Some(pattern) = pattern(set) {
            let regex = self.uri_regex(&pattern);
            self.typed(&node, void::URI_REGEX_PATTERN, &regex, xsd::STRING);
        }
        if let Some(example) = example(set) {
            let example = self.gist_mention(&example);
            self.link(&node, void::EXAMPLE_RESOURCE, &example);
        }
        node
    }

    /// Something another document names, described as fully as it is anywhere but its own
    /// document.
    fn gist_mention(&mut self, thing: &Thing) -> NamedNode {
        match thing {
            Thing::Aligned(a) => self.gist_aligned_core(a),
            Thing::Instant(at) => self.gist_instant(*at),
            Thing::Interval { start, duration } => self.gist_interval(*start, duration),
            Thing::Point(point) => self.gist_point(point),
            Thing::Set(set) => self.gist_set_labels(set),
        }
    }

    pub(super) fn gist(&mut self, thing: &Thing) -> NamedNode {
        let (node, set) = match thing {
            Thing::Aligned(a) => return self.gist_aligned(a),
            Thing::Set(set) => return self.gist_set(set),
            Thing::Instant(at) => (self.gist_instant(*at), Set::Instants),
            Thing::Interval { start, duration } => {
                (self.gist_interval(*start, duration), Set::Intervals)
            }
            Thing::Point(point) => (self.gist_point(point), Set::Points),
        };
        let set = self.gist_set_labels(&set);
        self.link(&node, gist::IS_MEMBER_OF, &set);
        node
    }
}
