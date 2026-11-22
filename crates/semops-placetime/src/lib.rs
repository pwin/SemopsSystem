//! Placetime: URIs for calendar intervals, instants and points, each described in RDF from
//! the URI alone, with no store behind it.
//!
//! The interval and instant URIs keep the patterns reference.data.gov.uk published from 2010
//! (IntervalServer, by Epimorphics), so data that already cites them keeps its meaning under
//! a new base:
//!
//! | URI, after the base | What it names |
//! | --- | --- |
//! | `id/year/2006`, `id/quarter/2006-Q1`, `id/day/1752-09-14` … | British calendar intervals: Julian until 2 September 1752, Gregorian from 14 September |
//! | `id/gregorian-year/2006` … `id/gregorian-second/2006-01-01T00:00:00` | Gregorian calendar intervals |
//! | `id/julian-year/1582` … | Julian calendar intervals (an addition) |
//! | `id/gregorian-instant/2006-01-01T00:00:00` | An instant |
//! | `id/gregorian-interval/2006-01-01T00:00:00/P3M` | An interval of any duration |
//! | `id/geopoint/wgs84/X-0.1276Y51.5072` | A point, longitude then latitude (an addition, after placetime.com) |
//! | `id/year`, `id/gregorian-calendar`, `id/interval`, `id/geopoint/wgs84` … | The URI sets themselves |
//!
//! An `id/` URI names the thing; its description is the document at the same path under
//! `doc/`, optionally with a format extension (`.ttl`, `.nt`, `.rdf`, `.jsonld`, and `.n3`
//! and `.json` for old links). [`resolve`] answers a request path with the redirect or the
//! document HTTP should give; the crate does no I/O and knows nothing of HTTP beyond that.
//!
//! The base is the service's choice: `https://{host}/` by default ([`Base::for_host`]), or
//! any configured URL ([`Base::new`]). The vocabularies keep their own namespaces, which are
//! names rather than addresses: `http://reference.data.gov.uk/def/intervals/` and the rest.

pub mod calendar;
mod describe;
pub mod path;
pub mod place;
pub mod vocab;

use oxrdf::{Graph, NamedNode};
use std::fmt;

pub use calendar::{Calendar, Duration, Moment};
pub use path::{Aligned, Set, Thing, Unit};
pub use place::Point;
pub use vocab::PREFIXES;

/// The URI everything here is named under, ending in `/`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Base {
    iri: String,
    /// Where the path starts: the byte after `scheme://authority`.
    path: usize,
}

/// Why a base or a host was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseError(String);

impl fmt::Display for BaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for BaseError {}

impl Base {
    /// A configured base: an absolute `https://` or `http://` IRI with no query or fragment.
    /// A missing final `/` is added, so `https://example.org/ref` names
    /// `https://example.org/ref/id/year/2006`.
    ///
    /// # Errors
    ///
    /// If the base is not such an IRI, or its host is not a host name or address.
    pub fn new(iri: &str) -> Result<Self, BaseError> {
        let refuse = |why: &str| BaseError(format!("`{iri}` cannot be a placetime base: {why}"));
        let (scheme, rest) = iri
            .split_once("://")
            .ok_or_else(|| refuse("it is not an absolute URL"))?;
        let scheme = scheme.to_ascii_lowercase();
        if scheme != "https" && scheme != "http" {
            return Err(refuse("only https and http bases are served"));
        }
        if rest.contains(['?', '#']) {
            return Err(refuse("a base has no query or fragment"));
        }
        let (authority, path) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, "/"),
        };
        let host = host(authority).ok_or_else(|| refuse("its host is not a host name"))?;
        let mut out = format!("{scheme}://{host}{path}");
        if !out.ends_with('/') {
            out.push('/');
        }
        NamedNode::new(out.as_str()).map_err(|e| refuse(&e.to_string()))?;
        Ok(Self {
            path: scheme.len() + 3 + host.len(),
            iri: out,
        })
    }

    /// `https://{host}/`, for a host taken from a request.
    ///
    /// # Errors
    ///
    /// If `host` is not a host name or address with an optional port. A request's `Host`
    /// header is the client's to write, and it goes into every URI in the answer.
    pub fn for_host(host_header: &str) -> Result<Self, BaseError> {
        let host = host(host_header.trim())
            .ok_or_else(|| BaseError(format!("`{host_header}` is not a host name")))?;
        Ok(Self {
            path: "https://".len() + host.len(),
            iri: format!("https://{host}/"),
        })
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.iri
    }

    /// The base's path, starting and ending with `/`: where in a server's URL space the
    /// URIs are answered.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.iri[self.path..]
    }
}

impl fmt::Display for Base {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.iri)
    }
}

/// A host name, IPv4 or bracketed IPv6 address, and optional port, lower-cased; `None` for
/// anything else.
fn host(authority: &str) -> Option<String> {
    let (name, port) = if let Some(rest) = authority.strip_prefix('[') {
        let (address, after) = rest.split_once(']')?;
        let address_ok = !address.is_empty()
            && address
                .bytes()
                .all(|b| b.is_ascii_hexdigit() || b == b':' || b == b'.');
        if !address_ok {
            return None;
        }
        let port = match after {
            "" => None,
            _ => Some(after.strip_prefix(':')?),
        };
        (format!("[{address}]"), port)
    } else {
        let (name, port) = match authority.split_once(':') {
            Some((name, port)) => (name, Some(port)),
            None => (authority, None),
        };
        let name = name.strip_suffix('.').unwrap_or(name);
        let label_ok = |l: &str| {
            !l.is_empty()
                && l.len() <= 63
                && l.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
                && !l.starts_with('-')
                && !l.ends_with('-')
        };
        if name.is_empty() || name.len() > 253 || !name.split('.').all(label_ok) {
            return None;
        }
        (name.to_owned(), port)
    };
    let mut out = name.to_ascii_lowercase();
    if let Some(port) = port {
        if port.is_empty() || !port.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let number: u16 = port.parse().ok()?;
        out.push_str(&format!(":{number}"));
    }
    Some(out)
}

/// A serialisation a document is offered in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Format {
    Turtle,
    NTriples,
    /// Served as Turtle, which is N3, as IntervalServer served it.
    N3,
    RdfXml,
    JsonLd,
}

impl Format {
    /// The formats a document says it is offered in.
    pub const OFFERED: [Self; 4] = [Self::Turtle, Self::NTriples, Self::RdfXml, Self::JsonLd];

    /// The format a document extension asks for: the four offered, and `n3` and `json`,
    /// which IntervalServer's links used.
    #[must_use]
    pub fn from_extension(extension: &str) -> Option<Self> {
        match extension {
            "ttl" => Some(Self::Turtle),
            "nt" => Some(Self::NTriples),
            "n3" => Some(Self::N3),
            "rdf" => Some(Self::RdfXml),
            "jsonld" | "json" => Some(Self::JsonLd),
            _ => None,
        }
    }

    #[must_use]
    pub fn extension(self) -> &'static str {
        match self {
            Self::Turtle => "ttl",
            Self::NTriples => "nt",
            Self::N3 => "n3",
            Self::RdfXml => "rdf",
            Self::JsonLd => "jsonld",
        }
    }

    #[must_use]
    pub fn media_type(self) -> &'static str {
        match self {
            Self::Turtle => "text/turtle",
            Self::NTriples => "application/n-triples",
            Self::N3 => "text/n3",
            Self::RdfXml => "application/rdf+xml",
            Self::JsonLd => "application/ld+json",
        }
    }

    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Turtle => "Turtle",
            Self::NTriples => "N-Triples",
            Self::N3 => "N3",
            Self::RdfXml => "RDF/XML",
            Self::JsonLd => "JSON-LD",
        }
    }
}

/// A document, ready to serialise.
#[derive(Debug)]
pub struct Document {
    /// The document as requested, extension included: what `Content-Location` should say
    /// when a format was asked for in the path.
    pub location: String,
    /// The document without an extension, to which a negotiated format's extension is added.
    pub generic: String,
    /// The format the path asked for, or `None` to negotiate one.
    pub format: Option<Format>,
    /// The thing the document is about.
    pub topic: NamedNode,
    pub graph: Graph,
}

/// What to answer a request with.
#[derive(Debug)]
pub enum Outcome {
    /// An `id/` URI: 303 See Other to its document.
    SeeOther(String),
    /// A thing asked for in a spelling other than its canonical one: 301 to that.
    Moved(String),
    Document(Box<Document>),
    NotFound,
}

/// Whether a path below the base is one this crate answers: anything under `id/` or `doc/`.
#[must_use]
pub fn claims(relative: &str) -> bool {
    relative.starts_with("id/") || relative.starts_with("doc/")
}

/// Answers a request for `relative`, the request path with the base's path taken off:
/// `id/quarter/2006-Q1`, `doc/quarter/2006-Q1.ttl`. Percent-encoded characters are decoded
/// first, so `%3A` serves as well as `:`.
#[must_use]
pub fn resolve(base: &Base, relative: &str) -> Outcome {
    let Some(relative) = decode(relative) else {
        return Outcome::NotFound;
    };
    let (document, rest) = if let Some(rest) = relative.strip_prefix("id/") {
        (false, rest)
    } else if let Some(rest) = relative.strip_prefix("doc/") {
        (true, rest)
    } else {
        return Outcome::NotFound;
    };
    let (path, extension) = match rest.rsplit_once('.') {
        Some((path, ext)) if Format::from_extension(ext).is_some() => (path, Some(ext)),
        _ => (rest, None),
    };
    let Some(thing) = Thing::parse(path) else {
        return Outcome::NotFound;
    };
    let canonical = thing.path();
    let suffix = extension.map(|e| format!(".{e}")).unwrap_or_default();
    if canonical != path {
        let stem = if document { "doc" } else { "id" };
        return Outcome::Moved(format!("{base}{stem}/{canonical}{suffix}"));
    }
    if !document {
        return Outcome::SeeOther(format!("{base}doc/{canonical}{suffix}"));
    }
    let mut builder = describe::Builder::new(base.as_str());
    let (topic, label) = builder.thing(&thing);
    builder.document(&canonical, &topic, &label);
    Outcome::Document(Box::new(Document {
        location: format!("{base}doc/{canonical}{suffix}"),
        generic: format!("{base}doc/{canonical}"),
        format: extension.and_then(Format::from_extension),
        topic,
        graph: builder.graph,
    }))
}

/// The description of whatever `path` names (`quarter/2006-Q1`, with no `id/`), or `None`.
/// The graph a document at that path serves, without the statements about the document.
#[must_use]
pub fn describe(base: &Base, path: &str) -> Option<Graph> {
    let thing = Thing::parse(path)?;
    let mut builder = describe::Builder::new(base.as_str());
    builder.thing(&thing);
    Some(builder.graph)
}

fn decode(text: &str) -> Option<String> {
    if !text.contains('%') {
        return Some(text.to_owned());
    }
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = text.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}
