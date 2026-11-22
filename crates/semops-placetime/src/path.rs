//! What a URI names, and how each thing is spelled in one.
//!
//! The patterns are reference.data.gov.uk's: `year/2006`, `quarter/2006-Q1`,
//! `gregorian-day/2006-01-31`, `gregorian-interval/2006-01-01T00:00:00/P3M`, and so on, with
//! `julian-…` beside `gregorian-…`, and `geopoint/wgs84/X…Y…` for points.

use crate::calendar::{Calendar, DAY, Duration, Moment, in_range};
use crate::place::Point;

/// The length of a calendar-aligned interval.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Unit {
    Year,
    Half,
    Quarter,
    Month,
    Week,
    Day,
    Hour,
    Minute,
    Second,
}

impl Unit {
    pub const ALL: [Self; 9] = [
        Self::Year,
        Self::Half,
        Self::Quarter,
        Self::Month,
        Self::Week,
        Self::Day,
        Self::Hour,
        Self::Minute,
        Self::Second,
    ];

    /// The unit's name in a URI segment.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Year => "year",
            Self::Half => "half",
            Self::Quarter => "quarter",
            Self::Month => "month",
            Self::Week => "week",
            Self::Day => "day",
            Self::Hour => "hour",
            Self::Minute => "minute",
            Self::Second => "second",
        }
    }

    /// The unit's name in a label: "British Quarter:2006-Q1".
    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            Self::Year => "Year",
            Self::Half => "Half",
            Self::Quarter => "Quarter",
            Self::Month => "Month",
            Self::Week => "Week",
            Self::Day => "Day",
            Self::Hour => "Hour",
            Self::Minute => "Minute",
            Self::Second => "Second",
        }
    }

    /// The unit's nominal duration. A British or Julian month is a month of that calendar,
    /// so the Gregorian interval it equals may be spelled differently; see
    /// [`Aligned::gregorian_duration`].
    #[must_use]
    pub fn duration(self) -> Duration {
        match self {
            Self::Year => Duration::years(1),
            Self::Half => Duration::months(6),
            Self::Quarter => Duration::months(3),
            Self::Month => Duration::months(1),
            Self::Week => Duration::days(7),
            Self::Day => Duration::days(1),
            Self::Hour => Duration {
                hours: 1,
                ..Duration::default()
            },
            Self::Minute => Duration {
                minutes: 1,
                ..Duration::default()
            },
            Self::Second => Duration::seconds(1),
        }
    }
}

/// An interval aligned with a calendar: a year, a quarter, a day of it, and so on, named by
/// its calendar, its unit and the moment it starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Aligned {
    pub calendar: Calendar,
    pub unit: Unit,
    pub start: Moment,
}

impl Aligned {
    /// The interval of `unit` holding `moment`.
    #[must_use]
    pub fn holding(calendar: Calendar, unit: Unit, moment: Moment) -> Option<Self> {
        let d = calendar.date(moment.day);
        let day = match unit {
            Unit::Year => calendar.days(d.year, 1, 1)?,
            Unit::Half => calendar.days(d.year, (d.month - 1) / 6 * 6 + 1, 1)?,
            Unit::Quarter => calendar.days(d.year, (d.month - 1) / 3 * 3 + 1, 1)?,
            Unit::Month => calendar.days(d.year, d.month, 1)?,
            Unit::Week => {
                let (year, week) = calendar.week_of(moment.day);
                calendar.week_start(year, week)?
            }
            Unit::Day | Unit::Hour | Unit::Minute | Unit::Second => moment.day,
        };
        let second = match unit {
            Unit::Hour => moment.second / 3600 * 3600,
            Unit::Minute => moment.second / 60 * 60,
            Unit::Second => moment.second,
            _ => 0,
        };
        Some(Self {
            calendar,
            unit,
            start: Moment { day, second },
        })
    }

    /// The moment the interval ends, which is the moment the next one starts.
    #[must_use]
    pub fn end(&self) -> Option<Moment> {
        let cal = self.calendar;
        let d = cal.date(self.start.day);
        let months = |n: i64| {
            let total = d.year * 12 + i64::from(d.month) - 1 + n;
            let month = u32::try_from(total.rem_euclid(12)).ok()? + 1;
            cal.days(total.div_euclid(12), month, 1)
                .map(Moment::midnight)
        };
        match self.unit {
            Unit::Year => cal.days(d.year + 1, 1, 1).map(Moment::midnight),
            Unit::Half => months(6),
            Unit::Quarter => months(3),
            Unit::Month => months(1),
            Unit::Week => self.checked(self.start.plus_seconds(7 * DAY)),
            Unit::Day => self.checked(self.start.plus_seconds(DAY)),
            Unit::Hour => self.checked(self.start.plus_seconds(3600)),
            Unit::Minute => self.checked(self.start.plus_seconds(60)),
            Unit::Second => self.checked(self.start.plus_seconds(1)),
        }
    }

    fn checked(&self, moment: Moment) -> Option<Moment> {
        in_range(self.calendar.date(moment.day).year).then_some(moment)
    }

    /// The next interval of the same unit, if it is in range.
    #[must_use]
    pub fn next(&self) -> Option<Self> {
        let start = self.end()?;
        let next = Self { start, ..*self };
        next.end().map(|_| next)
    }

    /// The previous interval of the same unit, if it is in range.
    #[must_use]
    pub fn previous(&self) -> Option<Self> {
        let before = self.start.plus_seconds(-1);
        if !in_range(self.calendar.date(before.day).year) {
            return None;
        }
        Self::holding(self.calendar, self.unit, before)
    }

    /// The intervals of a finer unit this one is made of, in order.
    #[must_use]
    pub fn children(&self, unit: Unit) -> Vec<Self> {
        let Some(end) = self.end() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut at = Self::holding(self.calendar, unit, self.start);
        while let Some(child) = at {
            if child.start >= end {
                break;
            }
            out.push(child);
            at = child.next();
        }
        out
    }

    /// The URI segment of the interval's set: `quarter`, `gregorian-quarter`.
    #[must_use]
    pub fn segment(&self) -> String {
        format!("{}{}", self.calendar.prefix(), self.unit.name())
    }

    /// How the URI spells the interval within its set: `2006-Q1`, `2006-W05`, `2006-01-31T09`.
    #[must_use]
    pub fn reference(&self) -> String {
        let d = self.calendar.date(self.start.day);
        let s = self.start;
        let day = format!("{:04}-{:02}-{:02}", d.year, d.month, d.day);
        match self.unit {
            Unit::Year => format!("{:04}", d.year),
            Unit::Half => format!("{:04}-H{}", d.year, (d.month - 1) / 6 + 1),
            Unit::Quarter => format!("{:04}-Q{}", d.year, (d.month - 1) / 3 + 1),
            Unit::Month => format!("{:04}-{:02}", d.year, d.month),
            Unit::Week => {
                let (year, week) = self.calendar.week_of(s.day);
                format!("{year:04}-W{week:02}")
            }
            Unit::Day => day,
            Unit::Hour => format!("{day}T{:02}", s.hour()),
            Unit::Minute => format!("{day}T{:02}:{:02}", s.hour(), s.minute()),
            Unit::Second => format!(
                "{day}T{:02}:{:02}:{:02}",
                s.hour(),
                s.minute(),
                s.second_of_minute()
            ),
        }
    }

    /// The duration of the Gregorian interval this one equals: the unit's own duration where
    /// adding it in the Gregorian calendar lands on the same end, otherwise the Gregorian
    /// span. The British 1752 is `P11M20D` from 12 January (Gregorian) to 1 January 1753.
    #[must_use]
    pub fn gregorian_duration(&self) -> Option<Duration> {
        let end = self.end()?;
        let nominal = self.unit.duration();
        if Calendar::Gregorian.add(self.start, &nominal) == Some(end) {
            Some(nominal)
        } else {
            Calendar::Gregorian.between(self.start, end)
        }
    }
}

/// A URI set: a collection of things the service names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Set {
    /// `interval`: every calendar's intervals.
    All,
    /// `uk-calendar`, `gregorian-calendar`, `julian-calendar`.
    Calendar(Calendar),
    /// `year`, `gregorian-month`, `julian-week` …
    Unit(Calendar, Unit),
    /// `gregorian-instant`.
    Instants,
    /// `gregorian-interval`.
    Intervals,
    /// `geopoint/wgs84`.
    Points,
}

impl Set {
    #[must_use]
    pub fn path(&self) -> String {
        match self {
            Self::All => "interval".to_owned(),
            Self::Calendar(c) => c.set_segment().to_owned(),
            Self::Unit(c, u) => format!("{}{}", c.prefix(), u.name()),
            Self::Instants => INSTANT.to_owned(),
            Self::Intervals => INTERVAL.to_owned(),
            Self::Points => POINT.to_owned(),
        }
    }
}

/// Anything a URI here names.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Thing {
    Aligned(Aligned),
    /// An instant on the Gregorian timeline, to the second.
    Instant(Moment),
    /// An interval on the Gregorian timeline: where it starts, and how long it lasts.
    Interval {
        start: Moment,
        duration: Duration,
    },
    Point(Point),
    Set(Set),
}

const INSTANT: &str = "gregorian-instant";
const INTERVAL: &str = "gregorian-interval";
const POINT: &str = "geopoint/wgs84";

impl Thing {
    /// The thing's canonical path, after `id/` or `doc/`.
    #[must_use]
    pub fn path(&self) -> String {
        match self {
            Self::Aligned(a) => format!("{}/{}", a.segment(), a.reference()),
            Self::Instant(m) => format!("{INSTANT}/{}", m.xsd_date_time().unwrap_or_default()),
            Self::Interval { start, duration } => format!(
                "{INTERVAL}/{}/{duration}",
                start.xsd_date_time().unwrap_or_default()
            ),
            Self::Point(p) => format!("{POINT}/{}", p.reference()),
            Self::Set(s) => s.path(),
        }
    }

    /// What a path names, or `None`. A path naming something in a spelling other than its
    /// canonical one (`P0Y3M` for `P3M`, `X-01.50` for `X-1.5`) is still understood: compare
    /// [`Thing::path`] with what was asked for to tell.
    #[must_use]
    pub fn parse(path: &str) -> Option<Self> {
        if let Some(rest) = path.strip_prefix(POINT) {
            return match rest.strip_prefix('/') {
                None if rest.is_empty() => Some(Self::Set(Set::Points)),
                Some(reference) => Point::parse(reference).map(Self::Point),
                None => None,
            };
        }
        let Some((segment, reference)) = path.split_once('/') else {
            return set(path).map(Self::Set);
        };
        match segment {
            INSTANT => {
                let mut c = Cursor(reference);
                let start = c.moment(Calendar::Gregorian, Unit::Second)?;
                c.done()?;
                Some(Self::Instant(start))
            }
            INTERVAL => {
                let (when, length) = reference.split_once('/')?;
                let mut c = Cursor(when);
                let start = c.moment(Calendar::Gregorian, Unit::Second)?;
                c.done()?;
                let duration = Duration::parse(length)?;
                if duration.is_zero() {
                    return None;
                }
                Calendar::Gregorian.add(start, &duration)?;
                Some(Self::Interval { start, duration })
            }
            _ => {
                let (calendar, unit) = aligned_set(segment)?;
                let mut c = Cursor(reference);
                let start = c.moment(calendar, unit)?;
                c.done()?;
                let aligned = Aligned {
                    calendar,
                    unit,
                    start,
                };
                aligned.end()?;
                Some(Self::Aligned(aligned))
            }
        }
    }
}

fn aligned_set(segment: &str) -> Option<(Calendar, Unit)> {
    Calendar::ALL.into_iter().find_map(|calendar| {
        let name = segment.strip_prefix(calendar.prefix())?;
        Unit::ALL
            .into_iter()
            .find(|u| u.name() == name)
            .map(|unit| (calendar, unit))
    })
}

fn set(segment: &str) -> Option<Set> {
    match segment {
        "interval" => Some(Set::All),
        INSTANT => Some(Set::Instants),
        INTERVAL => Some(Set::Intervals),
        _ => Calendar::ALL
            .into_iter()
            .find(|c| c.set_segment() == segment)
            .map(Set::Calendar)
            .or_else(|| aligned_set(segment).map(|(c, u)| Set::Unit(c, u))),
    }
}

/// Reads a reference left to right, each step taking exactly what the pattern allows.
struct Cursor<'a>(&'a str);

impl Cursor<'_> {
    fn literal(&mut self, text: &str) -> Option<()> {
        self.0 = self.0.strip_prefix(text)?;
        Some(())
    }

    fn digits(&mut self, count: usize) -> Option<u32> {
        let head = self.0.get(..count)?;
        if !head.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        self.0 = &self.0[count..];
        head.parse().ok()
    }

    /// `([0-9]{4})|([1-9][0-9]{3,})`: four digits, or more with no leading zero.
    fn year(&mut self) -> Option<i64> {
        let n = self.0.bytes().take_while(u8::is_ascii_digit).count();
        if n < 4 || (n > 4 && self.0.starts_with('0')) || n > 7 {
            return None;
        }
        let year = self.0[..n].parse().ok()?;
        self.0 = &self.0[n..];
        in_range(year).then_some(year)
    }

    fn done(&self) -> Option<()> {
        self.0.is_empty().then_some(())
    }

    /// The start of a `unit` of `calendar`, as its reference spells it.
    fn moment(&mut self, calendar: Calendar, unit: Unit) -> Option<Moment> {
        let year = self.year()?;
        let first = |month: u32| calendar.days(year, month, 1).map(Moment::midnight);
        match unit {
            Unit::Year => first(1),
            Unit::Half => {
                self.literal("-H")?;
                let half = self.digits(1).filter(|h| (1..=2).contains(h))?;
                first((half - 1) * 6 + 1)
            }
            Unit::Quarter => {
                self.literal("-Q")?;
                let quarter = self.digits(1).filter(|q| (1..=4).contains(q))?;
                first((quarter - 1) * 3 + 1)
            }
            Unit::Month => {
                self.literal("-")?;
                first(self.digits(2)?)
            }
            Unit::Week => {
                self.literal("-W")?;
                calendar
                    .week_start(year, self.digits(2)?)
                    .map(Moment::midnight)
            }
            Unit::Day | Unit::Hour | Unit::Minute | Unit::Second => {
                self.literal("-")?;
                let month = self.digits(2)?;
                self.literal("-")?;
                let day = calendar.days(year, month, self.digits(2)?)?;
                if unit == Unit::Day {
                    return Some(Moment::midnight(day));
                }
                self.literal("T")?;
                let hour = self.digits(2).filter(|h| *h < 24)?;
                let minute = if unit == Unit::Hour {
                    0
                } else {
                    self.literal(":")?;
                    self.digits(2).filter(|m| *m < 60)?
                };
                let second = if unit == Unit::Second {
                    self.literal(":")?;
                    self.digits(2).filter(|s| *s < 60)?
                } else {
                    0
                };
                Some(Moment {
                    day,
                    second: hour * 3600 + minute * 60 + second,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(path: &str) {
        let thing = Thing::parse(path).unwrap_or_else(|| panic!("{path} should parse"));
        assert_eq!(thing.path(), path);
    }

    #[test]
    fn every_pattern_round_trips() {
        for path in [
            "year/2006",
            "half/2006-H2",
            "quarter/2006-Q1",
            "month/2006-02",
            "week/2006-W52",
            "day/1752-09-14",
            "hour/2006-01-31T23",
            "minute/2006-01-31T23:59",
            "second/2006-01-31T23:59:59",
            "gregorian-year/0001",
            "gregorian-week/2004-W53",
            "julian-day/1582-10-04",
            "gregorian-instant/2006-01-01T00:00:00",
            "gregorian-interval/2016-06-30T00:00:00/P3M",
            "year/12345",
            "interval",
            "uk-calendar",
            "gregorian-calendar",
            "julian-calendar",
            "gregorian-month",
            "quarter",
            "gregorian-instant",
            "gregorian-interval",
            "geopoint/wgs84",
            "geopoint/wgs84/X-1.5Y51.25",
        ] {
            round_trip(path);
        }
    }

    #[test]
    fn what_does_not_exist_is_refused() {
        for path in [
            "year/206",
            "year/02006",
            "year/0000",
            "half/2006-H3",
            "quarter/2006-Q0",
            "month/2006-13",
            "day/2006-02-29",
            "day/1752-09-05",
            "week/2005-W53",
            "hour/2006-01-01T24",
            "second/2006-01-01T00:00:60",
            "gregorian-instant/2006-01-01T00:00",
            "gregorian-interval/2006-01-01T00:00:00/P",
            "gregorian-interval/2006-01-01T00:00:00/P0D",
            "gregorian-interval/2006-01-01T00:00:00",
            "government-year/2006",
            "instant/2006-01-01T00:00:00",
            "year/2006/",
            "year/999999",
            "",
        ] {
            assert_eq!(Thing::parse(path), None, "{path}");
        }
        // The Julian and Gregorian calendars have the days the British one dropped.
        assert!(Thing::parse("julian-day/1752-09-05").is_some());
        assert!(Thing::parse("gregorian-day/1752-09-05").is_some());
    }

    #[test]
    fn a_non_canonical_spelling_is_understood_and_corrected() {
        let thing = Thing::parse("gregorian-interval/2006-01-01T00:00:00/P0Y03M").unwrap();
        assert_eq!(thing.path(), "gregorian-interval/2006-01-01T00:00:00/P3M");
    }

    #[test]
    fn a_british_month_of_1752_holds_nineteen_days() {
        let Some(Thing::Aligned(month)) = Thing::parse("month/1752-09") else {
            panic!()
        };
        let days: Vec<_> = month
            .children(Unit::Day)
            .iter()
            .map(Aligned::reference)
            .collect();
        assert_eq!(days.len(), 19);
        assert_eq!(days[1], "1752-09-02");
        assert_eq!(days[2], "1752-09-14");
        let next = month.next().unwrap();
        assert_eq!(next.reference(), "1752-10");
        assert_eq!(next.previous().unwrap(), month);
    }

    #[test]
    fn neighbours_cross_their_boundaries() {
        let Some(Thing::Aligned(hour)) = Thing::parse("gregorian-hour/2006-01-01T00") else {
            panic!()
        };
        assert_eq!(hour.previous().unwrap().reference(), "2005-12-31T23");
        assert_eq!(hour.next().unwrap().reference(), "2006-01-01T01");
        let Some(Thing::Aligned(week)) = Thing::parse("gregorian-week/2004-W53") else {
            panic!()
        };
        assert_eq!(week.next().unwrap().reference(), "2005-W01");
        let Some(Thing::Aligned(first)) = Thing::parse("gregorian-year/0001") else {
            panic!()
        };
        assert_eq!(first.previous(), None);
    }

    #[test]
    fn the_gregorian_equivalent_of_a_british_interval() {
        let Some(Thing::Aligned(year)) = Thing::parse("year/1752") else {
            panic!()
        };
        assert_eq!(year.start.xsd_date_time().unwrap(), "1752-01-12T00:00:00");
        assert_eq!(year.gregorian_duration().unwrap().to_string(), "P11M20D");
        let Some(Thing::Aligned(feb)) = Thing::parse("month/1700-02") else {
            panic!()
        };
        // Julian February 1700 had 29 days; the Gregorian month from 11 February had 28.
        assert_eq!(feb.gregorian_duration().unwrap().to_string(), "P1M1D");
        let Some(Thing::Aligned(q)) = Thing::parse("quarter/2006-Q1") else {
            panic!()
        };
        assert_eq!(q.gregorian_duration().unwrap().to_string(), "P3M");
    }
}
