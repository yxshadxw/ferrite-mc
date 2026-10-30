//! The server's command tree and the game-loop actions its commands emit.
//!
//! Parsing, permission requirements and selectors live here (as in Brigadier); the
//! world state changes those commands describe are applied by the game loop.

use std::collections::HashMap;
use std::net::IpAddr;
use std::str::FromStr;

use ferrite_admin::AdminLists;
use ferrite_command::arguments::{Coordinates, EntityTarget, SelectorKind, WorldCoordinate};
use ferrite_command::{
    ArgumentType, ArgumentValue, CommandContext, CommandDispatcher, CommandError, EntityAnchor,
    argument, literal,
};
use ferrite_net::game::Outbound;
use tokio::sync::mpsc::UnboundedSender;
use uuid::Uuid;

/// A parsed teleport destination. Locations are already resolved against the source.
#[derive(Debug, Clone, PartialEq)]
pub enum TeleportDestination {
    /// An absolute position, with an optional explicit rotation.
    Location {
        pos: [f64; 3],
        yaw: Option<f32>,
        pitch: Option<f32>,
    },
    /// A position to look at after teleporting.
    FacingLocation { pos: [f64; 3], target: [f64; 3] },
    /// An entity to look at after teleporting.
    FacingEntity {
        pos: [f64; 3],
        entity: EntityTarget,
        anchor: EntityAnchor,
    },
    /// Another entity's position.
    Entity(EntityTarget),
}

/// A command's effect, applied by the game loop.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    GameMode(i32),
    Teleport {
        targets: EntityTarget,
        destination: TeleportDestination,
    },
    Message {
        targets: EntityTarget,
        text: String,
    },
    Kill {
        targets: EntityTarget,
    },
    Op {
        targets: EntityTarget,
    },
    Deop {
        targets: EntityTarget,
    },
    WhitelistAdd {
        targets: EntityTarget,
    },
    WhitelistRemove {
        targets: EntityTarget,
    },
    WhitelistList,
    WhitelistOn,
    WhitelistOff,
    WhitelistReload,
    Ban {
        targets: EntityTarget,
        reason: String,
    },
    BanIp {
        target: String,
        reason: String,
    },
    Pardon {
        targets: EntityTarget,
    },
    PardonIp {
        target: String,
    },
    BanList {
        ips: bool,
    },
    Kick {
        targets: EntityTarget,
        reason: String,
    },
    Stop,
    SaveAll {
        flush: bool,
    },
    SaveOn,
    SaveOff,
    Say(String),
    Tps,
    Mspt,
    ProfileStart,
    ProfileStop,
}

/// Who ran a command: permission level, position/orientation for relative and local
/// coordinates, and the channel actions are sent on.
#[derive(Debug, Clone)]
pub struct Source {
    pub permission: u8,
    pub origin: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub actions: UnboundedSender<Action>,
}

/// A player resolved from an entity argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPlayer {
    pub uuid: Uuid,
    pub name: String,
}

/// Online player state the command handlers need.
#[derive(Debug, Clone)]
pub struct OnlinePlayer {
    pub name: String,
    pub outbound: UnboundedSender<Outbound>,
    pub pos: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
}

fn send(source: &Source, action: Action) -> Result<i32, CommandError> {
    source
        .actions
        .send(action)
        .map_err(|_| CommandError::failed("command target unavailable"))?;
    Ok(1)
}

fn entity(ctx: &CommandContext<Source>, name: &str) -> Result<EntityTarget, CommandError> {
    ctx.argument(name)
        .and_then(ArgumentValue::as_entity)
        .cloned()
        .ok_or_else(|| CommandError::failed(format!("missing entity argument '{name}'")))
}

fn coordinates(ctx: &CommandContext<Source>, name: &str) -> Result<Coordinates, CommandError> {
    ctx.argument(name)
        .and_then(ArgumentValue::as_coordinates)
        .copied()
        .ok_or_else(|| CommandError::failed(format!("missing coordinate argument '{name}'")))
}

fn rotation(
    ctx: &CommandContext<Source>,
    name: &str,
) -> Result<[WorldCoordinate; 2], CommandError> {
    match ctx.argument(name) {
        Some(ArgumentValue::Rotation(rotation)) => Ok(*rotation),
        _ => Err(CommandError::failed(format!(
            "missing rotation argument '{name}'"
        ))),
    }
}

fn anchor(ctx: &CommandContext<Source>, name: &str) -> Result<EntityAnchor, CommandError> {
    ctx.argument(name)
        .and_then(ArgumentValue::as_entity_anchor)
        .ok_or_else(|| CommandError::failed(format!("missing anchor argument '{name}'")))
}

fn message_arg(ctx: &CommandContext<Source>, name: &str) -> Result<String, CommandError> {
    ctx.string(name)
        .map(str::to_owned)
        .ok_or_else(|| CommandError::failed(format!("missing text argument '{name}'")))
}

fn self_target() -> EntityTarget {
    EntityTarget::Selector {
        kind: SelectorKind::Myself,
        options: Vec::new(),
    }
}

fn target_or_self(
    ctx: &CommandContext<Source>,
    targets: bool,
) -> Result<EntityTarget, CommandError> {
    if targets {
        entity(ctx, "targets")
    } else {
        Ok(self_target())
    }
}

fn location(ctx: &CommandContext<Source>, targets: bool) -> Result<Action, CommandError> {
    let pos = coordinates(ctx, "location")?.resolve(
        ctx.source().origin,
        ctx.source().yaw,
        ctx.source().pitch,
    );
    Ok(Action::Teleport {
        targets: target_or_self(ctx, targets)?,
        destination: TeleportDestination::Location {
            pos,
            yaw: None,
            pitch: None,
        },
    })
}

fn location_rotation(ctx: &CommandContext<Source>, targets: bool) -> Result<Action, CommandError> {
    let pos = coordinates(ctx, "location")?.resolve(
        ctx.source().origin,
        ctx.source().yaw,
        ctx.source().pitch,
    );
    let (yaw, pitch) = ferrite_command::arguments::coords::resolve_rotation(
        rotation(ctx, "rotation")?,
        ctx.source().yaw,
        ctx.source().pitch,
    );
    Ok(Action::Teleport {
        targets: target_or_self(ctx, targets)?,
        destination: TeleportDestination::Location {
            pos,
            yaw: Some(yaw),
            pitch: Some(pitch),
        },
    })
}

fn location_facing_location(
    ctx: &CommandContext<Source>,
    targets: bool,
) -> Result<Action, CommandError> {
    let source = ctx.source();
    Ok(Action::Teleport {
        targets: target_or_self(ctx, targets)?,
        destination: TeleportDestination::FacingLocation {
            pos: coordinates(ctx, "location")?.resolve(source.origin, source.yaw, source.pitch),
            target: coordinates(ctx, "facingLocation")?.resolve(
                source.origin,
                source.yaw,
                source.pitch,
            ),
        },
    })
}

fn location_facing_entity(
    ctx: &CommandContext<Source>,
    targets: bool,
    anchor_argument: bool,
) -> Result<Action, CommandError> {
    let source = ctx.source();
    Ok(Action::Teleport {
        targets: target_or_self(ctx, targets)?,
        destination: TeleportDestination::FacingEntity {
            pos: coordinates(ctx, "location")?.resolve(source.origin, source.yaw, source.pitch),
            entity: entity(ctx, "facingEntity")?,
            anchor: if anchor_argument {
                anchor(ctx, "facingAnchor")?
            } else {
                EntityAnchor::Feet
            },
        },
    })
}

fn entity_destination(ctx: &CommandContext<Source>, targets: bool) -> Result<Action, CommandError> {
    Ok(Action::Teleport {
        targets: target_or_self(ctx, targets)?,
        destination: TeleportDestination::Entity(entity(ctx, "destination")?),
    })
}

fn location_branch(targets: bool) -> ferrite_command::NodeBuilder<Source> {
    argument("location", ArgumentType::Vec3)
        .executes(move |ctx| send(ctx.source(), location(ctx, targets)?))
        .then(
            literal("rotation").then(
                argument("rotation", ArgumentType::Rotation)
                    .executes(move |ctx| send(ctx.source(), location_rotation(ctx, targets)?)),
            ),
        )
        .then(
            literal("facing")
                .then(
                    literal("entity").then(
                        argument("facingEntity", ArgumentType::entity())
                            .executes(move |ctx| {
                                send(ctx.source(), location_facing_entity(ctx, targets, false)?)
                            })
                            .then(
                                argument("facingAnchor", ArgumentType::EntityAnchor).executes(
                                    move |ctx| {
                                        send(
                                            ctx.source(),
                                            location_facing_entity(ctx, targets, true)?,
                                        )
                                    },
                                ),
                            ),
                    ),
                )
                .then(
                    argument("facingLocation", ArgumentType::Vec3).executes(move |ctx| {
                        send(ctx.source(), location_facing_location(ctx, targets)?)
                    }),
                ),
        )
}

fn destination_branch(targets: bool) -> ferrite_command::NodeBuilder<Source> {
    argument("destination", ArgumentType::entity())
        .executes(move |ctx| send(ctx.source(), entity_destination(ctx, targets)?))
}

fn teleport_builder() -> ferrite_command::NodeBuilder<Source> {
    literal::<Source>("teleport")
        .requires(|source| source.permission >= 2)
        .restricted(true)
        .then(destination_branch(false))
        .then(location_branch(false))
        .then(
            argument("targets", ArgumentType::entities())
                .then(destination_branch(true))
                .then(location_branch(true)),
        )
}

fn message_builder() -> ferrite_command::NodeBuilder<Source> {
    literal::<Source>("msg").then(argument("targets", ArgumentType::players()).then(
        argument("message", ArgumentType::Message).executes(|ctx| {
            send(
                ctx.source(),
                Action::Message {
                    targets: entity(ctx, "targets")?,
                    text: message_arg(ctx, "message")?,
                },
            )
        }),
    ))
}

fn targets_command(
    name: &'static str,
    action: fn(EntityTarget) -> Action,
    argument_type: ArgumentType,
) -> ferrite_command::NodeBuilder<Source> {
    literal::<Source>(name)
        .requires(|source| source.permission >= 2)
        .restricted(true)
        .then(
            argument("targets", argument_type)
                .executes(move |ctx| send(ctx.source(), action(entity(ctx, "targets")?))),
        )
}

fn admin_targets_command(
    name: &'static str,
    action: fn(EntityTarget) -> Action,
) -> ferrite_command::NodeBuilder<Source> {
    literal::<Source>(name)
        .requires(|source| source.permission >= 3)
        .restricted(true)
        .then(
            argument("targets", ArgumentType::GameProfile)
                .executes(move |ctx| send(ctx.source(), action(entity(ctx, "targets")?))),
        )
}

fn whitelist_builder() -> ferrite_command::NodeBuilder<Source> {
    literal::<Source>("whitelist")
        .requires(|source| source.permission >= 3)
        .restricted(true)
        .then(literal("add").then(
            argument("targets", ArgumentType::GameProfile).executes(|ctx| {
                send(
                    ctx.source(),
                    Action::WhitelistAdd {
                        targets: entity(ctx, "targets")?,
                    },
                )
            }),
        ))
        .then(
            literal("remove").then(argument("targets", ArgumentType::GameProfile).executes(
                |ctx| {
                    send(
                        ctx.source(),
                        Action::WhitelistRemove {
                            targets: entity(ctx, "targets")?,
                        },
                    )
                },
            )),
        )
        .then(literal("list").executes(|ctx| send(ctx.source(), Action::WhitelistList)))
        .then(literal("on").executes(|ctx| send(ctx.source(), Action::WhitelistOn)))
        .then(literal("off").executes(|ctx| send(ctx.source(), Action::WhitelistOff)))
        .then(literal("reload").executes(|ctx| send(ctx.source(), Action::WhitelistReload)))
}

fn ban_builder() -> ferrite_command::NodeBuilder<Source> {
    literal::<Source>("ban")
        .requires(|source| source.permission >= 3)
        .restricted(true)
        .then(
            argument("targets", ArgumentType::GameProfile)
                .executes(|ctx| {
                    send(
                        ctx.source(),
                        Action::Ban {
                            targets: entity(ctx, "targets")?,
                            reason: "Banned by an operator.".into(),
                        },
                    )
                })
                .then(argument("reason", ArgumentType::Message).executes(|ctx| {
                    send(
                        ctx.source(),
                        Action::Ban {
                            targets: entity(ctx, "targets")?,
                            reason: message_arg(ctx, "reason")?,
                        },
                    )
                })),
        )
}

fn ban_ip_builder() -> ferrite_command::NodeBuilder<Source> {
    literal::<Source>("ban-ip")
        .requires(|source| source.permission >= 3)
        .restricted(true)
        .then(
            argument("target", ArgumentType::word())
                .executes(|ctx| {
                    send(
                        ctx.source(),
                        Action::BanIp {
                            target: message_arg(ctx, "target")?,
                            reason: "Banned by an operator.".into(),
                        },
                    )
                })
                .then(argument("reason", ArgumentType::Message).executes(|ctx| {
                    send(
                        ctx.source(),
                        Action::BanIp {
                            target: message_arg(ctx, "target")?,
                            reason: message_arg(ctx, "reason")?,
                        },
                    )
                })),
        )
}

fn pardon_ip_builder() -> ferrite_command::NodeBuilder<Source> {
    literal::<Source>("pardon-ip")
        .requires(|source| source.permission >= 3)
        .restricted(true)
        .then(argument("target", ArgumentType::word()).executes(|ctx| {
            send(
                ctx.source(),
                Action::PardonIp {
                    target: message_arg(ctx, "target")?,
                },
            )
        }))
}

fn banlist_builder() -> ferrite_command::NodeBuilder<Source> {
    literal::<Source>("banlist")
        .requires(|source| source.permission >= 3)
        .restricted(true)
        .executes(|ctx| send(ctx.source(), Action::BanList { ips: false }))
        .then(literal("ips").executes(|ctx| send(ctx.source(), Action::BanList { ips: true })))
        .then(literal("players").executes(|ctx| send(ctx.source(), Action::BanList { ips: false })))
}

fn kick_builder() -> ferrite_command::NodeBuilder<Source> {
    literal::<Source>("kick")
        .requires(|source| source.permission >= 3)
        .restricted(true)
        .then(
            argument("targets", ArgumentType::players())
                .executes(|ctx| {
                    send(
                        ctx.source(),
                        Action::Kick {
                            targets: entity(ctx, "targets")?,
                            reason: "Kicked by an operator.".into(),
                        },
                    )
                })
                .then(argument("reason", ArgumentType::Message).executes(|ctx| {
                    send(
                        ctx.source(),
                        Action::Kick {
                            targets: entity(ctx, "targets")?,
                            reason: message_arg(ctx, "reason")?,
                        },
                    )
                })),
        )
}

fn save_all_builder() -> ferrite_command::NodeBuilder<Source> {
    literal::<Source>("save-all")
        .requires(|source| source.permission >= 4)
        .restricted(true)
        .executes(|ctx| send(ctx.source(), Action::SaveAll { flush: false }))
        .then(literal("flush").executes(|ctx| send(ctx.source(), Action::SaveAll { flush: true })))
}

/// The full server command graph.
pub fn dispatcher() -> CommandDispatcher<Source> {
    let mut tree = CommandDispatcher::new();

    // ---- teleport ----
    let teleport = tree.register(teleport_builder());
    tree.register(
        literal::<Source>("tp")
            .requires(|source| source.permission >= 2)
            .restricted(true)
            .redirect(teleport),
    );

    // ---- messaging ----
    let msg = tree.register(message_builder());
    tree.register(literal::<Source>("tell").redirect(msg));
    tree.register(literal::<Source>("w").redirect(msg));

    // ---- entity effects ----
    tree.register(targets_command(
        "kill",
        |targets| Action::Kill { targets },
        ArgumentType::entities(),
    ));

    // ---- administration ----
    tree.register(admin_targets_command("op", |targets| Action::Op {
        targets,
    }));
    tree.register(admin_targets_command("deop", |targets| Action::Deop {
        targets,
    }));
    tree.register(whitelist_builder());
    tree.register(ban_builder());
    tree.register(ban_ip_builder());
    tree.register(admin_targets_command("pardon", |targets| Action::Pardon {
        targets,
    }));
    tree.register(pardon_ip_builder());
    tree.register(banlist_builder());
    tree.register(kick_builder());

    // ---- server control ----
    tree.register(
        literal::<Source>("stop")
            .requires(|source| source.permission >= 4)
            .restricted(true)
            .executes(|ctx| send(ctx.source(), Action::Stop)),
    );
    tree.register(save_all_builder());
    tree.register(
        literal::<Source>("save-on")
            .requires(|source| source.permission >= 4)
            .restricted(true)
            .executes(|ctx| send(ctx.source(), Action::SaveOn)),
    );
    tree.register(
        literal::<Source>("save-off")
            .requires(|source| source.permission >= 4)
            .restricted(true)
            .executes(|ctx| send(ctx.source(), Action::SaveOff)),
    );

    // ---- gamemode (owned by the gamemode worker) ----
    tree.register(
        literal::<Source>("gamemode")
            .requires(|s| s.permission >= 2)
            .then(literal("creative").executes(|ctx| send(ctx.source(), Action::GameMode(1))))
            .then(literal("survival").executes(|ctx| send(ctx.source(), Action::GameMode(0)))),
    );
    tree.register(
        literal::<Source>("say")
            .requires(|s| s.permission >= 2)
            .then(
                argument("message", ArgumentType::greedy_string()).executes(|ctx| {
                    send(
                        ctx.source(),
                        Action::Say(ctx.string("message").unwrap_or_default().to_owned()),
                    )
                }),
            ),
    );
    tree.register(literal::<Source>("tps").executes(|ctx| send(ctx.source(), Action::Tps)));
    tree.register(literal::<Source>("mspt").executes(|ctx| send(ctx.source(), Action::Mspt)));
    tree.register(
        literal::<Source>("profile")
            .requires(|s| s.permission >= 2)
            .then(
                literal::<Source>("start").executes(|ctx| send(ctx.source(), Action::ProfileStart)),
            )
            .then(
                literal::<Source>("stop").executes(|ctx| send(ctx.source(), Action::ProfileStop)),
            ),
    );
    tree
}

/// Resolves players from an entity argument against the online players.
pub fn resolve_online(
    target: &EntityTarget,
    online: &HashMap<Uuid, OnlinePlayer>,
    source: Option<Uuid>,
    origin: [f64; 3],
) -> Result<Vec<ResolvedPlayer>, &'static str> {
    let mut players: Vec<ResolvedPlayer> = match target {
        EntityTarget::Name(name) => online
            .iter()
            .find(|(_, player)| player.name.eq_ignore_ascii_case(name))
            .map(|(uuid, player)| ResolvedPlayer {
                uuid: *uuid,
                name: player.name.clone(),
            })
            .into_iter()
            .collect(),
        EntityTarget::Uuid(raw) => Uuid::parse_str(raw)
            .ok()
            .and_then(|uuid| {
                online.get(&uuid).map(|player| ResolvedPlayer {
                    uuid,
                    name: player.name.clone(),
                })
            })
            .into_iter()
            .collect(),
        EntityTarget::Selector { kind, .. } => match kind {
            SelectorKind::Myself => source
                .and_then(|uuid| {
                    online.get(&uuid).map(|player| ResolvedPlayer {
                        uuid,
                        name: player.name.clone(),
                    })
                })
                .into_iter()
                .collect(),
            SelectorKind::AllPlayers | SelectorKind::AllEntities => online
                .iter()
                .map(|(uuid, player)| ResolvedPlayer {
                    uuid: *uuid,
                    name: player.name.clone(),
                })
                .collect(),
            SelectorKind::RandomPlayer => {
                let mut all: Vec<_> = online
                    .iter()
                    .map(|(uuid, player)| ResolvedPlayer {
                        uuid: *uuid,
                        name: player.name.clone(),
                    })
                    .collect();
                all.sort_by(|a, b| a.name.cmp(&b.name));
                if all.is_empty() {
                    Vec::new()
                } else {
                    let index = (Uuid::new_v4().as_u128() % all.len() as u128) as usize;
                    vec![all.swap_remove(index)]
                }
            }
            SelectorKind::NearestPlayer | SelectorKind::NearestEntity => online
                .iter()
                .min_by(|(_, a), (_, b)| {
                    let distance = |player: &OnlinePlayer| {
                        player
                            .pos
                            .iter()
                            .zip(origin)
                            .map(|(a, b)| (a - b) * (a - b))
                            .sum::<f64>()
                    };
                    distance(a).total_cmp(&distance(b))
                })
                .map(|(uuid, player)| ResolvedPlayer {
                    uuid: *uuid,
                    name: player.name.clone(),
                })
                .into_iter()
                .collect(),
        },
    };
    players.sort_by(|a, b| a.name.cmp(&b.name));
    players.dedup_by_key(|player| player.uuid);

    let empty_key = if matches!(
        target,
        EntityTarget::Selector {
            kind: SelectorKind::Myself,
            ..
        }
    ) {
        "argument.entity.notfound.entity"
    } else {
        "argument.entity.notfound.player"
    };
    if players.is_empty() {
        return Err(empty_key);
    }
    apply_options(target, players, online, origin)
}

fn apply_options(
    target: &EntityTarget,
    players: Vec<ResolvedPlayer>,
    online: &HashMap<Uuid, OnlinePlayer>,
    origin: [f64; 3],
) -> Result<Vec<ResolvedPlayer>, &'static str> {
    let EntityTarget::Selector { options, .. } = target else {
        return Ok(players);
    };
    let mut players = players;
    for option in options {
        match option.key.as_str() {
            "name" => {
                let expected = option.value.trim_matches('"');
                players.retain(|player| player.name == expected);
            }
            "distance" => {
                let range = option.value.trim();
                let (min, max) = range.split_once("..").map_or_else(
                    || match range.parse::<f64>() {
                        Ok(value) => (value, value),
                        Err(_) => (0.0, f64::INFINITY),
                    },
                    |(min, max)| {
                        (
                            min.parse().unwrap_or(0.0),
                            max.parse().unwrap_or(f64::INFINITY),
                        )
                    },
                );
                let distance = |player: &ResolvedPlayer| {
                    online.get(&player.uuid).map_or(0.0, |p| {
                        p.pos
                            .iter()
                            .zip(origin)
                            .map(|(a, b)| (a - b) * (a - b))
                            .sum::<f64>()
                            .sqrt()
                    })
                };
                players.retain(|player| {
                    let d = distance(player);
                    d >= min && d <= max
                });
            }
            "limit" => {
                if let Ok(limit) = option.value.parse::<usize>() {
                    players.truncate(limit);
                }
            }
            "sort" => match option.value.as_str() {
                "nearest" => players.sort_by(|a, b| {
                    let distance = |player: &ResolvedPlayer| {
                        online.get(&player.uuid).map_or(0.0, |p| {
                            p.pos
                                .iter()
                                .zip(origin)
                                .map(|(x, y)| (x - y) * (x - y))
                                .sum::<f64>()
                        })
                    };
                    distance(a).total_cmp(&distance(b))
                }),
                "furthest" => players.sort_by(|a, b| {
                    let distance = |player: &ResolvedPlayer| {
                        online.get(&player.uuid).map_or(0.0, |p| {
                            p.pos
                                .iter()
                                .zip(origin)
                                .map(|(x, y)| (x - y) * (x - y))
                                .sum::<f64>()
                        })
                    };
                    distance(b).total_cmp(&distance(a))
                }),
                "random" => {
                    let length = players.len();
                    if length > 0 {
                        let index = (Uuid::new_v4().as_u128() % length as u128) as usize;
                        players.swap(0, index);
                    }
                }
                _ => return Err("argument.entity.options.sort.irreversible"),
            },
            _ => {}
        }
    }
    Ok(players)
}

/// Resolves game profiles for admin commands: online players first, then the profile cache.
///
/// Names that are neither online nor cached are rejected, matching a server that has never
/// seen the profile.
pub fn resolve_profiles(
    target: &EntityTarget,
    online: &HashMap<Uuid, OnlinePlayer>,
    admin: Option<&AdminLists>,
    source: Option<Uuid>,
    origin: [f64; 3],
) -> Result<Vec<ResolvedPlayer>, &'static str> {
    let online_players = resolve_online(target, online, source, origin).unwrap_or_default();
    if !online_players.is_empty() || matches!(target, EntityTarget::Selector { .. }) {
        if online_players.is_empty() {
            return Err("argument.player.unknown");
        }
        return Ok(online_players);
    }
    let (raw, uuid) = match target {
        EntityTarget::Name(raw) => (raw.as_str(), Uuid::parse_str(raw).ok()),
        EntityTarget::Uuid(raw) => (raw.as_str(), Uuid::parse_str(raw).ok()),
        EntityTarget::Selector { .. } => unreachable!(),
    };
    if let Some(uuid) = uuid {
        let name = online
            .get(&uuid)
            .map(|player| player.name.clone())
            .or_else(|| admin.and_then(|admin| admin.usercache.name_for(uuid)))
            .unwrap_or_else(|| uuid.to_string());
        return Ok(vec![ResolvedPlayer { uuid, name }]);
    }
    let Some(admin) = admin else {
        return Err("argument.player.unknown");
    };
    let Some(uuid) = admin.usercache.uuid_for(raw) else {
        return Err("argument.player.unknown");
    };
    let name = admin
        .usercache
        .name_for(uuid)
        .unwrap_or_else(|| raw.to_owned());
    Ok(vec![ResolvedPlayer { uuid, name }])
}

/// Whether a string is a literal IP address.
pub fn parse_ip(target: &str) -> Option<IpAddr> {
    IpAddr::from_str(target).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(permission: u8) -> Source {
        let (actions, _) = tokio::sync::mpsc::unbounded_channel();
        Source {
            permission,
            origin: [0.0, 64.0, 0.0],
            yaw: 0.0,
            pitch: 0.0,
            actions,
        }
    }

    fn run(input: &str, permission: u8) -> (Result<i32, CommandError>, Vec<Action>) {
        let tree = dispatcher();
        let (actions, mut received) = tokio::sync::mpsc::unbounded_channel();
        let mut source = source(permission);
        source.actions = actions;
        let result = tree.execute(input, source);
        (
            result,
            std::iter::from_fn(|| received.try_recv().ok()).collect(),
        )
    }

    fn only(input: &str, permission: u8) -> Action {
        let (result, actions) = run(input, permission);
        assert!(result.is_ok(), "{input}: {result:?}");
        assert_eq!(actions.len(), 1, "{input}: {actions:?}");
        actions.into_iter().next().unwrap()
    }

    fn denied(input: &str) {
        let (result, actions) = run(input, 0);
        assert!(result.is_err(), "{input} should be denied");
        assert!(actions.is_empty(), "{input} produced {actions:?}");
    }

    #[test]
    fn tp_and_teleport_accept_vanilla_forms() {
        denied("tp 1 2 3");
        denied("teleport 1 2 3");

        assert_eq!(
            only("tp 10 70 20", 2),
            Action::Teleport {
                targets: self_target(),
                destination: TeleportDestination::Location {
                    pos: [10.5, 70.0, 20.5],
                    yaw: None,
                    pitch: None,
                },
            }
        );
        assert_eq!(
            only("teleport @a ~ ~5 ~", 2),
            Action::Teleport {
                targets: EntityTarget::Selector {
                    kind: SelectorKind::AllPlayers,
                    options: Vec::new(),
                },
                destination: TeleportDestination::Location {
                    pos: [0.0, 69.0, 0.0],
                    yaw: None,
                    pitch: None,
                },
            }
        );
        assert_eq!(
            only("tp @s ^1 ^2 ^3", 2),
            Action::Teleport {
                targets: self_target(),
                destination: TeleportDestination::Location {
                    pos: [1.0, 66.0, 3.0],
                    yaw: None,
                    pitch: None,
                },
            }
        );
        assert_eq!(
            only("tp @s 10 70 20 facing 0 64 0", 2),
            Action::Teleport {
                targets: self_target(),
                destination: TeleportDestination::FacingLocation {
                    pos: [10.5, 70.0, 20.5],
                    target: [0.5, 64.0, 0.5],
                },
            }
        );
        assert_eq!(
            only("tp @s 10 70 20 facing entity Alice eyes", 2),
            Action::Teleport {
                targets: self_target(),
                destination: TeleportDestination::FacingEntity {
                    pos: [10.5, 70.0, 20.5],
                    entity: EntityTarget::Name("Alice".into()),
                    anchor: EntityAnchor::Eyes,
                },
            }
        );
        assert_eq!(
            only("tp @s 10 70 20 rotation ~90 ~-10", 2),
            Action::Teleport {
                targets: self_target(),
                destination: TeleportDestination::Location {
                    pos: [10.5, 70.0, 20.5],
                    yaw: Some(90.0),
                    pitch: Some(-10.0),
                },
            }
        );
        assert_eq!(
            only("tp Alice", 2),
            Action::Teleport {
                targets: self_target(),
                destination: TeleportDestination::Entity(EntityTarget::Name("Alice".into())),
            }
        );
        assert!(run("tp @s 1 2 3 facing entity @p head", 2).0.is_err());
    }

    #[test]
    fn msg_tell_and_w_message_players() {
        let action = only("msg Alice hello world", 0);
        assert_eq!(
            action,
            Action::Message {
                targets: EntityTarget::Name("Alice".into()),
                text: "hello world".into(),
            }
        );
        assert_eq!(only("tell Alice hi", 0), {
            Action::Message {
                targets: EntityTarget::Name("Alice".into()),
                text: "hi".into(),
            }
        });
        assert_eq!(only("w @a hi", 0), {
            Action::Message {
                targets: EntityTarget::Selector {
                    kind: SelectorKind::AllPlayers,
                    options: Vec::new(),
                },
                text: "hi".into(),
            }
        });
        assert!(run("msg", 0).0.is_err());
    }

    #[test]
    fn kill_requires_gamemaster() {
        denied("kill @a");
        assert_eq!(
            only("kill @a", 2),
            Action::Kill {
                targets: EntityTarget::Selector {
                    kind: SelectorKind::AllPlayers,
                    options: Vec::new(),
                },
            }
        );
        assert!(run("kill", 2).0.is_err());
    }

    #[test]
    fn op_and_deop_require_admin() {
        denied("op Alice");
        denied("deop Alice");
        assert_eq!(
            only("op Alice", 3),
            Action::Op {
                targets: EntityTarget::Name("Alice".into()),
            }
        );
        assert_eq!(
            only("deop Alice", 3),
            Action::Deop {
                targets: EntityTarget::Name("Alice".into()),
            }
        );
        assert!(run("op", 3).0.is_err());
    }

    #[test]
    fn whitelist_requires_admin() {
        denied("whitelist add Alice");
        denied("whitelist on");
        assert_eq!(
            only("whitelist add Alice", 3),
            Action::WhitelistAdd {
                targets: EntityTarget::Name("Alice".into()),
            }
        );
        assert_eq!(
            only("whitelist remove Alice", 3),
            Action::WhitelistRemove {
                targets: EntityTarget::Name("Alice".into()),
            }
        );
        assert_eq!(only("whitelist list", 3), Action::WhitelistList);
        assert_eq!(only("whitelist on", 3), Action::WhitelistOn);
        assert_eq!(only("whitelist off", 3), Action::WhitelistOff);
        assert_eq!(only("whitelist reload", 3), Action::WhitelistReload);
        assert!(run("whitelist add", 3).0.is_err());
    }

    #[test]
    fn ban_requires_admin_and_defaults_the_reason() {
        denied("ban Alice");
        assert_eq!(
            only("ban Alice", 3),
            Action::Ban {
                targets: EntityTarget::Name("Alice".into()),
                reason: "Banned by an operator.".into(),
            }
        );
        assert_eq!(
            only("ban Alice griefing", 3),
            Action::Ban {
                targets: EntityTarget::Name("Alice".into()),
                reason: "griefing".into(),
            }
        );
        assert!(run("ban", 3).0.is_err());
    }

    #[test]
    fn ban_ip_requires_admin_and_accepts_a_reason() {
        denied("ban-ip 127.0.0.1");
        assert_eq!(
            only("ban-ip 127.0.0.1", 3),
            Action::BanIp {
                target: "127.0.0.1".into(),
                reason: "Banned by an operator.".into(),
            }
        );
        assert_eq!(
            only("ban-ip 127.0.0.1 griefing", 3),
            Action::BanIp {
                target: "127.0.0.1".into(),
                reason: "griefing".into(),
            }
        );
        assert!(run("ban-ip", 3).0.is_err());
    }

    #[test]
    fn pardon_commands_require_admin() {
        denied("pardon Alice");
        denied("pardon-ip 127.0.0.1");
        assert_eq!(
            only("pardon Alice", 3),
            Action::Pardon {
                targets: EntityTarget::Name("Alice".into()),
            }
        );
        assert_eq!(
            only("pardon-ip 127.0.0.1", 3),
            Action::PardonIp {
                target: "127.0.0.1".into(),
            }
        );
        assert!(run("pardon", 3).0.is_err());
    }

    #[test]
    fn banlist_requires_admin_and_lists_both_kinds() {
        denied("banlist");
        assert_eq!(only("banlist", 3), Action::BanList { ips: false });
        assert_eq!(only("banlist players", 3), Action::BanList { ips: false });
        assert_eq!(only("banlist ips", 3), Action::BanList { ips: true });
        assert!(run("banlist everything", 3).0.is_err());
    }

    #[test]
    fn kick_requires_admin_and_defaults_the_reason() {
        denied("kick Alice");
        assert_eq!(
            only("kick Alice", 3),
            Action::Kick {
                targets: EntityTarget::Name("Alice".into()),
                reason: "Kicked by an operator.".into(),
            }
        );
        assert_eq!(
            only("kick @a nice try", 3),
            Action::Kick {
                targets: EntityTarget::Selector {
                    kind: SelectorKind::AllPlayers,
                    options: Vec::new(),
                },
                reason: "nice try".into(),
            }
        );
        assert!(run("kick", 3).0.is_err());
    }

    #[test]
    fn stop_requires_owner() {
        denied("stop");
        assert!(run("stop", 3).0.is_err());
        assert_eq!(only("stop", 4), Action::Stop);
    }

    #[test]
    fn save_all_requires_owner_and_accepts_flush() {
        denied("save-all");
        assert!(run("save-all", 3).0.is_err());
        assert_eq!(only("save-all", 4), Action::SaveAll { flush: false });
        assert_eq!(only("save-all flush", 4), Action::SaveAll { flush: true });
        assert!(run("save-all slowly", 4).0.is_err());
    }

    #[test]
    fn entity_and_profile_resolution_use_online_then_cache() {
        use ferrite_admin::AdminLists;

        let alice = Uuid::from_u128(1);
        let bob = Uuid::from_u128(2);
        let carol = Uuid::from_u128(3);
        let mut online = HashMap::new();
        for (uuid, name, pos) in [
            (alice, "Alice", [0.0, 64.0, 0.0]),
            (bob, "Bob", [100.0, 64.0, 0.0]),
            (carol, "Carol", [10.0, 64.0, 0.0]),
        ] {
            let (outbound, _) = tokio::sync::mpsc::unbounded_channel();
            online.insert(
                uuid,
                OnlinePlayer {
                    name: name.into(),
                    outbound,
                    pos,
                    yaw: 0.0,
                    pitch: 0.0,
                },
            );
        }

        let name = |name: &str| EntityTarget::Name(name.into());
        assert_eq!(
            resolve_online(&name("alice"), &online, None, [0.0; 3]).unwrap(),
            [ResolvedPlayer {
                uuid: alice,
                name: "Alice".into()
            }]
        );
        assert_eq!(
            resolve_online(&name("Nobody"), &online, None, [0.0; 3]).unwrap_err(),
            "argument.entity.notfound.player"
        );
        let all = |options| EntityTarget::Selector {
            kind: SelectorKind::AllPlayers,
            options,
        };
        let names = |players: Vec<ResolvedPlayer>| {
            players
                .into_iter()
                .map(|player| player.name)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            names(resolve_online(&all(Vec::new()), &online, None, [0.0; 3]).unwrap()),
            ["Alice", "Bob", "Carol"]
        );
        assert_eq!(
            names(
                resolve_online(
                    &all(vec![ferrite_command::arguments::SelectorOption {
                        key: "limit".into(),
                        value: "1".into()
                    }]),
                    &online,
                    None,
                    [0.0; 3]
                )
                .unwrap()
            ),
            ["Alice"]
        );
        assert_eq!(
            names(
                resolve_online(
                    &all(vec![ferrite_command::arguments::SelectorOption {
                        key: "name".into(),
                        value: "Bob".into()
                    }]),
                    &online,
                    None,
                    [0.0; 3]
                )
                .unwrap()
            ),
            ["Bob"]
        );
        assert_eq!(
            names(
                resolve_online(
                    &EntityTarget::Selector {
                        kind: SelectorKind::NearestPlayer,
                        options: Vec::new()
                    },
                    &online,
                    None,
                    [100.0, 64.0, 0.0]
                )
                .unwrap()
            ),
            ["Bob"]
        );
        assert_eq!(
            resolve_online(&self_target(), &online, Some(carol), [0.0; 3]).unwrap(),
            [ResolvedPlayer {
                uuid: carol,
                name: "Carol".into()
            }]
        );

        let dir = std::env::temp_dir().join(format!("ferrite-command-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let admin = AdminLists::load(&dir).unwrap();
        let offline = Uuid::from_u128(4);
        admin.usercache.cache("Offline", offline);
        assert_eq!(
            resolve_profiles(&name("Offline"), &online, Some(&admin), None, [0.0; 3]).unwrap(),
            [ResolvedPlayer {
                uuid: offline,
                name: "Offline".into()
            }]
        );
        assert_eq!(
            resolve_profiles(&name("Ghost"), &online, Some(&admin), None, [0.0; 3]).unwrap_err(),
            "argument.player.unknown"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn save_on_and_off_require_owner() {
        denied("save-on");
        denied("save-off");
        assert!(run("save-on", 3).0.is_err());
        assert_eq!(only("save-on", 4), Action::SaveOn);
        assert_eq!(only("save-off", 4), Action::SaveOff);
    }
}
