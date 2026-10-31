//! Rewrites every version pair needs, independent of what changed between the versions.

use ferrite_protocol::packets::configuration::KnownPack;
use ferrite_protocol::packets::handshake::Intention;
use ferrite_protocol::{Decode, Encode};

use crate::TranslateError;

/// Handshake from the older client: claim the newer protocol towards the next step.
pub fn rewrite_intention(body: &[u8], older: i32, newer: i32) -> Result<Vec<u8>, TranslateError> {
    let mut buf = body;
    let mut intention = Intention::decode(&mut buf)?;
    if intention.protocol_version == older {
        intention.protocol_version = newer;
    }
    let mut out = Vec::with_capacity(body.len());
    intention.encode(&mut out);
    out.extend_from_slice(buf);
    Ok(out)
}

/// Status response towards the older client: advertise the client's own protocol so the
/// server list shows the server as compatible. The version name is left as the core's.
pub fn rewrite_status(body: &[u8], newer: i32, older: i32) -> Result<Vec<u8>, TranslateError> {
    let mut buf = body;
    let json = String::decode(&mut buf)?;
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&json) else {
        return Ok(body.to_vec());
    };
    if let Some(version) = value.get_mut("version").and_then(|v| v.as_object_mut())
        && version.get("protocol").and_then(|p| p.as_i64()) == Some(i64::from(newer))
    {
        version.insert("protocol".into(), older.into());
    }
    let mut out = Vec::new();
    value.to_string().encode(&mut out);
    out.extend_from_slice(buf);
    Ok(out)
}

/// Rewrite the version string of the vanilla `minecraft:core` known pack (both directions
/// of `select_known_packs`).
pub fn rewrite_known_packs(body: &[u8], from: &str, to: &str) -> Result<Vec<u8>, TranslateError> {
    let mut buf = body;
    let mut packs = Vec::<KnownPack>::decode(&mut buf)?;
    for pack in &mut packs {
        if pack.namespace == "minecraft" && pack.id == "core" && pack.version == from {
            pack.version = to.to_owned();
        }
    }
    let mut out = Vec::with_capacity(body.len());
    packs.encode(&mut out);
    out.extend_from_slice(buf);
    Ok(out)
}
