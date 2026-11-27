//! Reject ambiguous cache input documents before deserializing their declared types.
#![allow(unexpected_cfgs)]
#[cfg(not(verus_keep_ghost))]
use vstd::prelude::verus;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

verus! {
/// Adopt a duplicate-key diagnostic only when the parser supplied a quoted key that decodes.
pub fn duplicate_diagnostic<K, D, L, E, P, F>(fields: Option<(K,L)>, parse: P, format: F) -> (out: Option<E>)
    where P: FnOnce(K) -> Option<D>, F: FnOnce(D,L) -> E,
    requires forall|key: K| parse.requires((key,)), forall|key: D, loc: L| format.requires((key,loc)),
    ensures match fields {
        None => out.is_none(),
        Some((quoted, loc)) => exists|key: Option<D>| #[trigger] parse.ensures((quoted,),key) &&
            match key { None => out.is_none(), Some(value) =>
                match out { Some(e) => format.ensures((value,loc),e), None => false } },
    },
{
    let (quoted, loc) = match fields { None => return None, Some(pair) => pair };
    let key = parse(quoted);
    assert(parse.ensures((quoted,),key));
    match key { None => None, Some(value) => Some(format(value,loc)) }
}

/// The strict parse precedes conversion to the requested document type. Preserve the first error.
pub fn decode_document<P, J, V, E, C, D>(
    parsed: Result<P, E>, convert: C, decode: D,
) -> (out: Result<V, E>)
    where C: FnOnce(P) -> Result<J, E>, D: FnOnce(J) -> Result<V, E>,
    requires forall|p: P| convert.requires((p,)), forall|j: J| decode.requires((j,)),
    ensures match parsed {
        Err(error) => out == Err::<V, E>(error),
        Ok(p) => exists|json: Result<J,E>| #[trigger] convert.ensures((p,), json) &&
            match json { Err(error) => out == Err::<V,E>(error), Ok(value) => decode.ensures((value,), out) },
    },
{
    let value = match parsed { Err(error) => return Err(error), Ok(p) => p };
    let converted = convert(value);
    assert(convert.ensures((value,), converted));
    match converted { Err(error) => Err(error), Ok(json) => decode(json) }
}
}

#[cfg(not(verus_keep_ghost))]
fn strict_json(bytes: &[u8]) -> Result<serde_yaml_ng::Value, String> {
    serde_json::from_slice(bytes).map_err(|error: serde_json::Error| {
        let message = error.to_string();
        let fields = message
            .strip_prefix("duplicate entry with key ")
            .and_then(|tail| tail.rsplit_once(" at line "));
        duplicate_diagnostic(
            fields,
            |quoted| litrs::StringLit::parse(quoted).ok(),
            |key, location| format!("duplicate JSON member {} at line {location}", key.value()),
        )
        .unwrap_or(message)
    })
}

#[cfg(not(verus_keep_ghost))]
pub fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    decode_document(
        strict_json(bytes),
        |value| serde_json::to_value(value).map_err(|error| error.to_string()),
        |json| serde_json::from_value(json).map_err(|error| error.to_string()),
    )
}
