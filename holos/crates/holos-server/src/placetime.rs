//! The placetime URI sets, served beside the SPARQL endpoint.
//!
//! `semops-placetime` decides what a path names and describes it; this module turns that into
//! an HTTP answer: 303 from `/id/…` to `/doc/…`, 301 to a canonical spelling, a document in
//! the format the path or the `Accept` header asks for, or 404. It takes the request's
//! headers as plain values and returns the answer as plain values, so it is tested without a
//! socket.
//!
//! The base the URIs are minted under is configured (`--placetime-base`) or taken from the
//! request (`--placetime`): `https://{Host}/`. The scheme is always `https` in the second case,
//! because a server behind a TLS front door sees plain HTTP and would otherwise mint `http://`
//! names for things published as `https://`. `X-Forwarded-Host` is read only when the
//! operator trusts forwarded headers at all (`--trust-forwarded-identity`), since a client
//! that can name the host can name every URI in the answer.

use oxrdfio::{RdfFormat, RdfSerializer};
use semops_placetime::{Base, Format, Outcome, Vocabulary};

/// Where the base comes from.
#[derive(Clone, Debug)]
pub enum Mode {
    /// `https://{Host}/`, from each request.
    Host,
    /// A configured base, the same for every request.
    Fixed(Base),
}

impl Mode {
    /// The part of the server's URL space the URIs are answered in.
    #[must_use]
    pub fn path(&self) -> &str {
        match self {
            Self::Host => "/",
            Self::Fixed(base) => base.path(),
        }
    }

    /// The request path below the base, if it is one placetime answers.
    #[must_use]
    pub fn relative<'a>(&self, path: &'a str) -> Option<&'a str> {
        let rest = path.strip_prefix(self.path())?;
        semops_placetime::claims(rest).then_some(rest)
    }
}

/// An HTTP answer, ready to send.
#[derive(Debug)]
pub struct Answer {
    pub status: u16,
    pub content_type: String,
    pub headers: Vec<(&'static str, String)>,
    pub body: Vec<u8>,
}

impl Answer {
    fn text(status: u16, body: &str) -> Self {
        Self {
            status,
            content_type: "text/plain; charset=utf-8".to_owned(),
            headers: Vec::new(),
            body: body.as_bytes().to_vec(),
        }
    }

    fn redirect(status: u16, location: String) -> Self {
        Self {
            status,
            content_type: "text/plain; charset=utf-8".to_owned(),
            body: format!("{location}\n").into_bytes(),
            headers: vec![
                ("Location", location),
                // The answer is a function of the URI and the base, nothing else.
                ("Cache-Control", "public, max-age=86400".to_owned()),
            ],
        }
    }
}

/// Answers a request for `relative` (from [`Mode::relative`]), with documents in
/// `vocabulary`.
///
/// `host` is the `Host` header; `forwarded_host` is `X-Forwarded-Host`, which the caller
/// passes only if forwarded headers are trusted.
#[must_use]
pub fn answer(
    mode: &Mode,
    vocabulary: Vocabulary,
    relative: &str,
    host: Option<&str>,
    forwarded_host: Option<&str>,
    accept: Option<&str>,
) -> Answer {
    let base = match mode {
        Mode::Fixed(base) => base.clone(),
        Mode::Host => {
            // A proxy may append itself: the first name is the one the client used.
            let named = forwarded_host
                .and_then(|h| h.split(',').next())
                .map(str::trim)
                .filter(|h| !h.is_empty())
                .or(host);
            let Some(named) = named else {
                return Answer::text(
                    400,
                    "placetime names its URIs after the Host header, and this request has none\n",
                );
            };
            match Base::for_host(named) {
                Ok(base) => base,
                Err(e) => return Answer::text(400, &format!("{e}\n")),
            }
        }
    };
    match semops_placetime::resolve_with(&base, relative, vocabulary) {
        Outcome::SeeOther(location) => Answer::redirect(303, location),
        Outcome::Moved(location) => Answer::redirect(301, location),
        Outcome::NotFound => Answer::text(404, "not found\n"),
        Outcome::Document(doc) => {
            let doc = *doc;
            let (format, location, negotiated) = match doc.format {
                Some(format) => (format, doc.location, false),
                None => {
                    let format = negotiate(accept);
                    (
                        format,
                        format!("{}.{}", doc.generic, format.extension()),
                        true,
                    )
                }
            };
            let body = match serialise(&doc.graph, &doc.topic, format, vocabulary) {
                Ok(body) => body,
                Err(e) => return Answer::text(500, &format!("serialising the document: {e}\n")),
            };
            let mut headers = vec![
                ("Content-Location", location),
                ("Cache-Control", "public, max-age=86400".to_owned()),
            ];
            if negotiated {
                headers.push(("Vary", "Accept".to_owned()));
            }
            Answer {
                status: 200,
                content_type: format!("{}; charset=utf-8", format.media_type()),
                headers,
                body,
            }
        }
    }
}

/// The format an `Accept` header prefers among those a document is offered in. Turtle when
/// nothing is said, or nothing offered is acceptable: a document is better than a 406 to a
/// browser that sent `text/html`.
#[must_use]
pub fn negotiate(accept: Option<&str>) -> Format {
    let Some(accept) = accept else {
        return Format::Turtle;
    };
    let mut best = (Format::Turtle, 0.0_f32);
    for part in accept.split(',') {
        let mut fields = part.split(';');
        let media = fields.next().unwrap_or("").trim().to_ascii_lowercase();
        let q = fields
            .filter_map(|p| p.trim().strip_prefix("q="))
            .find_map(|v| v.trim().parse::<f32>().ok())
            .unwrap_or(1.0);
        let format = match media.as_str() {
            "text/turtle" | "application/x-turtle" | "*/*" | "text/*" => Format::Turtle,
            "application/n-triples" => Format::NTriples,
            "text/n3" => Format::N3,
            "application/rdf+xml" => Format::RdfXml,
            "application/ld+json" | "application/json" => Format::JsonLd,
            _ => continue,
        };
        if q > best.1 {
            best = (format, q);
        }
    }
    best.0
}

/// Serialises a document with the thing it is about first, then everything else grouped by
/// subject, so the Turtle a person opens starts with what they asked for.
fn serialise(
    graph: &oxrdf::Graph,
    topic: &oxrdf::NamedNode,
    format: Format,
    vocabulary: Vocabulary,
) -> Result<Vec<u8>, std::io::Error> {
    let rdf_format = match format {
        Format::Turtle | Format::N3 => RdfFormat::Turtle,
        Format::NTriples => RdfFormat::NTriples,
        Format::RdfXml => RdfFormat::RdfXml,
        Format::JsonLd => RdfFormat::JsonLd {
            profile: oxrdfio::JsonLdProfileSet::empty(),
        },
    };
    let mut serializer = RdfSerializer::from_format(rdf_format);
    for (name, iri) in vocabulary.prefixes() {
        serializer = serializer
            .with_prefix(*name, *iri)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
    }
    let mut triples: Vec<oxrdf::TripleRef<'_>> = graph.iter().collect();
    triples.sort_by_cached_key(|t| {
        let blank = matches!(t.subject, oxrdf::NamedOrBlankNodeRef::BlankNode(_));
        (
            t.subject != topic.as_ref().into(),
            blank,
            t.subject.to_string(),
            t.predicate != oxrdf::vocab::rdf::TYPE,
            t.predicate.to_string(),
            t.object.to_string(),
        )
    });
    let mut writer = serializer.for_writer(Vec::new());
    for triple in triples {
        writer.serialize_triple(triple)?;
    }
    writer.finish()
}
