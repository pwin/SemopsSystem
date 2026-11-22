//! Points on the WGS84 ellipsoid, named as placetime.com named them.
//!
//! `geopoint/wgs84/X{longitude}Y{latitude}` in decimal degrees: `X0Y0` is where the equator
//! crosses the prime meridian, `X-0.1276Y51.5072` is London. A point is described with
//! GeoSPARQL, so a store that loads the description can query it spatially, and with the W3C
//! Basic Geo vocabulary, which older linked data uses.

/// Digits a coordinate may keep after the point: 10 is a ten-thousandth of a millimetre at
/// the equator, which is finer than any survey, and a bound keeps URIs from growing without
/// end.
const MAX_FRACTION: usize = 10;

/// A coordinate in decimal degrees, held as its canonical decimal spelling: no `+`, no
/// leading zeros, no trailing zeros after the point, no `-0`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Coordinate(String);

impl Coordinate {
    /// Parses `-?[0-9]+(\.[0-9]+)?` within `-limit..=limit` degrees.
    fn parse(text: &str, limit: u32) -> Option<Self> {
        if text.len() > 32 {
            return None;
        }
        let (negative, magnitude) = match text.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, text),
        };
        let (whole, fraction) = match magnitude.split_once('.') {
            Some((w, f)) if !f.is_empty() => (w, f),
            Some(_) => return None,
            None => (magnitude, ""),
        };
        if whole.is_empty()
            || !whole.bytes().all(|b| b.is_ascii_digit())
            || !fraction.bytes().all(|b| b.is_ascii_digit())
        {
            return None;
        }
        let whole = whole.trim_start_matches('0');
        let fraction = fraction.trim_end_matches('0');
        if whole.len() > 3 || fraction.len() > MAX_FRACTION {
            return None;
        }
        let degrees: u32 = if whole.is_empty() {
            0
        } else {
            whole.parse().ok()?
        };
        if degrees > limit || (degrees == limit && !fraction.is_empty()) {
            return None;
        }
        let zero = degrees == 0 && fraction.is_empty();
        let mut out = String::new();
        if negative && !zero {
            out.push('-');
        }
        out.push_str(&degrees.to_string());
        if !fraction.is_empty() {
            out.push('.');
            out.push_str(fraction);
        }
        Some(Self(out))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn is_negative(&self) -> bool {
        self.0.starts_with('-')
    }

    fn magnitude(&self) -> &str {
        self.0.trim_start_matches('-')
    }
}

/// A point, by latitude and longitude in WGS84.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Point {
    pub latitude: Coordinate,
    pub longitude: Coordinate,
}

impl Point {
    /// Parses `X{longitude}Y{latitude}`.
    #[must_use]
    pub fn parse(reference: &str) -> Option<Self> {
        let rest = reference.strip_prefix('X')?;
        let (longitude, latitude) = rest.split_once('Y')?;
        Some(Self {
            latitude: Coordinate::parse(latitude, 90)?,
            longitude: Coordinate::parse(longitude, 180)?,
        })
    }

    /// The canonical `X{longitude}Y{latitude}`.
    #[must_use]
    pub fn reference(&self) -> String {
        format!("X{}Y{}", self.longitude.as_str(), self.latitude.as_str())
    }

    /// The point as Well-Known Text, longitude first: GeoSPARQL reads a WKT literal with no
    /// reference system as OGC CRS84, which is WGS84 in that axis order.
    #[must_use]
    pub fn wkt(&self) -> String {
        format!(
            "POINT({} {})",
            self.longitude.as_str(),
            self.latitude.as_str()
        )
    }

    /// The RFC 5870 `geo:` URI, latitude first.
    #[must_use]
    pub fn geo_uri(&self) -> String {
        format!("geo:{},{}", self.latitude.as_str(), self.longitude.as_str())
    }

    /// "WGS84 Point: 51.5072° N, 0.1276° W".
    #[must_use]
    pub fn label(&self) -> String {
        format!(
            "WGS84 Point: {}° {}, {}° {}",
            self.latitude.magnitude(),
            if self.latitude.is_negative() {
                'S'
            } else {
                'N'
            },
            self.longitude.magnitude(),
            if self.longitude.is_negative() {
                'W'
            } else {
                'E'
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn points_parse_to_a_canonical_spelling() {
        let p = Point::parse("X-000.12760Y051.50720").unwrap();
        assert_eq!(p.reference(), "X-0.1276Y51.5072");
        assert_eq!(p.wkt(), "POINT(-0.1276 51.5072)");
        assert_eq!(p.geo_uri(), "geo:51.5072,-0.1276");
        assert_eq!(p.label(), "WGS84 Point: 51.5072° N, 0.1276° W");
        assert_eq!(Point::parse("X-0Y-0.0").unwrap().reference(), "X0Y0");
        assert_eq!(Point::parse("X180Y-90").unwrap().reference(), "X180Y-90");
    }

    #[test]
    fn points_off_the_globe_or_oddly_spelled_are_refused() {
        for bad in [
            "X0Y90.1",
            "X180.5Y0",
            "X-181Y0",
            "Y0X0",
            "X0",
            "X+1Y0",
            "X1.Y0",
            "X.5Y0",
            "X1e2Y0",
            "XY0",
            "X0Y",
            "X0.12345678901Y0",
            "X 1Y0",
        ] {
            assert_eq!(Point::parse(bad), None, "{bad}");
        }
        // Trailing zeros beyond the limit are not precision, and are dropped.
        assert!(Point::parse("X0.123456789000000Y0").is_some());
    }
}
