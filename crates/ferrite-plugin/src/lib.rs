//! Hosts plugins in an embedded JVM.
use ferrite_jvm::jni::{
    JNIEnv,
    objects::{JClass, JObject, JString, JValue},
};
use ferrite_jvm::{Jvm, JvmConfig, NativeMethod};
pub use scheduling::{EntityTask, RegionTask};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

static LOGS: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
static REGION_QUEUE: OnceLock<Mutex<scheduling::Queue>> = OnceLock::new();
static READY: OnceLock<Mutex<Vec<RegionTask>>> = OnceLock::new();
static READY_ENTITIES: OnceLock<Mutex<Vec<EntityTask>>> = OnceLock::new();
static MODES: OnceLock<HashMap<String, bool>> = OnceLock::new();
static REGION_CONTEXT: OnceLock<Option<std::sync::Arc<dyn RegionContext>>> = OnceLock::new();
static SERVER_HANDLER: OnceLock<Option<ServerHandler>> = OnceLock::new();
static WORLD_HANDLER: OnceLock<Option<WorldHandler>> = OnceLock::new();
static PLAYER_HANDLER: OnceLock<Option<PlayerHandler>> = OnceLock::new();
static COMMAND_HANDLER: OnceLock<Option<CommandHandler>> = OnceLock::new();
thread_local! { static REGION: std::cell::RefCell<Option<(String, i32, i32)>> = const { std::cell::RefCell::new(None) }; }
thread_local! { static CAPTURED_MESSAGES: std::cell::RefCell<Option<Vec<PluginMessage>>> = const { std::cell::RefCell::new(None) }; }
const BOOTSTRAP: &str = "dev/ferrite/server/Bootstrap";
/// Bukkit event classes the server forwards; keep in sync with the Java bridge.
pub const PLAYER_JOIN_EVENT: &str = "org.bukkit.event.player.PlayerJoinEvent";
pub const PLAYER_QUIT_EVENT: &str = "org.bukkit.event.player.PlayerQuitEvent";
pub const ASYNC_CHAT_EVENT: &str = "io.papermc.paper.event.player.AsyncChatEvent";
pub const ASYNC_PLAYER_CHAT_EVENT: &str = "org.bukkit.event.player.AsyncPlayerChatEvent";
pub const PLAYER_MOVE_EVENT: &str = "org.bukkit.event.player.PlayerMoveEvent";
/// Event classes with at least one registered listener, reported by the JVM.
static LISTENED_EVENTS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

#[derive(Default)]
pub struct PluginConfig {
    pub bridge_jar: PathBuf,
    pub java_home: Option<PathBuf>,
    pub command_handler: Option<CommandHandler>,
    pub region_context: Option<std::sync::Arc<dyn RegionContext>>,
    pub server_snapshot: ServerSnapshot,
    pub player_handler: Option<PlayerHandler>,
    pub world_handler: Option<WorldHandler>,
    pub server_handler: Option<ServerHandler>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BlockPosition {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}
/// Where a player was and where a movement packet takes it; Bukkit's `PlayerMoveEvent`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerMove {
    pub from: [f64; 3],
    pub to: [f64; 3],
    pub from_yaw: f32,
    pub from_pitch: f32,
    pub to_yaw: f32,
    pub to_pitch: f32,
}
#[derive(Clone, Debug)]
pub struct BlockSnapshot {
    pub position: BlockPosition,
    pub material: String,
}
/// Only explicitly supplied blocks are readable; absent positions are unknown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColumnHeight {
    pub x: i32,
    pub z: i32,
    pub y: i32,
}
#[derive(Clone, Debug)]
pub struct WorldSnapshot {
    pub id: uuid::Uuid,
    pub name: String,
    pub time: i64,
    pub spawn: BlockPosition,
    pub blocks: Vec<BlockSnapshot>,
    pub heights: Vec<ColumnHeight>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorldCommand {
    SetBlock {
        world: uuid::Uuid,
        position: BlockPosition,
        material: String,
    },
}
pub type WorldHandler = std::sync::Arc<dyn Fn(WorldCommand) + Send + Sync>;
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerLocation {
    pub world: uuid::Uuid,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub yaw: f32,
    pub pitch: f32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemSnapshot {
    pub material: String,
    pub amount: u8,
}
#[derive(Clone, Debug)]
pub struct PlayerSnapshot {
    pub id: uuid::Uuid,
    pub name: String,
    pub operator: bool,
    pub health: f64,
    pub food: u8,
    pub mode: GameMode,
    pub location: PlayerLocation,
    pub inventory: Vec<Option<ItemSnapshot>>,
}
/// A player the server knows but that is not connected.
#[derive(Clone, Debug)]
pub struct OfflinePlayerSnapshot {
    pub id: uuid::Uuid,
    pub name: Option<String>,
    pub operator: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MessageContent {
    Plain(String),
    AdventureJson(String),
}
#[derive(Clone, Debug, PartialEq)]
pub enum PlayerCommand {
    Message {
        player: uuid::Uuid,
        content: MessageContent,
    },
    Teleport {
        player: uuid::Uuid,
        location: PlayerLocation,
    },
    SetInventorySlot {
        player: uuid::Uuid,
        slot: u8,
        item: Option<ItemSnapshot>,
    },
    SetOperator {
        player: uuid::Uuid,
        operator: bool,
    },
    DisplayName {
        player: uuid::Uuid,
        component_json: Option<String>,
    },
    Kick {
        player: uuid::Uuid,
        component_json: Option<String>,
    },
    Title {
        player: uuid::Uuid,
        title: Option<String>,
        subtitle: Option<String>,
        fade_in: i32,
        stay: i32,
        fade_out: i32,
    },
    PlaySound {
        player: uuid::Uuid,
        world: uuid::Uuid,
        sound: String,
        category: SoundCategory,
        x: f64,
        y: f64,
        z: f64,
        volume: f32,
        pitch: f32,
        seed: Option<i64>,
    },
    SetHealth {
        player: uuid::Uuid,
        health: f64,
    },
    SetFood {
        player: uuid::Uuid,
        food: u8,
    },
    SetGameMode {
        player: uuid::Uuid,
        mode: GameMode,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundCategory {
    Master,
    Music,
    Records,
    Weather,
    Blocks,
    Hostile,
    Neutral,
    Players,
    Ambient,
    Voice,
    Ui,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameMode {
    Survival,
    Creative,
    Adventure,
    Spectator,
}
pub type PlayerHandler = std::sync::Arc<dyn Fn(PlayerCommand) + Send + Sync>;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServerCommand {
    BanIp {
        address: std::net::IpAddr,
        reason: Option<String>,
        expires_unix_millis: Option<i64>,
        source: Option<String>,
    },
    PardonIp {
        address: std::net::IpAddr,
    },
    SetWhitelist {
        enabled: bool,
    },
}
pub type ServerHandler = std::sync::Arc<dyn Fn(ServerCommand) + Send + Sync>;
/// Server settings made visible to plugins during startup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerSnapshot {
    pub motd: String,
    pub max_players: u16,
    pub port: u16,
    pub view_distance: u8,
    pub whitelist: bool,
    pub online_mode: bool,
    pub banned_ips: Vec<std::net::IpAddr>,
}
impl Default for ServerSnapshot {
    fn default() -> Self {
        Self {
            motd: "Ferrite".into(),
            max_players: 20,
            port: 25565,
            view_distance: 10,
            whitelist: false,
            online_mode: true,
            banned_ips: Vec::new(),
        }
    }
}
/// Server-owned routing and thread ownership information for region-aware plugins.
pub trait RegionContext: Send + Sync {
    fn is_owned_by_current_region(&self, world: uuid::Uuid, chunk_x: i32, chunk_z: i32) -> bool;
    fn schedule_on_region(&self, task: RegionTask);
    fn schedule_on_entity(&self, task: EntityTask);
    fn retire_entity(&self, entity: uuid::Uuid);
}
pub type CommandHandler = std::sync::Arc<dyn Fn(CommandSender, &str) -> bool + Send + Sync>;
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Jvm(#[from] ferrite_jvm::JvmError),
    #[error("plugin metadata rejected: {0}")]
    Metadata(String),
    #[error("task was not issued or has already been consumed")]
    InvalidTask,
    #[error("region context is not configured")]
    MissingRegionContext,
    #[error("invalid player snapshot")]
    InvalidPlayerSnapshot,
    #[error("invalid offline player snapshot")]
    InvalidOfflinePlayerSnapshot,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandSender {
    Console,
    Player(uuid::Uuid),
}
/// A command declared by a plugin's `plugin.yml` or `paper-plugin.yml`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginCommand {
    pub name: String,
    pub aliases: Vec<String>,
    pub permission: Option<String>,
    pub usage: Option<String>,
}
/// A message a plugin sent to one player during a callback.
#[derive(Clone, Debug, PartialEq)]
pub struct PluginMessage {
    pub player: uuid::Uuid,
    pub content: MessageContent,
}
/// Result of dispatching a line to a plugin `CommandExecutor`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PluginCommandResult {
    pub handled: bool,
    /// Messages captured from `CommandSender.sendMessage` during the call.
    pub messages: Vec<PluginMessage>,
}
/// Server hooks the game loop forwards to plugins.
pub trait ServerHooks: Send + Sync {
    /// Fires Bukkit's `PlayerJoinEvent` for a player that just joined.
    fn player_joined(&self, id: uuid::Uuid, name: &str) -> Result<(), Error>;
    /// Fires Bukkit's `PlayerQuitEvent` before the player is removed.
    fn player_left(&self, id: uuid::Uuid, name: &str) -> Result<(), Error>;
    /// Fires Paper's `AsyncChatEvent` and Bukkit's `AsyncPlayerChatEvent`.
    ///
    /// Returns the message after listeners ran, or `None` when one cancelled chat.
    fn chat(&self, id: uuid::Uuid, name: &str, message: &str) -> Result<Option<String>, Error>;
    /// Fires Bukkit's `PlayerMoveEvent`; `false` means a listener cancelled the move.
    fn player_moved(
        &self,
        id: uuid::Uuid,
        name: &str,
        movement: &PlayerMove,
    ) -> Result<bool, Error>;
    /// Commands declared by enabled plugins.
    fn plugin_commands(&self) -> Result<Vec<PluginCommand>, Error>;
    /// Runs a plugin command for `sender`, returning captured `sendMessage` output.
    fn run_plugin_command(
        &self,
        sender: CommandSender,
        line: &str,
    ) -> Result<PluginCommandResult, Error>;
}
pub struct PluginHost {
    jvm: Jvm,
    issued_regions: Mutex<Vec<RegionTask>>,
    issued_entities: Mutex<Vec<EntityTask>>,
    region_context: Option<std::sync::Arc<dyn RegionContext>>,
}
extern "system" fn log(mut env: JNIEnv, _: JClass, message: JString) {
    if let Ok(s) = env.get_string(&message) {
        LOGS.get_or_init(Default::default)
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(s.into());
    }
}
fn listened_events() -> &'static Mutex<HashSet<String>> {
    LISTENED_EVENTS.get_or_init(Default::default)
}
/// Records that a plugin registered a listener for one Bukkit event class.
extern "system" fn event_registered(mut env: JNIEnv, _: JClass, event: JString) {
    if let Ok(event) = env.get_string(&event) {
        let event: String = event.into();
        listened_events()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(event);
    }
}
/// Forgets every listener after the JVM disabled all plugins.
extern "system" fn events_cleared(_: JNIEnv, _: JClass) {
    listened_events()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clear();
}
extern "system" fn server_whitelist(_: JNIEnv, _: JClass, enabled: u8) -> u8 {
    let Some(handler) = SERVER_HANDLER.get().and_then(Option::as_ref) else {
        return 0;
    };
    u8::from(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            handler(ServerCommand::SetWhitelist {
                enabled: enabled != 0,
            })
        }))
        .is_ok(),
    )
}
extern "system" fn server_ban(
    mut env: JNIEnv,
    _: JClass,
    address: JString,
    reason: JString,
    expires: i64,
    source: JString,
    ban: u8,
) -> u8 {
    let Ok(address) = env.get_string(&address) else {
        return 0;
    };
    let address: String = address.into();
    let Ok(address) = address.parse() else {
        return 0;
    };
    let reason = if reason.is_null() {
        None
    } else {
        match env.get_string(&reason) {
            Ok(value) => Some(value.into()),
            Err(_) => return 0,
        }
    };
    let source = if source.is_null() {
        None
    } else {
        match env.get_string(&source) {
            Ok(value) => Some(value.into()),
            Err(_) => return 0,
        }
    };
    let command = if ban == 0 {
        ServerCommand::PardonIp { address }
    } else {
        ServerCommand::BanIp {
            address,
            reason,
            expires_unix_millis: (expires >= 0).then_some(expires),
            source,
        }
    };
    let Some(handler) = SERVER_HANDLER.get().and_then(Option::as_ref) else {
        return 0;
    };
    u8::from(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handler(command))).is_ok())
}
extern "system" fn world_set_block(
    mut env: JNIEnv,
    _: JClass,
    world: JString,
    x: i32,
    y: i32,
    z: i32,
    material: JString,
) -> u8 {
    let Ok(world) = env.get_string(&world) else {
        return 0;
    };
    let world: String = world.into();
    let Ok(world) = uuid::Uuid::parse_str(&world) else {
        return 0;
    };
    let Ok(material) = env.get_string(&material) else {
        return 0;
    };
    let material: String = material.into();
    let Some(handler) = WORLD_HANDLER.get().and_then(Option::as_ref) else {
        return 0;
    };
    u8::from(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            handler(WorldCommand::SetBlock {
                world,
                position: BlockPosition { x, y, z },
                material,
            })
        }))
        .is_ok(),
    )
}
extern "system" fn player_message(
    mut env: JNIEnv,
    _: JClass,
    player: JString,
    message: JString,
    component: u8,
) {
    let Ok(player) = env.get_string(&player) else {
        return;
    };
    let player: String = player.into();
    let Ok(player) = uuid::Uuid::parse_str(&player) else {
        return;
    };
    let Ok(message) = env.get_string(&message) else {
        return;
    };
    let message: String = message.into();
    let content = if component == 0 {
        MessageContent::Plain(message)
    } else {
        MessageContent::AdventureJson(message)
    };
    let captured = CAPTURED_MESSAGES.with(|captured| {
        let mut captured = captured.borrow_mut();
        let Some(messages) = captured.as_mut() else {
            return false;
        };
        messages.push(PluginMessage {
            player,
            content: content.clone(),
        });
        true
    });
    if captured {
        return;
    }
    if let Some(handler) = PLAYER_HANDLER.get().and_then(Option::as_ref) {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            handler(PlayerCommand::Message { player, content })
        }));
    }
}
extern "system" fn player_sound(
    mut env: JNIEnv,
    _: JClass,
    player: JString,
    world: JString,
    sound: JString,
    category: i32,
    x: f64,
    y: f64,
    z: f64,
    volume: f32,
    pitch: f32,
    has_seed: u8,
    seed: i64,
) -> u8 {
    if !x.is_finite()
        || !y.is_finite()
        || !z.is_finite()
        || !volume.is_finite()
        || !pitch.is_finite()
    {
        return 0;
    }
    let Ok(player) = env.get_string(&player) else {
        return 0;
    };
    let player: String = player.into();
    let Ok(player) = uuid::Uuid::parse_str(&player) else {
        return 0;
    };
    let Ok(world) = env.get_string(&world) else {
        return 0;
    };
    let world: String = world.into();
    let Ok(world) = uuid::Uuid::parse_str(&world) else {
        return 0;
    };
    let Ok(sound) = env.get_string(&sound) else {
        return 0;
    };
    let sound: String = sound.into();
    let category = match category {
        0 => SoundCategory::Master,
        1 => SoundCategory::Music,
        2 => SoundCategory::Records,
        3 => SoundCategory::Weather,
        4 => SoundCategory::Blocks,
        5 => SoundCategory::Hostile,
        6 => SoundCategory::Neutral,
        7 => SoundCategory::Players,
        8 => SoundCategory::Ambient,
        9 => SoundCategory::Voice,
        10 => SoundCategory::Ui,
        _ => return 0,
    };
    let Some(handler) = PLAYER_HANDLER.get().and_then(Option::as_ref) else {
        return 0;
    };
    u8::from(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            handler(PlayerCommand::PlaySound {
                player,
                world,
                sound,
                category,
                x,
                y,
                z,
                volume,
                pitch,
                seed: (has_seed != 0).then_some(seed),
            })
        }))
        .is_ok(),
    )
}
extern "system" fn player_visual(
    mut env: JNIEnv,
    _: JClass,
    player: JString,
    kind: i32,
    first: JString,
    second: JString,
    fade_in: i32,
    stay: i32,
    fade_out: i32,
) -> u8 {
    let Ok(player) = env.get_string(&player) else {
        return 0;
    };
    let player: String = player.into();
    let Ok(player) = uuid::Uuid::parse_str(&player) else {
        return 0;
    };
    let first = if first.is_null() {
        None
    } else {
        match env.get_string(&first) {
            Ok(value) => Some(value.into()),
            Err(_) => return 0,
        }
    };
    let second = if second.is_null() {
        None
    } else {
        match env.get_string(&second) {
            Ok(value) => Some(value.into()),
            Err(_) => return 0,
        }
    };
    let command = match kind {
        0 => PlayerCommand::DisplayName {
            player,
            component_json: first,
        },
        1 => PlayerCommand::Kick {
            player,
            component_json: first,
        },
        2 => PlayerCommand::Title {
            player,
            title: first,
            subtitle: second,
            fade_in,
            stay,
            fade_out,
        },
        _ => return 0,
    };
    let Some(handler) = PLAYER_HANDLER.get().and_then(Option::as_ref) else {
        return 0;
    };
    u8::from(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handler(command))).is_ok())
}
extern "system" fn player_operator(
    mut env: JNIEnv,
    _: JClass,
    player: JString,
    operator: u8,
) -> u8 {
    let Ok(player) = env.get_string(&player) else {
        return 0;
    };
    let player: String = player.into();
    let Ok(player) = uuid::Uuid::parse_str(&player) else {
        return 0;
    };
    let Some(handler) = PLAYER_HANDLER.get().and_then(Option::as_ref) else {
        return 0;
    };
    u8::from(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            handler(PlayerCommand::SetOperator {
                player,
                operator: operator != 0,
            })
        }))
        .is_ok(),
    )
}
extern "system" fn player_inventory(
    mut env: JNIEnv,
    _: JClass,
    player: JString,
    slot: i32,
    material: JString,
    amount: i32,
) -> u8 {
    if !(0..41).contains(&slot) || !(0..=255).contains(&amount) {
        return 0;
    }
    let Ok(player) = env.get_string(&player) else {
        return 0;
    };
    let player: String = player.into();
    let Ok(player) = uuid::Uuid::parse_str(&player) else {
        return 0;
    };
    let Ok(material) = env.get_string(&material) else {
        return 0;
    };
    let material: String = material.into();
    let item = if amount == 0 {
        None
    } else {
        Some(ItemSnapshot {
            material,
            amount: amount as u8,
        })
    };
    let Some(handler) = PLAYER_HANDLER.get().and_then(Option::as_ref) else {
        return 0;
    };
    u8::from(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            handler(PlayerCommand::SetInventorySlot {
                player,
                slot: slot as u8,
                item,
            })
        }))
        .is_ok(),
    )
}
extern "system" fn player_teleport(
    mut env: JNIEnv,
    _: JClass,
    player: JString,
    world: JString,
    x: f64,
    y: f64,
    z: f64,
    yaw: f32,
    pitch: f32,
) -> u8 {
    if !x.is_finite() || !y.is_finite() || !z.is_finite() || !yaw.is_finite() || !pitch.is_finite()
    {
        return 0;
    }
    let Ok(player) = env.get_string(&player) else {
        return 0;
    };
    let player: String = player.into();
    let Ok(player) = uuid::Uuid::parse_str(&player) else {
        return 0;
    };
    let Ok(world) = env.get_string(&world) else {
        return 0;
    };
    let world: String = world.into();
    let Ok(world) = uuid::Uuid::parse_str(&world) else {
        return 0;
    };
    let Some(handler) = PLAYER_HANDLER.get().and_then(Option::as_ref) else {
        return 0;
    };
    u8::from(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            handler(PlayerCommand::Teleport {
                player,
                location: PlayerLocation {
                    world,
                    x,
                    y,
                    z,
                    yaw,
                    pitch,
                },
            })
        }))
        .is_ok(),
    )
}
extern "system" fn player_state(
    mut env: JNIEnv,
    _: JClass,
    player: JString,
    kind: i32,
    value: f64,
) {
    let Ok(player) = env.get_string(&player) else {
        return;
    };
    let player: String = player.into();
    let Ok(player) = uuid::Uuid::parse_str(&player) else {
        return;
    };
    let command = match kind {
        0 if value.is_finite() && value >= 0.0 => PlayerCommand::SetHealth {
            player,
            health: value,
        },
        1 if value.fract() == 0.0 && (0.0..=20.0).contains(&value) => PlayerCommand::SetFood {
            player,
            food: value as u8,
        },
        2 => {
            let mode = match value as i32 {
                0 => GameMode::Survival,
                1 => GameMode::Creative,
                2 => GameMode::Adventure,
                3 => GameMode::Spectator,
                _ => return,
            };
            PlayerCommand::SetGameMode { player, mode }
        }
        _ => return,
    };
    if let Some(handler) = PLAYER_HANDLER.get().and_then(Option::as_ref) {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handler(command)));
    }
}
extern "system" fn dispatch_back(mut env: JNIEnv, _: JClass, sender: JString, line: JString) -> u8 {
    let Ok(sender) = env.get_string(&sender) else {
        return 0;
    };
    let sender: String = sender.into();
    let sender = if sender == "console" {
        CommandSender::Console
    } else if let Ok(id) = uuid::Uuid::parse_str(&sender) {
        CommandSender::Player(id)
    } else {
        return 0;
    };
    let Ok(line) = env.get_string(&line) else {
        return 0;
    };
    let line: String = line.into();
    COMMAND_HANDLER
        .get()
        .and_then(Option::as_ref)
        .map_or(0, |handler| {
            u8::from(
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handler(sender, &line)))
                    .unwrap_or(false),
            )
        })
}
extern "system" fn player_move(
    mut env: JNIEnv,
    _: JClass,
    player: JString,
    name: JString,
    from_x: f64,
    from_y: f64,
    from_z: f64,
    from_yaw: f32,
    from_pitch: f32,
    to_x: f64,
    to_y: f64,
    to_z: f64,
    to_yaw: f32,
    to_pitch: f32,
) -> u8 {
    if [from_x, from_y, from_z, to_x, to_y, to_z]
        .iter()
        .any(|v| !v.is_finite())
        || [from_yaw, from_pitch, to_yaw, to_pitch]
            .iter()
            .any(|v| !v.is_finite())
    {
        return 1;
    }
    let Ok(player) = env.get_string(&player) else {
        return 1;
    };
    let player: String = player.into();
    let Ok(player) = uuid::Uuid::parse_str(&player) else {
        return 1;
    };
    let Ok(name) = env.get_string(&name) else {
        return 1;
    };
    let name: String = name.into();
    let allowed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Option<bool> {
        let player = env.new_string(player.to_string()).ok()?;
        let name = env.new_string(name).ok()?;
        env.call_static_method(
            BOOTSTRAP,
            "firePlayerMove",
            "(Ljava/lang/String;Ljava/lang/String;DDDFFDDDFF)Z",
            &[
                JValue::Object(player.as_ref()),
                JValue::Object(name.as_ref()),
                JValue::Double(from_x),
                JValue::Double(from_y),
                JValue::Double(from_z),
                JValue::Float(from_yaw),
                JValue::Float(from_pitch),
                JValue::Double(to_x),
                JValue::Double(to_y),
                JValue::Double(to_z),
                JValue::Float(to_yaw),
                JValue::Float(to_pitch),
            ],
        )
        .ok()?
        .z()
        .ok()
    }));
    u8::from(matches!(allowed, Ok(Some(true))))
}
extern "system" fn schedule_global(_: JNIEnv, _: JClass, id: i64, delay: i64) {
    REGION_QUEUE
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .global(id, delay.max(0) as u64);
}
extern "system" fn current_tick(_: JNIEnv, _: JClass) -> i64 {
    REGION_QUEUE
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .current_tick()
        .min(i64::MAX as u64) as i64
}
extern "system" fn schedule_region(
    mut env: JNIEnv,
    _: JClass,
    id: i64,
    world: JString,
    x: i32,
    z: i32,
    delay: i64,
    plugin: JString,
) {
    let Ok(world) = env.get_string(&world) else {
        return;
    };
    let world: String = world.into();
    let Ok(plugin) = env.get_string(&plugin) else {
        return;
    };
    let plugin: String = plugin.into();
    let aware = MODES
        .get()
        .and_then(|m| m.get(&plugin.to_ascii_lowercase()))
        .copied()
        .unwrap_or(false);
    REGION_QUEUE
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .region(id, world, x, z, delay.max(0) as u64, aware);
}
extern "system" fn schedule_entity(
    mut env: JNIEnv,
    _: JClass,
    id: i64,
    entity: JString,
    delay: i64,
    plugin: JString,
) {
    let Ok(entity) = env.get_string(&entity) else {
        return;
    };
    let entity: String = entity.into();
    let Ok(plugin) = env.get_string(&plugin) else {
        return;
    };
    let plugin: String = plugin.into();
    let aware = MODES
        .get()
        .and_then(|m| m.get(&plugin.to_ascii_lowercase()))
        .copied()
        .unwrap_or(false);
    REGION_QUEUE
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .entity(id, entity, delay.max(0) as u64, aware);
}
extern "system" fn is_region_thread(
    mut env: JNIEnv,
    _: JClass,
    world: JString,
    x: i32,
    z: i32,
) -> u8 {
    let Ok(world) = env.get_string(&world) else {
        return 0;
    };
    let world: String = world.into();
    if let Some(context) = REGION_CONTEXT.get().and_then(Option::as_ref) {
        return uuid::Uuid::parse_str(&world).ok().map_or(0, |id| {
            u8::from(
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    context.is_owned_by_current_region(id, x, z)
                }))
                .unwrap_or(false),
            )
        });
    }
    REGION.with(|region| {
        u8::from(
            region
                .borrow()
                .as_ref()
                .is_some_and(|current| current == &(world, x, z)),
        )
    })
}
impl PluginHost {
    pub fn start(dir: &Path, cfg: PluginConfig) -> Result<Self, Error> {
        let plan = ferrite_plugin_meta::scan_directory(dir)?;
        if !plan.failed.is_empty() {
            return Err(Error::Metadata(format!("{:?}", plan.failed)));
        }
        let modes = plan
            .loaded()
            .map(|p| {
                (
                    p.name().to_ascii_lowercase(),
                    matches!(
                        p.threading_mode(),
                        ferrite_plugin_meta::ThreadingMode::RegionAware
                    ),
                )
            })
            .collect();
        let jvm = Jvm::start(JvmConfig {
            java_home: cfg.java_home,
            classpath: vec![cfg.bridge_jar],
            ..Default::default()
        })?;
        let _ = MODES.set(modes);
        let _ = COMMAND_HANDLER.set(cfg.command_handler);
        let _ = PLAYER_HANDLER.set(cfg.player_handler);
        let _ = WORLD_HANDLER.set(cfg.world_handler);
        let _ = SERVER_HANDLER.set(cfg.server_handler);
        let _ = REGION_CONTEXT.set(cfg.region_context.clone());
        jvm.register_natives(
            "dev/ferrite/server/Native$Bindings",
            &[
                NativeMethod::new("log", "(Ljava/lang/String;)V", log as *mut _),
                NativeMethod::new(
                    "eventRegistered",
                    "(Ljava/lang/String;)V",
                    event_registered as *mut _,
                ),
                NativeMethod::new("eventsCleared", "()V", events_cleared as *mut _),
                NativeMethod::new(
                    "serverBan",
                    "(Ljava/lang/String;Ljava/lang/String;JLjava/lang/String;Z)Z",
                    server_ban as *mut _,
                ),
                NativeMethod::new("serverWhitelist", "(Z)Z", server_whitelist as *mut _),
                NativeMethod::new(
                    "worldSetBlock",
                    "(Ljava/lang/String;IIILjava/lang/String;)Z",
                    world_set_block as *mut _,
                ),
                NativeMethod::new(
                    "playerSound",
                    "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;IDDDFFZJ)Z",
                    player_sound as *mut _,
                ),
                NativeMethod::new(
                    "playerVisual",
                    "(Ljava/lang/String;ILjava/lang/String;Ljava/lang/String;III)Z",
                    player_visual as *mut _,
                ),
                NativeMethod::new(
                    "playerOperator",
                    "(Ljava/lang/String;Z)Z",
                    player_operator as *mut _,
                ),
                NativeMethod::new(
                    "playerInventory",
                    "(Ljava/lang/String;ILjava/lang/String;I)Z",
                    player_inventory as *mut _,
                ),
                NativeMethod::new(
                    "playerTeleport",
                    "(Ljava/lang/String;Ljava/lang/String;DDDFF)Z",
                    player_teleport as *mut _,
                ),
                NativeMethod::new(
                    "playerState",
                    "(Ljava/lang/String;ID)V",
                    player_state as *mut _,
                ),
                NativeMethod::new(
                    "playerMessage",
                    "(Ljava/lang/String;Ljava/lang/String;Z)V",
                    player_message as *mut _,
                ),
                NativeMethod::new(
                    "dispatchCommand",
                    "(Ljava/lang/String;Ljava/lang/String;)Z",
                    dispatch_back as *mut _,
                ),
                NativeMethod::new(
                    "playerMove",
                    "(Ljava/lang/String;Ljava/lang/String;DDDFFDDDFF)Z",
                    player_move as *mut _,
                ),
                NativeMethod::new("scheduleGlobal", "(JJ)V", schedule_global as *mut _),
                NativeMethod::new("currentTick", "()J", current_tick as *mut _),
                NativeMethod::new(
                    "scheduleRegion",
                    "(JLjava/lang/String;IIJLjava/lang/String;)V",
                    schedule_region as *mut _,
                ),
                NativeMethod::new(
                    "scheduleEntity",
                    "(JLjava/lang/String;JLjava/lang/String;)V",
                    schedule_entity as *mut _,
                ),
                NativeMethod::new(
                    "isRegionThread",
                    "(Ljava/lang/String;II)Z",
                    is_region_thread as *mut _,
                ),
            ],
        )?;
        jvm.with_env(|env| {
            let motd = env.new_string(&cfg.server_snapshot.motd)?;
            env.call_static_method(
                BOOTSTRAP,
                "configure",
                "(Ljava/lang/String;IIIZZ)V",
                &[
                    JValue::Object(motd.as_ref()),
                    JValue::Int(i32::from(cfg.server_snapshot.max_players)),
                    JValue::Int(i32::from(cfg.server_snapshot.port)),
                    JValue::Int(i32::from(cfg.server_snapshot.view_distance)),
                    JValue::Bool(u8::from(cfg.server_snapshot.whitelist)),
                    JValue::Bool(u8::from(cfg.server_snapshot.online_mode)),
                ],
            )?;
            for address in &cfg.server_snapshot.banned_ips {
                let address = env.new_string(address.to_string())?;
                env.call_static_method(
                    BOOTSTRAP,
                    "configureIpBan",
                    "(Ljava/lang/String;)V",
                    &[JValue::Object(address.as_ref())],
                )?;
            }
            let paths: Vec<_> = plan.loaded().collect();
            let array =
                env.new_object_array(paths.len() as i32, "java/lang/String", JObject::null())?;
            for (i, entry) in paths.iter().enumerate() {
                let path = env.new_string(entry.source.to_string_lossy())?;
                env.set_object_array_element(&array, i as i32, path)?;
            }
            let modes = env.new_boolean_array(paths.len() as i32)?;
            let flags: Vec<u8> = paths
                .iter()
                .map(|p| {
                    u8::from(matches!(
                        p.threading_mode(),
                        ferrite_plugin_meta::ThreadingMode::RegionAware
                    ))
                })
                .collect();
            env.set_boolean_array_region(&modes, 0, &flags)?;
            env.call_static_method(
                BOOTSTRAP,
                "start",
                "([Ljava/lang/String;[Z)V",
                &[
                    JValue::Object(array.as_ref()),
                    JValue::Object(modes.as_ref()),
                ],
            )?;
            Ok(())
        })?;
        Ok(Self {
            jvm,
            issued_regions: Mutex::new(Vec::new()),
            issued_entities: Mutex::new(Vec::new()),
            region_context: cfg.region_context,
        })
    }
    /// Publish known world blocks; unprovided positions remain unavailable.
    pub fn register_world(&self, world: &WorldSnapshot) -> Result<(), Error> {
        self.jvm.with_env(|env| {
            let id = env.new_string(world.id.to_string())?;
            let name = env.new_string(&world.name)?;
            env.call_static_method(
                BOOTSTRAP,
                "registerWorld",
                "(Ljava/lang/String;Ljava/lang/String;JIII)V",
                &[
                    JValue::Object(id.as_ref()),
                    JValue::Object(name.as_ref()),
                    JValue::Long(world.time),
                    JValue::Int(world.spawn.x),
                    JValue::Int(world.spawn.y),
                    JValue::Int(world.spawn.z),
                ],
            )?;
            for height in &world.heights {
                env.call_static_method(
                    BOOTSTRAP,
                    "setHeightSnapshot",
                    "(Ljava/lang/String;III)V",
                    &[
                        JValue::Object(id.as_ref()),
                        JValue::Int(height.x),
                        JValue::Int(height.z),
                        JValue::Int(height.y),
                    ],
                )?;
            }
            for block in &world.blocks {
                let material = env.new_string(&block.material)?;
                env.call_static_method(
                    BOOTSTRAP,
                    "setBlockSnapshot",
                    "(Ljava/lang/String;IIILjava/lang/String;)V",
                    &[
                        JValue::Object(id.as_ref()),
                        JValue::Int(block.position.x),
                        JValue::Int(block.position.y),
                        JValue::Int(block.position.z),
                        JValue::Object(material.as_ref()),
                    ],
                )?;
            }
            Ok(())
        })?;
        Ok(())
    }
    /// Publish an online player snapshot for plugin lookups and command dispatch.
    pub fn register_player(&self, player: &PlayerSnapshot) -> Result<(), Error> {
        if !player.health.is_finite()
            || player.health < 0.0
            || player.food > 20
            || player.inventory.len() > 41
            || player
                .inventory
                .iter()
                .flatten()
                .any(|item| item.amount == 0)
            || !player.location.x.is_finite()
            || !player.location.y.is_finite()
            || !player.location.z.is_finite()
            || !player.location.yaw.is_finite()
            || !player.location.pitch.is_finite()
        {
            return Err(Error::InvalidPlayerSnapshot);
        }
        let mode = match player.mode {
            GameMode::Survival => 0,
            GameMode::Creative => 1,
            GameMode::Adventure => 2,
            GameMode::Spectator => 3,
        };
        self.jvm.with_env(|env| {
            let id = env.new_string(player.id.to_string())?;
            let name = env.new_string(&player.name)?;
            let world = env.new_string(player.location.world.to_string())?;
            env.call_static_method(
                BOOTSTRAP,
                "registerPlayer",
                "(Ljava/lang/String;Ljava/lang/String;ZDIILjava/lang/String;DDDFF)V",
                &[
                    JValue::Object(id.as_ref()),
                    JValue::Object(name.as_ref()),
                    JValue::Bool(u8::from(player.operator)),
                    JValue::Double(player.health),
                    JValue::Int(i32::from(player.food)),
                    JValue::Int(mode),
                    JValue::Object(world.as_ref()),
                    JValue::Double(player.location.x),
                    JValue::Double(player.location.y),
                    JValue::Double(player.location.z),
                    JValue::Float(player.location.yaw),
                    JValue::Float(player.location.pitch),
                ],
            )?;
            for (slot, item) in player.inventory.iter().enumerate() {
                if let Some(item) = item {
                    let material = env.new_string(&item.material)?;
                    env.call_static_method(
                        BOOTSTRAP,
                        "setInventorySnapshot",
                        "(Ljava/lang/String;ILjava/lang/String;I)V",
                        &[
                            JValue::Object(id.as_ref()),
                            JValue::Int(slot as i32),
                            JValue::Object(material.as_ref()),
                            JValue::Int(i32::from(item.amount)),
                        ],
                    )?;
                }
            }
            Ok(())
        })?;
        Ok(())
    }
    /// Publish a known offline player for name and operator lookups.
    pub fn register_offline_player(&self, player: &OfflinePlayerSnapshot) -> Result<(), Error> {
        if player.name.as_deref().is_some_and(str::is_empty) {
            return Err(Error::InvalidOfflinePlayerSnapshot);
        }
        self.jvm.with_env(|env| {
            let id = env.new_string(player.id.to_string())?;
            let name = match &player.name {
                Some(name) => env.new_string(name)?,
                None => JString::from(JObject::null()),
            };
            env.call_static_method(
                BOOTSTRAP,
                "registerOfflinePlayer",
                "(Ljava/lang/String;Ljava/lang/String;Z)V",
                &[
                    JValue::Object(id.as_ref()),
                    JValue::Object(name.as_ref()),
                    JValue::Bool(u8::from(player.operator)),
                ],
            )?;
            Ok(())
        })?;
        Ok(())
    }
    /// Return whether a plugin completed enabling and remains enabled.
    pub fn is_plugin_enabled(&self, name: &str) -> Result<bool, Error> {
        Ok(self.jvm.with_env(|env| {
            let name = env.new_string(name)?;
            env.call_static_method(
                BOOTSTRAP,
                "isPluginEnabled",
                "(Ljava/lang/String;)Z",
                &[JValue::Object(name.as_ref())],
            )?
            .z()
        })?)
    }
    pub fn logs(&self) -> Vec<String> {
        LOGS.get_or_init(Default::default)
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
    /// Take accumulated plugin log messages for delivery by the server.
    pub fn drain_logs(&self) -> Vec<String> {
        std::mem::take(
            &mut *LOGS
                .get_or_init(Default::default)
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }
    pub fn dispatch_command(&self, sender: CommandSender, line: &str) -> Result<bool, Error> {
        let sender = match sender {
            CommandSender::Console => "console".to_owned(),
            CommandSender::Player(id) => id.to_string(),
        };
        Ok(self.jvm.with_env(|env| {
            let sender = env.new_string(sender)?;
            let line = env.new_string(line)?;
            env.call_static_method(
                BOOTSTRAP,
                "dispatchCommand",
                "(Ljava/lang/String;Ljava/lang/String;)Z",
                &[
                    JValue::Object(sender.as_ref()),
                    JValue::Object(line.as_ref()),
                ],
            )?
            .z()
        })?)
    }
    pub fn tick(&self) -> Result<(), Error> {
        self.jvm.call_static::<()>(BOOTSTRAP, "tick", "()V", &[])?;
        let (ready, globals, entities) = REGION_QUEUE
            .get_or_init(Default::default)
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .advance();
        READY
            .get_or_init(Default::default)
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .extend(ready);
        READY_ENTITIES
            .get_or_init(Default::default)
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .extend(entities);
        for id in globals {
            self.jvm
                .call_static::<()>(BOOTSTRAP, "runGlobalTask", "(J)V", &[JValue::Long(id)])?;
        }
        Ok(())
    }
    pub fn take_region_tasks(&self) -> Vec<RegionTask> {
        let tasks = std::mem::take(
            &mut *READY
                .get_or_init(Default::default)
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        self.issued_regions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .extend(tasks.iter().cloned());
        tasks
    }
    pub fn take_entity_tasks(&self) -> Vec<EntityTask> {
        let tasks = std::mem::take(
            &mut *READY_ENTITIES
                .get_or_init(Default::default)
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        self.issued_entities
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .extend(tasks.iter().cloned());
        tasks
    }
    /// Transfer ready work to the server's region and entity schedulers.
    pub fn route_ready_tasks(&self) -> Result<(), Error> {
        let context = self
            .region_context
            .as_ref()
            .ok_or(Error::MissingRegionContext)?;
        for task in self.take_region_tasks() {
            context.schedule_on_region(task);
        }
        for task in self.take_entity_tasks() {
            context.schedule_on_entity(task);
        }
        Ok(())
    }
    pub fn run_entity_task(&self, task: EntityTask) -> Result<(), Error> {
        consume_task(&self.issued_entities, &task)?;
        Ok(self
            .jvm
            .call_static(BOOTSTRAP, "runEntityTask", "(J)V", &[JValue::Long(task.id)])?)
    }
    pub fn run_region_task(&self, task: RegionTask) -> Result<(), Error> {
        consume_task(&self.issued_regions, &task)?;
        let previous = REGION
            .with(|region| region.replace(Some((task.world.clone(), task.chunk_x, task.chunk_z))));
        let result =
            self.jvm
                .call_static(BOOTSTRAP, "runRegionTask", "(J)V", &[JValue::Long(task.id)]);
        REGION.with(|region| *region.borrow_mut() = previous);
        Ok(result?)
    }
    /// Commands declared by enabled plugins in their descriptor.
    pub fn declared_commands(&self) -> Result<Vec<PluginCommand>, Error> {
        let encoded: String = self.jvm.with_env(|env| {
            let value = env
                .call_static_method(BOOTSTRAP, "pluginCommands", "()Ljava/lang/String;", &[])?
                .l()?;
            if value.is_null() {
                return Ok(String::new());
            }
            let value = JString::from(value);
            let value: String = env.get_string(&value)?.into();
            Ok(value)
        })?;
        Ok(encoded
            .lines()
            .filter_map(|line| {
                let mut fields = line.split('\t');
                let name = fields.next().filter(|name| !name.is_empty())?;
                let aliases = fields
                    .next()
                    .unwrap_or_default()
                    .split(',')
                    .filter(|alias| !alias.is_empty())
                    .map(str::to_owned)
                    .collect();
                let permission = fields
                    .next()
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned);
                let usage = fields
                    .next()
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned);
                Some(PluginCommand {
                    name: name.to_owned(),
                    aliases,
                    permission,
                    usage,
                })
            })
            .collect())
    }

    /// Executes a plugin command through its Bukkit `CommandExecutor`.
    ///
    /// Messages the plugin sends with `CommandSender.sendMessage` on this thread are captured
    /// and returned so the game loop can deliver them to the player.
    pub fn execute_plugin_command(
        &self,
        sender: CommandSender,
        line: &str,
    ) -> Result<PluginCommandResult, Error> {
        let line = line.trim().trim_start_matches('/');
        let mut parts = line.split_whitespace();
        let Some(label) = parts.next() else {
            return Ok(PluginCommandResult::default());
        };
        let label = label.to_owned();
        let args: Vec<String> = parts.map(str::to_owned).collect();
        let sender = match sender {
            CommandSender::Console => "console".to_owned(),
            CommandSender::Player(id) => id.to_string(),
        };
        let previous = CAPTURED_MESSAGES.with(|captured| captured.replace(Some(Vec::new())));
        let result = self.jvm.with_env(|env| {
            let sender = env.new_string(sender)?;
            let label = env.new_string(&label)?;
            let array =
                env.new_object_array(args.len() as i32, "java/lang/String", JObject::null())?;
            for (index, arg) in args.iter().enumerate() {
                let arg = env.new_string(arg)?;
                env.set_object_array_element(&array, index as i32, arg)?;
            }
            let status = env.call_static_method(
                BOOTSTRAP,
                "runPluginCommand",
                "(Ljava/lang/String;Ljava/lang/String;[Ljava/lang/String;)I",
                &[
                    JValue::Object(sender.as_ref()),
                    JValue::Object(label.as_ref()),
                    JValue::Object(array.as_ref()),
                ],
            )?;
            status.i()
        });
        let messages = CAPTURED_MESSAGES
            .with(|captured| captured.replace(previous))
            .unwrap_or_default();
        Ok(PluginCommandResult {
            handled: result? == 1,
            messages,
        })
    }

    /// Whether at least one plugin listener is registered for `event_class`.
    pub fn listens_for(&self, event_class: &str) -> bool {
        listened_events()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains(event_class)
    }

    /// Fires Bukkit's `PlayerJoinEvent`; no JNI call without a registered listener.
    pub fn fire_player_join(&self, id: uuid::Uuid, name: &str) -> Result<(), Error> {
        if !self.listens_for(PLAYER_JOIN_EVENT) {
            return Ok(());
        }
        self.jvm.with_env(|env| {
            let id = env.new_string(id.to_string())?;
            let name = env.new_string(name)?;
            env.call_static_method(
                BOOTSTRAP,
                "firePlayerJoin",
                "(Ljava/lang/String;Ljava/lang/String;)V",
                &[JValue::Object(id.as_ref()), JValue::Object(name.as_ref())],
            )?;
            Ok(())
        })?;
        Ok(())
    }

    /// Fires Bukkit's `PlayerQuitEvent`; no JNI call without a registered listener.
    pub fn fire_player_quit(&self, id: uuid::Uuid) -> Result<(), Error> {
        if !self.listens_for(PLAYER_QUIT_EVENT) {
            return Ok(());
        }
        self.jvm.with_env(|env| {
            let id = env.new_string(id.to_string())?;
            env.call_static_method(
                BOOTSTRAP,
                "firePlayerQuit",
                "(Ljava/lang/String;)V",
                &[JValue::Object(id.as_ref())],
            )?;
            Ok(())
        })?;
        Ok(())
    }

    /// Fires Bukkit's `PlayerMoveEvent`; no JNI call without a registered listener.
    pub fn fire_player_move(
        &self,
        id: uuid::Uuid,
        name: &str,
        movement: &PlayerMove,
    ) -> Result<bool, Error> {
        if !self.listens_for(PLAYER_MOVE_EVENT) {
            return Ok(true);
        }
        Ok(self.jvm.with_env(|env| {
            let id = env.new_string(id.to_string())?;
            let name = env.new_string(name)?;
            let allowed = env
                .call_static_method(
                    BOOTSTRAP,
                    "firePlayerMove",
                    "(Ljava/lang/String;Ljava/lang/String;DDDFFDDDFF)Z",
                    &[
                        JValue::Object(id.as_ref()),
                        JValue::Object(name.as_ref()),
                        JValue::Double(movement.from[0]),
                        JValue::Double(movement.from[1]),
                        JValue::Double(movement.from[2]),
                        JValue::Float(movement.from_yaw),
                        JValue::Float(movement.from_pitch),
                        JValue::Double(movement.to[0]),
                        JValue::Double(movement.to[1]),
                        JValue::Double(movement.to[2]),
                        JValue::Float(movement.to_yaw),
                        JValue::Float(movement.to_pitch),
                    ],
                )?
                .z()?;
            Ok(allowed)
        })?)
    }

    /// Fires Paper's `AsyncChatEvent` and Bukkit's `AsyncPlayerChatEvent`.
    ///
    /// Returns the message after listeners ran, or `None` when a listener cancelled chat.
    pub fn fire_chat(
        &self,
        id: uuid::Uuid,
        name: &str,
        message: &str,
    ) -> Result<Option<String>, Error> {
        if !self.listens_for(ASYNC_CHAT_EVENT) && !self.listens_for(ASYNC_PLAYER_CHAT_EVENT) {
            return Ok(Some(message.to_owned()));
        }
        Ok(self.jvm.with_env(|env| {
            let id = env.new_string(id.to_string())?;
            let name = env.new_string(name)?;
            let message = env.new_string(message)?;
            let result = env
                .call_static_method(
                    BOOTSTRAP,
                    "fireChat",
                    "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
                    &[
                        JValue::Object(id.as_ref()),
                        JValue::Object(name.as_ref()),
                        JValue::Object(message.as_ref()),
                    ],
                )?
                .l()?;
            if result.is_null() {
                return Ok(None);
            }
            let result = JString::from(result);
            let result: String = env.get_string(&result)?.into();
            Ok(Some(result))
        })?)
    }
    pub fn shutdown(&self) -> Result<(), Error> {
        self.issued_regions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        self.issued_entities
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        Ok(self.jvm.call_static(BOOTSTRAP, "shutdown", "()V", &[])?)
    }
}
impl ServerHooks for PluginHost {
    fn player_joined(&self, id: uuid::Uuid, name: &str) -> Result<(), Error> {
        self.fire_player_join(id, name)
    }
    fn player_left(&self, id: uuid::Uuid, _name: &str) -> Result<(), Error> {
        self.fire_player_quit(id)
    }
    fn chat(&self, id: uuid::Uuid, name: &str, message: &str) -> Result<Option<String>, Error> {
        self.fire_chat(id, name, message)
    }
    fn player_moved(
        &self,
        id: uuid::Uuid,
        name: &str,
        movement: &PlayerMove,
    ) -> Result<bool, Error> {
        self.fire_player_move(id, name, movement)
    }
    fn plugin_commands(&self) -> Result<Vec<PluginCommand>, Error> {
        self.declared_commands()
    }
    fn run_plugin_command(
        &self,
        sender: CommandSender,
        line: &str,
    ) -> Result<PluginCommandResult, Error> {
        self.execute_plugin_command(sender, line)
    }
}
fn consume_task<T: PartialEq>(issued: &Mutex<Vec<T>>, task: &T) -> Result<(), Error> {
    let mut issued = issued
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let index = issued
        .iter()
        .position(|candidate| candidate == task)
        .ok_or(Error::InvalidTask)?;
    issued.swap_remove(index);
    Ok(())
}
mod scheduling;
