//! Calendar arithmetic on day numbers.
//!
//! Every date is a day number, counted from 1970-01-01 in the proleptic Gregorian calendar.
//! A day number names the same day whatever calendar it is read in, which is what lets a
//! British or Julian interval say which Gregorian interval it equals: the moment is the same,
//! only its spelling differs.

use std::fmt;

/// The latest year a URI may name. The URI patterns allow any number of digits; this bound
/// keeps every sum below well inside `i64` and every URI a sensible length.
pub const MAX_YEAR: i64 = 999_999;

/// Seconds in a day. The URI sets are unzoned, so every day has exactly this many.
pub const DAY: i64 = 86_400;

/// A calendar the URI sets are aligned with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Calendar {
    /// The Gregorian calendar for every date, before 1582 too.
    Gregorian,
    /// The Julian calendar for every date, after 1582 too.
    Julian,
    /// The calendar in force in Great Britain and its colonies: Julian until Wednesday
    /// 2 September 1752 and Gregorian from the next day, Thursday 14 September 1752, under
    /// the Calendar (New Style) Act 1750. The year is taken to start on 1 January throughout,
    /// as IntervalServer took it, though the legal year began on 25 March until 1751.
    British,
}

/// A calendar date. Which calendar it is in is the caller's to know.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    pub year: i64,
    pub month: u32,
    pub day: u32,
}

/// A moment, to the second: a day number and the seconds since that day's midnight.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Moment {
    pub day: i64,
    pub second: u32,
}

impl Moment {
    #[must_use]
    pub fn midnight(day: i64) -> Self {
        Self { day, second: 0 }
    }

    /// Seconds since 1970-01-01T00:00:00.
    #[must_use]
    pub fn seconds(self) -> i64 {
        self.day * DAY + i64::from(self.second)
    }

    #[must_use]
    pub fn from_seconds(seconds: i64) -> Self {
        Self {
            day: seconds.div_euclid(DAY),
            // In 0..86_400 by construction.
            second: u32::try_from(seconds.rem_euclid(DAY)).unwrap_or(0),
        }
    }

    #[must_use]
    pub fn plus_seconds(self, seconds: i64) -> Self {
        Self::from_seconds(self.seconds() + seconds)
    }

    #[must_use]
    pub fn hour(self) -> u32 {
        self.second / 3600
    }

    #[must_use]
    pub fn minute(self) -> u32 {
        self.second / 60 % 60
    }

    #[must_use]
    pub fn second_of_minute(self) -> u32 {
        self.second % 60
    }

    /// The `xsd:dateTime` lexical form, without a timezone, in the Gregorian calendar, which
    /// is the calendar XML Schema dates are in. `None` before year 1 or after [`MAX_YEAR`].
    #[must_use]
    pub fn xsd_date_time(self) -> Option<String> {
        let d = Calendar::Gregorian.date(self.day);
        in_range(d.year).then(|| {
            format!(
                "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
                d.year,
                d.month,
                d.day,
                self.hour(),
                self.minute(),
                self.second_of_minute()
            )
        })
    }

    /// The `xsd:date` lexical form of the moment's day, in the Gregorian calendar.
    #[must_use]
    pub fn xsd_date(self) -> Option<String> {
        let d = Calendar::Gregorian.date(self.day);
        in_range(d.year).then(|| format!("{:04}-{:02}-{:02}", d.year, d.month, d.day))
    }
}

#[must_use]
pub fn in_range(year: i64) -> bool {
    (1..=MAX_YEAR).contains(&year)
}

/// The day of the week, Monday 1 to Sunday 7. 1970-01-01 was a Thursday, and the week runs
/// on unbroken through every change of calendar.
#[must_use]
pub fn weekday(day: i64) -> u32 {
    u32::try_from((day + 3).rem_euclid(7)).unwrap_or(0) + 1
}

/// Day number of a proleptic Gregorian date (Howard Hinnant's `days_from_civil`).
#[must_use]
pub fn gregorian_days(year: i64, month: u32, day: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = i64::from(month);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The proleptic Gregorian date of a day number (Hinnant's `civil_from_days`).
#[must_use]
pub fn gregorian_date(days: i64) -> Date {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    Date {
        year,
        month: u32::try_from(month).unwrap_or(1),
        day: u32::try_from(day).unwrap_or(1),
    }
}

/// Day number of a proleptic Julian date, through the Julian day number.
#[must_use]
pub fn julian_days(year: i64, month: u32, day: u32) -> i64 {
    let a = (14 - i64::from(month)) / 12;
    let y = year + 4800 - a;
    let m = i64::from(month) + 12 * a - 3;
    let jdn = i64::from(day) + (153 * m + 2) / 5 + 365 * y + y.div_euclid(4) - 32_083;
    jdn - 2_440_588
}

/// The proleptic Julian date of a day number.
#[must_use]
pub fn julian_date(days: i64) -> Date {
    let c = days + 2_440_588 + 32_082;
    let d = (4 * c + 3).div_euclid(1461);
    let e = c - (1461 * d).div_euclid(4);
    let m = (5 * e + 2) / 153;
    let day = e - (153 * m + 2) / 5 + 1;
    let month = m + 3 - 12 * (m / 10);
    let year = d - 4800 + m / 10;
    Date {
        year,
        month: u32::try_from(month).unwrap_or(1),
        day: u32::try_from(day).unwrap_or(1),
    }
}

/// The last British date in the Julian calendar, and the first in the Gregorian.
const BRITISH_LAST_JULIAN: Date = Date {
    year: 1752,
    month: 9,
    day: 2,
};
const BRITISH_FIRST_GREGORIAN: Date = Date {
    year: 1752,
    month: 9,
    day: 14,
};

impl Calendar {
    /// The calendar's name as labels spell it: "British Year:2006".
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Gregorian => "Gregorian",
            Self::Julian => "Julian",
            Self::British => "British",
        }
    }

    /// What goes before a unit's name in a URI segment: `gregorian-year`, `julian-year`, and
    /// the British calendar's plain `year`.
    #[must_use]
    pub fn prefix(self) -> &'static str {
        match self {
            Self::Gregorian => "gregorian-",
            Self::Julian => "julian-",
            Self::British => "",
        }
    }

    /// The segment of the URI set holding every interval aligned with this calendar.
    #[must_use]
    pub fn set_segment(self) -> &'static str {
        match self {
            Self::Gregorian => "gregorian-calendar",
            Self::Julian => "julian-calendar",
            Self::British => "uk-calendar",
        }
    }

    pub const ALL: [Self; 3] = [Self::British, Self::Gregorian, Self::Julian];

    /// The first day the British calendar is Gregorian.
    fn british_change() -> i64 {
        gregorian_days(
            BRITISH_FIRST_GREGORIAN.year,
            BRITISH_FIRST_GREGORIAN.month,
            BRITISH_FIRST_GREGORIAN.day,
        )
    }

    #[must_use]
    pub fn is_leap(self, year: i64) -> bool {
        let julian = year.rem_euclid(4) == 0;
        let gregorian = julian && (year.rem_euclid(100) != 0 || year.rem_euclid(400) == 0);
        match self {
            Self::Gregorian => gregorian,
            Self::Julian => julian,
            // 1752 was a leap year under both rules, so the change of rule in its September
            // does not disturb February.
            Self::British => {
                if year < 1752 {
                    julian
                } else {
                    gregorian
                }
            }
        }
    }

    /// The month's nominal length. September 1752 in the British calendar is nominally 30
    /// days, eleven of which (3 to 13) never happened; [`Calendar::days`] refuses those.
    #[must_use]
    pub fn month_length(self, year: i64, month: u32) -> u32 {
        match month {
            4 | 6 | 9 | 11 => 30,
            2 if self.is_leap(year) => 29,
            2 => 28,
            _ => 31,
        }
    }

    /// The day number of a date in this calendar, or `None` for a date that does not exist
    /// in it: a 30 February, a British 3 to 13 September 1752, a year out of range.
    #[must_use]
    pub fn days(self, year: i64, month: u32, day: u32) -> Option<i64> {
        if !in_range(year) || !(1..=12).contains(&month) || day == 0 {
            return None;
        }
        if day > self.month_length(year, month) {
            return None;
        }
        let date = Date { year, month, day };
        if self == Self::British && date > BRITISH_LAST_JULIAN && date < BRITISH_FIRST_GREGORIAN {
            return None;
        }
        Some(self.days_unchecked(date))
    }

    /// As [`Calendar::days`], for a date already known to be well formed. A British date in
    /// the eleven missing days is taken to be the first day after them, which is where
    /// adding a month or a year to a date can land.
    #[must_use]
    pub fn days_unchecked(self, date: Date) -> i64 {
        match self {
            Self::Gregorian => gregorian_days(date.year, date.month, date.day),
            Self::Julian => julian_days(date.year, date.month, date.day),
            Self::British => {
                if date >= BRITISH_FIRST_GREGORIAN {
                    gregorian_days(date.year, date.month, date.day)
                } else if date > BRITISH_LAST_JULIAN {
                    Self::british_change()
                } else {
                    julian_days(date.year, date.month, date.day)
                }
            }
        }
    }

    /// The date a day number falls on in this calendar.
    #[must_use]
    pub fn date(self, day: i64) -> Date {
        match self {
            Self::Gregorian => gregorian_date(day),
            Self::Julian => julian_date(day),
            Self::British => {
                if day >= Self::british_change() {
                    gregorian_date(day)
                } else {
                    julian_date(day)
                }
            }
        }
    }

    /// The day of the year, counting the days that happened: in the British 1752,
    /// 14 September is day 247, not day 258.
    #[must_use]
    pub fn day_of_year(self, day: i64) -> u32 {
        let year = self.date(day).year;
        let first = self.days_unchecked(Date {
            year,
            month: 1,
            day: 1,
        });
        u32::try_from(day - first + 1).unwrap_or(0)
    }

    /// The Monday that starts week 1 of a year: the week holding the year's first Thursday,
    /// so the one holding 4 January, as ISO 8601 numbers Gregorian weeks. The same rule
    /// numbers the Julian and British weeks, over the days each calendar has.
    #[must_use]
    pub fn week_one(self, year: i64) -> i64 {
        let fourth = self.days_unchecked(Date {
            year,
            month: 1,
            day: 4,
        });
        fourth - i64::from(weekday(fourth) - 1)
    }

    /// The week-numbering year and week a day falls in.
    #[must_use]
    pub fn week_of(self, day: i64) -> (i64, u32) {
        let year = self.date(day).year;
        let start = if day >= self.week_one(year + 1) {
            year + 1
        } else if day < self.week_one(year) {
            year - 1
        } else {
            year
        };
        let week = (day - self.week_one(start)) / 7 + 1;
        (start, u32::try_from(week).unwrap_or(0))
    }

    /// How many weeks a week-numbering year has: 52 or 53, and 51 for the British 1752,
    /// which lost eleven days.
    #[must_use]
    pub fn weeks_in(self, year: i64) -> u32 {
        u32::try_from((self.week_one(year + 1) - self.week_one(year)) / 7).unwrap_or(0)
    }

    /// The Monday starting a week, or `None` for a week the year does not have.
    #[must_use]
    pub fn week_start(self, year: i64, week: u32) -> Option<i64> {
        (in_range(year) && week >= 1 && week <= self.weeks_in(year))
            .then(|| self.week_one(year) + 7 * i64::from(week - 1))
    }

    /// Adds a duration, a field at a time from years down to seconds, as
    /// `java.util.Calendar.add` does: adding a year or a month keeps the day of the month,
    /// cut back to the length of the month it lands in, so 31 January plus a month is the
    /// last day of February. `None` past [`MAX_YEAR`].
    #[must_use]
    pub fn add(self, start: Moment, duration: &Duration) -> Option<Moment> {
        let mut date = self.date(start.day);
        if duration.years > 0 {
            date.year = date.year.checked_add(i64::from(duration.years))?;
            if !in_range(date.year) {
                return None;
            }
            date.day = date.day.min(self.month_length(date.year, date.month));
        }
        if duration.months > 0 {
            let months = date.year * 12 + i64::from(date.month) - 1 + i64::from(duration.months);
            date.year = months.div_euclid(12);
            date.month = u32::try_from(months.rem_euclid(12)).ok()? + 1;
            if !in_range(date.year) {
                return None;
            }
            date.day = date.day.min(self.month_length(date.year, date.month));
        }
        let day = self.days_unchecked(date) + i64::from(duration.days);
        let seconds = i64::from(duration.hours) * 3600
            + i64::from(duration.minutes) * 60
            + i64::from(duration.seconds);
        let end = Moment {
            day,
            second: start.second,
        }
        .plus_seconds(seconds);
        in_range(self.date(end.day).year).then_some(end)
    }

    /// The duration that, added to `start` in this calendar, reaches `end`: as many whole
    /// years as fit, then months, days, hours, minutes and seconds. `None` unless `end` is
    /// after `start`.
    #[must_use]
    pub fn between(self, start: Moment, end: Moment) -> Option<Duration> {
        if end <= start {
            return None;
        }
        let mut found = Duration::default();
        let mut at = start;

        let (s, e) = (self.date(at.day), self.date(end.day));
        let mut years = u32::try_from(e.year - s.year).unwrap_or(0);
        while years > 0 {
            match self.add(at, &Duration::years(years)) {
                Some(m) if m <= end => break,
                _ => years -= 1,
            }
        }
        if years > 0 {
            at = self.add(at, &Duration::years(years))?;
        }
        found.years = years;

        let (s, e) = (self.date(at.day), self.date(end.day));
        let span = (e.year * 12 + i64::from(e.month)) - (s.year * 12 + i64::from(s.month));
        let mut months = u32::try_from(span).unwrap_or(0);
        while months > 0 {
            match self.add(at, &Duration::months(months)) {
                Some(m) if m <= end => break,
                _ => months -= 1,
            }
        }
        if months > 0 {
            at = self.add(at, &Duration::months(months))?;
        }
        found.months = months;

        let rest = end.seconds() - at.seconds();
        found.days = u32::try_from(rest / DAY).ok()?;
        let rest = rest % DAY;
        found.hours = u32::try_from(rest / 3600).ok()?;
        found.minutes = u32::try_from(rest / 60 % 60).ok()?;
        found.seconds = u32::try_from(rest % 60).ok()?;
        Some(found)
    }
}

/// An `xsd:duration` with no fractions and no sign, which is all the interval URIs allow.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Duration {
    pub years: u32,
    pub months: u32,
    pub days: u32,
    pub hours: u32,
    pub minutes: u32,
    pub seconds: u32,
}

impl Duration {
    #[must_use]
    pub fn years(years: u32) -> Self {
        Self {
            years,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn months(months: u32) -> Self {
        Self {
            months,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn days(days: u32) -> Self {
        Self {
            days,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn seconds(seconds: u32) -> Self {
        Self {
            seconds,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn is_zero(&self) -> bool {
        *self == Self::default()
    }

    /// Parses `P(nY)?(nM)?(nD)?(T(nH)?(nM)?(nS)?)?`, the pattern IntervalServer accepted.
    /// Leading zeros and zero fields are accepted here and dropped from the spelling
    /// [`Duration`]'s `Display` gives, which is the canonical one.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let mut rest = text.strip_prefix('P')?;
        let mut duration = Self::default();
        let mut in_time = false;
        // The designators allowed next, in order; a field may appear once, in its place.
        let mut order = 0;
        while !rest.is_empty() {
            if let Some(after) = rest.strip_prefix('T') {
                if in_time || after.is_empty() {
                    return None;
                }
                in_time = true;
                order = 3;
                rest = after;
                continue;
            }
            let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
            if digits == 0 || digits > 9 {
                return None;
            }
            let value: u32 = rest[..digits].parse().ok()?;
            let designator = rest[digits..].chars().next()?;
            let slot = match (in_time, designator) {
                (false, 'Y') => 0,
                (false, 'M') => 1,
                (false, 'D') => 2,
                (true, 'H') => 3,
                (true, 'M') => 4,
                (true, 'S') => 5,
                _ => return None,
            };
            if slot < order {
                return None;
            }
            order = slot + 1;
            match slot {
                0 => duration.years = value,
                1 => duration.months = value,
                2 => duration.days = value,
                3 => duration.hours = value,
                4 => duration.minutes = value,
                _ => duration.seconds = value,
            }
            rest = &rest[digits + 1..];
        }
        Some(duration)
    }

    /// "3 months ", "1 year 6 months ": the phrase IntervalServer put in an interval's
    /// comment, trailing space included.
    #[must_use]
    pub fn phrase(&self) -> String {
        let mut out = String::new();
        for (n, unit) in [
            (self.years, "year"),
            (self.months, "month"),
            (self.days, "day"),
            (self.hours, "hour"),
            (self.minutes, "minute"),
            (self.seconds, "second"),
        ] {
            if n > 0 {
                out.push_str(&format!("{n} {unit}{} ", if n > 1 { "s" } else { "" }));
            }
        }
        out
    }
}

impl fmt::Display for Duration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_zero() {
            return f.write_str("P0D");
        }
        f.write_str("P")?;
        for (n, d) in [(self.years, 'Y'), (self.months, 'M'), (self.days, 'D')] {
            if n > 0 {
                write!(f, "{n}{d}")?;
            }
        }
        if self.hours > 0 || self.minutes > 0 || self.seconds > 0 {
            f.write_str("T")?;
            for (n, d) in [(self.hours, 'H'), (self.minutes, 'M'), (self.seconds, 'S')] {
                if n > 0 {
                    write!(f, "{n}{d}")?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn day_numbers_round_trip_in_both_calendars() {
        for day in (-800_000..800_000).step_by(997) {
            let g = gregorian_date(day);
            assert_eq!(gregorian_days(g.year, g.month, g.day), day);
            let j = julian_date(day);
            assert_eq!(julian_days(j.year, j.month, j.day), day);
        }
        assert_eq!(gregorian_days(1970, 1, 1), 0);
        assert_eq!(gregorian_days(2000, 3, 1), 11_017);
    }

    #[test]
    fn the_british_change_skips_eleven_days_and_no_weekday() {
        let wednesday = Calendar::British.days(1752, 9, 2).unwrap();
        let thursday = Calendar::British.days(1752, 9, 14).unwrap();
        assert_eq!(thursday, wednesday + 1);
        assert_eq!(weekday(wednesday), 3);
        assert_eq!(weekday(thursday), 4);
        for day in 3..=13 {
            assert_eq!(Calendar::British.days(1752, 9, day), None);
        }
        assert_eq!(julian_days(1752, 9, 2), wednesday);
        assert_eq!(gregorian_days(1752, 9, 14), thursday);
    }

    #[test]
    fn the_julian_calendar_drifts_as_it_should() {
        // Ten days behind in the seventeenth century, eleven after the Julian-only leap day
        // of 1700, thirteen now.
        assert_eq!(julian_days(1700, 1, 1), gregorian_days(1700, 1, 11));
        assert_eq!(julian_days(1700, 3, 1), gregorian_days(1700, 3, 12));
        assert_eq!(julian_days(2000, 1, 1), gregorian_days(2000, 1, 14));
        assert!(Calendar::Julian.is_leap(1900));
        assert!(!Calendar::Gregorian.is_leap(1900));
        assert!(Calendar::British.is_leap(1700));
        assert!(!Calendar::British.is_leap(1800));
    }

    #[test]
    fn iso_weeks_match_iso_8601() {
        let g = Calendar::Gregorian;
        // 2004 has 53 weeks; 2005-W01 starts on 3 January 2005.
        assert_eq!(g.weeks_in(2004), 53);
        assert_eq!(g.weeks_in(2005), 52);
        assert_eq!(g.week_start(2005, 1), Some(gregorian_days(2005, 1, 3)));
        // 1 January 2005 is in 2004-W53; 29 December 2008 is in 2009-W01.
        assert_eq!(g.week_of(gregorian_days(2005, 1, 1)), (2004, 53));
        assert_eq!(g.week_of(gregorian_days(2008, 12, 29)), (2009, 1));
        assert_eq!(g.week_start(2005, 53), None);
    }

    #[test]
    fn the_british_1752_has_fifty_one_weeks() {
        let b = Calendar::British;
        assert_eq!(b.weeks_in(1752), 51);
        let monday = b.week_start(1752, 51).unwrap();
        assert_eq!(b.week_start(1753, 1), Some(monday + 7));
        // The week across the change runs from Monday 31 August to Sunday 17 September.
        let thursday = b.days(1752, 9, 14).unwrap();
        let (year, week) = b.week_of(thursday);
        assert_eq!(b.week_start(year, week), Some(b.days(1752, 8, 31).unwrap()));
        assert_eq!(b.day_of_year(thursday), 247);
    }

    #[test]
    fn adding_months_cuts_back_to_the_months_length() {
        let g = Calendar::Gregorian;
        let jan31 = Moment::midnight(gregorian_days(2016, 1, 31));
        let feb = g.add(jan31, &Duration::months(1)).unwrap();
        assert_eq!(
            gregorian_date(feb.day),
            Date {
                year: 2016,
                month: 2,
                day: 29
            }
        );
        let leap = Moment::midnight(gregorian_days(2016, 2, 29));
        let next = g.add(leap, &Duration::years(1)).unwrap();
        assert_eq!(
            gregorian_date(next.day),
            Date {
                year: 2017,
                month: 2,
                day: 28
            }
        );
    }

    #[test]
    fn between_inverts_add() {
        let g = Calendar::Gregorian;
        let start = Moment::midnight(gregorian_days(1752, 1, 12));
        let end = Moment::midnight(gregorian_days(1753, 1, 1));
        let d = g.between(start, end).unwrap();
        assert_eq!(d.to_string(), "P11M20D");
        assert_eq!(g.add(start, &d), Some(end));
        let end = start.plus_seconds(DAY + 3661);
        assert_eq!(g.between(start, end).unwrap().to_string(), "P1DT1H1M1S");
        assert_eq!(g.between(end, start), None);
    }

    #[test]
    fn durations_parse_and_print_canonically() {
        let d = Duration::parse("P0Y03M").unwrap();
        assert_eq!(d.to_string(), "P3M");
        assert_eq!(Duration::parse("PT36H").unwrap().to_string(), "PT36H");
        assert_eq!(
            Duration::parse("P1Y2M3DT4H5M6S").unwrap().phrase(),
            "1 year 2 months 3 days 4 hours 5 minutes 6 seconds "
        );
        assert!(Duration::parse("P").unwrap().is_zero());
        for bad in [
            "",
            "3M",
            "PT",
            "P1M1Y",
            "P1H",
            "PT1D",
            "P-1D",
            "P1.5Y",
            "P1Y1Y",
            "P1234567890D",
        ] {
            assert_eq!(Duration::parse(bad), None, "{bad}");
        }
    }
}
