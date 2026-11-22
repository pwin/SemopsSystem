//! Identity from Azure's built-in authentication ("Easy Auth").
//!
//! Azure Container Apps and App Service can authenticate requests themselves, against
//! Entra ID or another provider, before they reach the container. A request that gets
//! through carries the verified identity in `X-MS-CLIENT-PRINCIPAL`: base64 of a JSON
//! document listing the token's claims, with `role_typ` naming the claim that holds app
//! roles. This module turns that into a [`Principal`]: the object id as its subject, the
//! app roles and the `groups` claim as its roles, every other claim as an attribute.
//!
//! It verifies nothing, because the platform has. That is only true for a request that
//! came through the platform's authentication, so the server reads the header only with
//! `--trust-azure-easy-auth`, which belongs on a deployment where authentication is
//! required and nothing reaches the container except through it. See
//! `deploy/azure/README.md`.

use holos_security::Principal;
use std::collections::{BTreeMap, BTreeSet};

/// The claim types Entra ID uses for the object id, and the role claim's usual names.
const OBJECT_ID: [&str; 2] = [
    "http://schemas.microsoft.com/identity/claims/objectidentifier",
    "oid",
];
const ROLE_CLAIMS: [&str; 2] = [
    "roles",
    "http://schemas.microsoft.com/ws/2008/06/identity/claims/role",
];

/// Decodes standard base64 (with or without padding); `None` for anything else.
fn base64(text: &str) -> Option<Vec<u8>> {
    fn value(c: u8) -> Option<u32> {
        Some(u32::from(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        }))
    }
    let bytes: Vec<u8> = text.trim().bytes().filter(|b| *b != b'=').collect();
    if bytes.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        let mut acc = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            acc |= value(c)? << (18 - 6 * i);
        }
        let [_, a, b, c] = acc.to_be_bytes();
        out.push(a);
        if chunk.len() > 2 {
            out.push(b);
        }
        if chunk.len() > 3 {
            out.push(c);
        }
    }
    Some(out)
}

/// The principal an `X-MS-CLIENT-PRINCIPAL` value describes.
///
/// `principal_id` is `X-MS-CLIENT-PRINCIPAL-ID`, used as the subject when the claims carry
/// no object id.
///
/// # Errors
///
/// If the value is not base64 of the JSON document the platform writes, or names no
/// subject at all. The caller treats that as anonymous: a header it cannot read grants
/// nothing.
pub fn principal(header: &str, principal_id: Option<&str>) -> Result<Principal, String> {
    let bytes = base64(header).ok_or("X-MS-CLIENT-PRINCIPAL is not base64")?;
    let document: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| format!("X-MS-CLIENT-PRINCIPAL: {e}"))?;
    let role_typ = document["role_typ"].as_str().unwrap_or(ROLE_CLAIMS[0]);
    let issuer = document["auth_typ"].as_str().unwrap_or("easyauth");
    let mut claims: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for claim in document["claims"]
        .as_array()
        .ok_or("X-MS-CLIENT-PRINCIPAL has no claims")?
    {
        let (Some(typ), Some(val)) = (claim["typ"].as_str(), claim["val"].as_str()) else {
            continue;
        };
        // App roles and group membership both become roles; the groups also stay an
        // attribute, so a rule can tell them apart.
        let is_role = typ == role_typ || ROLE_CLAIMS.contains(&typ);
        let key = if is_role { "roles" } else { typ };
        claims
            .entry(key.to_owned())
            .or_default()
            .insert(val.to_owned());
        if typ == "groups" {
            claims
                .entry("roles".to_owned())
                .or_default()
                .insert(val.to_owned());
        }
    }
    let subject = OBJECT_ID
        .iter()
        .find_map(|k| claims.get(*k).and_then(|v| v.iter().next()).cloned())
        .or_else(|| principal_id.map(str::to_owned))
        .filter(|s| !s.is_empty())
        .ok_or("X-MS-CLIENT-PRINCIPAL names no subject")?;
    Ok(Principal::from_verified_claims(
        issuer, &subject, &claims, "roles",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(text: &str) -> String {
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let bytes = text.as_bytes();
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let n = chunk
                .iter()
                .enumerate()
                .fold(0u32, |acc, (i, &b)| acc | u32::from(b) << (16 - 8 * i));
            for i in 0..=chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            }
        }
        while !out.len().is_multiple_of(4) {
            out.push('=');
        }
        out
    }

    const DOCUMENT: &str = r#"{
      "auth_typ": "aad",
      "name_typ": "http://schemas.xmlsoap.org/ws/2005/05/identity/claims/emailaddress",
      "role_typ": "http://schemas.microsoft.com/ws/2008/06/identity/claims/role",
      "claims": [
        {"typ": "http://schemas.microsoft.com/identity/claims/objectidentifier", "val": "6f1b2c3d-0000-4000-8000-000000000001"},
        {"typ": "http://schemas.microsoft.com/ws/2008/06/identity/claims/role", "val": "Holos.Read"},
        {"typ": "http://schemas.microsoft.com/ws/2008/06/identity/claims/role", "val": "hr"},
        {"typ": "groups", "val": "a1b2c3d4-0000-4000-8000-0000000000aa"},
        {"typ": "name", "val": "Alice Example"}
      ]
    }"#;

    #[test]
    fn roles_and_groups_become_roles_and_the_rest_attributes() {
        let p = principal(&encode(DOCUMENT), None).expect("parses");
        assert!(
            p.id.as_str()
                .contains("6f1b2c3d-0000-4000-8000-000000000001"),
            "{}",
            p.id
        );
        assert!(p.id.as_str().starts_with("urn:holos:principal:aad/"));
        for role in ["Holos.Read", "hr", "a1b2c3d4-0000-4000-8000-0000000000aa"] {
            assert!(p.roles.contains(role), "{role} missing from {:?}", p.roles);
        }
        assert_eq!(p.attributes.get("name").map(|v| v.len()), Some(1));
    }

    #[test]
    fn the_principal_id_header_is_the_fallback_subject() {
        let doc = r#"{"auth_typ":"aad","claims":[{"typ":"roles","val":"reader"}]}"#;
        let p = principal(&encode(doc), Some("00000000-0000-4000-8000-0000000000bb")).unwrap();
        assert!(p
            .id
            .as_str()
            .ends_with("00000000-0000-4000-8000-0000000000bb"));
        assert!(p.roles.contains("reader"));
        assert!(
            principal(&encode(doc), None).is_err(),
            "no subject at all is refused"
        );
    }

    #[test]
    fn a_header_that_is_not_the_platforms_grants_nothing() {
        for bad in [
            "not base64!",
            &encode("not json"),
            &encode(r#"{"claims": 3}"#),
        ] {
            assert!(principal(bad, Some("x")).is_err(), "{bad}");
        }
    }

    #[test]
    fn base64_round_trips() {
        for text in ["", "a", "ab", "abc", "abcd", "hello, world"] {
            assert_eq!(base64(&encode(text)).as_deref(), Some(text.as_bytes()));
        }
    }
}
