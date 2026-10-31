//! Entity targets: player names, UUIDs and `@x[...]` selectors.
//!
//! Selector options are parsed for shape only (known key, `=`, a raw value); evaluating
//! them against a world is the game's job.

use crate::error::{CommandSyntaxError, minecraft};
use crate::reader::{ARGUMENT_SEPARATOR, StringReader};

use super::scan::read_balanced;

/// The `@` selector variable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SelectorKind {
    /// `@p`: nearest player.
    NearestPlayer,
    /// `@a`: all players.
    AllPlayers,
    /// `@r`: random player.
    RandomPlayer,
    /// `@s`: the executing entity.
    Myself,
    /// `@e`: all entities.
    AllEntities,
    /// `@n`: nearest entity.
    NearestEntity,
}

impl SelectorKind {
    pub const ALL: [SelectorKind; 6] = [
        Self::AllPlayers,
        Self::AllEntities,
        Self::NearestEntity,
        Self::NearestPlayer,
        Self::RandomPlayer,
        Self::Myself,
    ];

    pub fn from_char(c: char) -> Option<Self> {
        Some(match c {
            'p' => Self::NearestPlayer,
            'a' => Self::AllPlayers,
            'r' => Self::RandomPlayer,
            's' => Self::Myself,
            'e' => Self::AllEntities,
            'n' => Self::NearestEntity,
            _ => return None,
        })
    }

    pub fn char(self) -> char {
        match self {
            Self::NearestPlayer => 'p',
            Self::AllPlayers => 'a',
            Self::RandomPlayer => 'r',
            Self::Myself => 's',
            Self::AllEntities => 'e',
            Self::NearestEntity => 'n',
        }
    }

    fn default_limit_is_one(self) -> bool {
        !matches!(self, Self::AllPlayers | Self::AllEntities)
    }

    fn players_only(self) -> bool {
        matches!(
            self,
            Self::NearestPlayer | Self::AllPlayers | Self::RandomPlayer
        )
    }
}

/// One `key=value` selector option; `value` is raw source text (may start with `!`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectorOption {
    pub key: String,
    pub value: String,
}

/// A parsed entity argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntityTarget {
    Name(String),
    Uuid(String),
    Selector {
        kind: SelectorKind,
        options: Vec<SelectorOption>,
    },
}

/// Option keys vanilla's `EntitySelectorOptions` registers.
pub const SELECTOR_OPTIONS: &[&str] = &[
    "name",
    "distance",
    "level",
    "x",
    "y",
    "z",
    "dx",
    "dy",
    "dz",
    "x_rotation",
    "y_rotation",
    "limit",
    "sort",
    "gamemode",
    "team",
    "type",
    "tag",
    "nbt",
    "scores",
    "advancements",
    "predicate",
];

impl EntityTarget {
    fn option(&self, key: &str) -> impl Iterator<Item = &str> {
        let options: &[SelectorOption] = match self {
            Self::Selector { options, .. } => options,
            _ => &[],
        };
        options
            .iter()
            .filter(move |o| o.key == key)
            .map(|o| o.value.as_str())
    }

    /// Whether this can select more than one entity (`getMaxResults() > 1`).
    pub fn may_select_many(&self) -> bool {
        match self {
            Self::Name(_) | Self::Uuid(_) => false,
            Self::Selector { kind, .. } => match self.option("limit").last() {
                Some(limit) => limit.trim().parse::<i32>().map_or(true, |n| n > 1),
                None => !kind.default_limit_is_one(),
            },
        }
    }

    /// Whether this may select non-player entities (`includesEntities()`).
    pub fn includes_entities(&self) -> bool {
        match self {
            Self::Name(_) => false,
            Self::Uuid(_) => true,
            Self::Selector { kind, .. } => {
                !kind.players_only()
                    && *kind != SelectorKind::Myself
                    && !self
                        .option("type")
                        .any(|t| matches!(t.trim(), "player" | "minecraft:player"))
            }
        }
    }

    pub fn is_self(&self) -> bool {
        matches!(
            self,
            Self::Selector {
                kind: SelectorKind::Myself,
                ..
            }
        )
    }

    /// Whether this is an `@` selector (vanilla's `usesSelector()`), which needs the
    /// `commands/entity_selectors` permission to resolve.
    pub fn is_selector(&self) -> bool {
        matches!(self, Self::Selector { .. })
    }
}

fn is_uuid(s: &str) -> bool {
    let groups: Vec<&str> = s.split('-').collect();
    s.len() <= 36
        && groups.len() == 5
        && groups
            .iter()
            .all(|g| !g.is_empty() && g.chars().all(|c| c.is_ascii_hexdigit()))
}

fn read_option_value(reader: &mut StringReader) -> Result<String, CommandSyntaxError> {
    let start = reader.cursor();
    if reader.peek_is('!') {
        reader.skip();
        reader.skip_whitespace();
    }
    while let Some(c) = reader.peek() {
        match c {
            ',' | ']' => break,
            '"' | '\'' => {
                reader.read_quoted_string()?;
            }
            '{' | '[' => read_balanced(reader)?,
            _ => reader.skip(),
        }
    }
    Ok(reader.string()[start..reader.cursor()]
        .trim_end()
        .to_owned())
}

fn parse_options(reader: &mut StringReader) -> Result<Vec<SelectorOption>, CommandSyntaxError> {
    let mut options = Vec::new();
    reader.skip(); // '['
    reader.skip_whitespace();
    while reader.can_read() && !reader.peek_is(']') {
        reader.skip_whitespace();
        let start = reader.cursor();
        let key = reader.read_string()?;
        if !SELECTOR_OPTIONS.contains(&key.as_str()) {
            reader.set_cursor(start);
            return Err(minecraft::option_unknown(reader, &key));
        }
        reader.skip_whitespace();
        if !reader.peek_is('=') {
            reader.set_cursor(start);
            return Err(minecraft::option_valueless(reader, &key));
        }
        reader.skip();
        reader.skip_whitespace();
        let value_start = reader.cursor();
        let value = read_option_value(reader)?;
        if key == "limit" {
            let mut r = StringReader::new(value.clone());
            let limit = r.read_int().map_err(|_| {
                reader.set_cursor(value_start);
                crate::error::builtin::invalid_int(reader, &value)
            })?;
            if limit < 1 {
                reader.set_cursor(value_start);
                return Err(minecraft::limit_too_small(reader));
            }
        }
        options.push(SelectorOption { key, value });
        reader.skip_whitespace();
        match reader.peek() {
            Some(',') => reader.skip(),
            Some(']') => {}
            _ => return Err(minecraft::options_unterminated(reader)),
        }
    }
    if !reader.can_read() {
        return Err(minecraft::options_unterminated(reader));
    }
    reader.skip(); // ']'
    Ok(options)
}

/// Parses `@x[...]`. The reader must be at `@`.
pub fn parse_selector(reader: &mut StringReader) -> Result<EntityTarget, CommandSyntaxError> {
    reader.skip(); // '@'
    let Some(c) = reader.peek() else {
        return Err(minecraft::selector_missing(reader));
    };
    let Some(kind) = SelectorKind::from_char(c) else {
        return Err(minecraft::selector_unknown(reader, &format!("@{c}")));
    };
    reader.skip();
    let options = if reader.peek_is('[') {
        parse_options(reader)?
    } else {
        Vec::new()
    };
    Ok(EntityTarget::Selector { kind, options })
}

/// `EntitySelectorParser.parse`: a selector, a UUID, or a player name (max 16 chars).
pub fn parse_target(reader: &mut StringReader) -> Result<EntityTarget, CommandSyntaxError> {
    if reader.peek_is('@') {
        return parse_selector(reader);
    }
    let start = reader.cursor();
    let s = reader.read_string()?;
    if is_uuid(&s) {
        return Ok(EntityTarget::Uuid(s));
    }
    if s.is_empty() || s.chars().count() > 16 {
        reader.set_cursor(start);
        return Err(minecraft::entity_invalid(reader));
    }
    Ok(EntityTarget::Name(s))
}

/// `EntityArgument.parse` with its single/players checks (errors are at cursor 0, as in
/// vanilla).
pub fn parse_entity(
    reader: &mut StringReader,
    single: bool,
    players_only: bool,
) -> Result<EntityTarget, CommandSyntaxError> {
    let target = parse_target(reader)?;
    let at_start = |reader: &StringReader| {
        let mut r = reader.clone();
        r.set_cursor(0);
        r
    };
    if single && target.may_select_many() {
        let r = at_start(reader);
        return Err(if players_only {
            minecraft::player_too_many(&r)
        } else {
            minecraft::entity_too_many(&r)
        });
    }
    if players_only && target.includes_entities() && !target.is_self() {
        return Err(minecraft::player_only(&at_start(reader)));
    }
    Ok(target)
}

/// `GameProfileArgument.parse`: a players-only selector, or any text up to a space.
pub fn parse_game_profile(reader: &mut StringReader) -> Result<EntityTarget, CommandSyntaxError> {
    if reader.peek_is('@') {
        let target = parse_selector(reader)?;
        if target.includes_entities() {
            let mut r = reader.clone();
            r.set_cursor(0);
            return Err(minecraft::player_only(&r));
        }
        return Ok(target);
    }
    Ok(EntityTarget::Name(reader.read_until_separator().to_owned()))
}

/// A score holder: `*`, a selector, or a name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScoreHolder {
    /// `*`: every tracked score holder.
    All,
    Target(EntityTarget),
}

impl ScoreHolder {
    /// Whether this is (or contains) an `@` selector.
    pub fn is_selector(&self) -> bool {
        matches!(self, Self::Target(target) if target.is_selector())
    }
}

pub fn parse_score_holder(
    reader: &mut StringReader,
    multiple: bool,
) -> Result<ScoreHolder, CommandSyntaxError> {
    if reader.peek_is('@') {
        let target = parse_selector(reader)?;
        if !multiple && target.may_select_many() {
            return Err(minecraft::entity_too_many(reader));
        }
        return Ok(ScoreHolder::Target(target));
    }
    let start = reader.cursor();
    while reader.peek().is_some_and(|c| c != ARGUMENT_SEPARATOR) {
        reader.skip();
    }
    let name = &reader.string()[start..reader.cursor()];
    if name == "*" {
        Ok(ScoreHolder::All)
    } else {
        Ok(ScoreHolder::Target(EntityTarget::Name(name.to_owned())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity(s: &str, single: bool, players: bool) -> Result<EntityTarget, CommandSyntaxError> {
        parse_entity(&mut StringReader::new(s.to_owned()), single, players)
    }

    #[test]
    fn names_uuids_selectors() {
        assert_eq!(
            entity("Notch", true, true).unwrap(),
            EntityTarget::Name("Notch".into())
        );
        assert!(matches!(
            entity("dd12be42-52a9-4a91-a8a1-11c01849e498", true, false).unwrap(),
            EntityTarget::Uuid(_)
        ));
        let t = entity("@e[type=zombie, limit=1,tag=!x]", true, false).unwrap();
        let EntityTarget::Selector { kind, options } = t else {
            panic!()
        };
        assert_eq!(kind, SelectorKind::AllEntities);
        assert_eq!(options.len(), 3);
        assert_eq!(options[2].value, "!x");
        assert_eq!(
            entity("waytoolongplayername", true, false)
                .unwrap_err()
                .message(),
            "Invalid name or UUID"
        );
    }

    #[test]
    fn selector_errors() {
        assert_eq!(
            entity("@a", true, true).unwrap_err().translation_key(),
            "argument.player.toomany"
        );
        assert_eq!(
            entity("@e", false, true).unwrap_err().translation_key(),
            "argument.player.entities"
        );
        assert!(entity("@e[type=minecraft:player]", false, true).is_ok());
        assert!(entity("@s", true, true).is_ok());
        assert_eq!(
            entity("@x", false, false).unwrap_err().message(),
            "Unknown selector type '@x'"
        );
        assert_eq!(
            entity("@", false, false).unwrap_err().message(),
            "Missing selector type"
        );
        assert_eq!(
            entity("@e[foo=1]", false, false).unwrap_err().message(),
            "Unknown option 'foo'"
        );
        assert_eq!(
            entity("@e[limit=0]", false, false).unwrap_err().message(),
            "Limit must be at least 1"
        );
        assert_eq!(
            entity("@e[tag=a", false, false).unwrap_err().message(),
            "Expected end of options"
        );
        assert!(entity("@e[nbt={a:[1,2]},scores={x=1..}]", false, false).is_ok());
    }

    #[test]
    fn score_holders() {
        let mut r = StringReader::new("*");
        assert_eq!(parse_score_holder(&mut r, true).unwrap(), ScoreHolder::All);
        let mut r = StringReader::new("#fake.name x");
        assert_eq!(
            parse_score_holder(&mut r, false).unwrap(),
            ScoreHolder::Target(EntityTarget::Name("#fake.name".into()))
        );
    }
}
