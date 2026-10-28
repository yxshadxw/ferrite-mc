//! Resource ids with the `minecraft:` namespace filled in when omitted.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A namespaced resource id such as `minecraft:overworld/continents`.
///
/// Parsing defaults a missing namespace to `minecraft:` so both spellings used in the data
/// and in code resolve to the same key.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ResourceId {
    namespace: String,
    path: String,
}

impl ResourceId {
    /// Parses `namespace:path` or `path` (which implies `minecraft:`).
    pub fn parse(value: &str) -> Result<Self, InvalidResourceId> {
        let value = value.trim();
        if value.starts_with('#') {
            return Err(InvalidResourceId(value.to_owned()));
        }
        let (namespace, path) = value.split_once(':').unwrap_or(("minecraft", value));
        if namespace.is_empty() || path.is_empty() {
            return Err(InvalidResourceId(value.to_owned()));
        }
        Ok(Self {
            namespace: namespace.to_owned(),
            path: path.to_owned(),
        })
    }

    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    pub fn path(&self) -> &str {
        &self.path
    }
}

impl fmt::Display for ResourceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.namespace, self.path)
    }
}

impl FromStr for ResourceId {
    type Err = InvalidResourceId;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl Serialize for ResourceId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for ResourceId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

/// Error returned when a resource id is empty or starts with `#` (a tag, not an id).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvalidResourceId(pub String);

impl fmt::Display for InvalidResourceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid resource id `{}`", self.0)
    }
}

impl std::error::Error for InvalidResourceId {}

#[cfg(test)]
mod tests {
    use super::ResourceId;

    #[test]
    fn fills_in_the_default_namespace() {
        let id = ResourceId::parse("overworld/continents").unwrap();
        assert_eq!(id.namespace(), "minecraft");
        assert_eq!(id.path(), "overworld/continents");
        assert_eq!(id.to_string(), "minecraft:overworld/continents");
        assert_eq!(
            id,
            ResourceId::parse("minecraft:overworld/continents").unwrap()
        );
    }

    #[test]
    fn rejects_empty_and_tag_ids() {
        assert!(ResourceId::parse("").is_err());
        assert!(ResourceId::parse("minecraft:").is_err());
        assert!(ResourceId::parse(":stone").is_err());
        assert!(ResourceId::parse("#minecraft:logs").is_err());
    }

    #[test]
    fn serde_round_trip() {
        let id = ResourceId::parse("ferrite:test/value").unwrap();
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"ferrite:test/value\"");
        let back: ResourceId = serde_json::from_str(&json).unwrap();
        assert_eq!(back, id);
    }
}
