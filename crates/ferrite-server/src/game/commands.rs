use ferrite_command::{
    ArgumentType, CommandContext, CommandDispatcher, CommandError, CommandSyntaxError, argument,
    arguments::{EntityTarget, GameMode, SelectorKind},
    literal,
};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::UnboundedSender;
use uuid::Uuid;

/// The overworld clock's registry name, used by `/time` feedback.
pub const OVERWORLD_CLOCK: &str = "minecraft:overworld";
const SERVER_NAME: &str = "Server";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Weather {
    Clear,
    Rain,
    Thunder,
}

impl Weather {
    fn feedback_key(self) -> &'static str {
        match self {
            Self::Clear => "commands.weather.set.clear",
            Self::Rain => "commands.weather.set.rain",
            Self::Thunder => "commands.weather.set.thunder",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerEntry {
    pub uuid: Uuid,
    pub name: String,
    pub game_mode: i32,
}

/// Mutable world state needed by commands. Commands run on the game loop, so a short-lived
/// mutex keeps `/list`, `/gamemode <mode> <target>`, and the clocks consistent with joins,
/// leaves, and each other.
#[derive(Debug, Clone)]
pub struct GameState {
    pub default_gamemode: i32,
    pub difficulty: i32,
    /// Ticks of the overworld clock (`/time set` and `/time add`).
    pub clock_time: i64,
    /// Total world ticks (`/time query gametime`).
    pub game_time: i64,
    pub weather: Weather,
    pub weather_duration: i32,
    pub seed: i64,
    pub max_players: i32,
    pub players: Vec<PlayerEntry>,
}

impl GameState {
    pub fn new(default_gamemode: i32, seed: i64, max_players: i32) -> Self {
        Self {
            default_gamemode,
            difficulty: 2,
            clock_time: 0,
            game_time: 0,
            weather: Weather::Clear,
            weather_duration: 6_000,
            seed,
            max_players,
            players: Vec::new(),
        }
    }

    pub fn join(&mut self, uuid: Uuid, name: String, game_mode: i32, max_players: i32) {
        self.leave(uuid);
        self.players.push(PlayerEntry {
            uuid,
            name,
            game_mode,
        });
        self.max_players = max_players;
    }

    pub fn leave(&mut self, uuid: Uuid) {
        self.players.retain(|player| player.uuid != uuid);
    }

    pub fn tick(&mut self) {
        self.clock_time += 1;
        self.game_time += 1;
    }
}

impl Default for GameState {
    fn default() -> Self {
        Self::new(1, 0, 20)
    }
}

/// A vanilla translation key, its arguments, and the English rendering used by the
/// current plain-text chat packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Feedback {
    pub key: &'static str,
    pub args: Vec<String>,
    pub text: String,
}

impl Feedback {
    pub fn new(key: &'static str, args: Vec<String>) -> Self {
        Self {
            key,
            text: render(english(key), &args),
            args,
        }
    }
}

fn english(key: &str) -> &'static str {
    match key {
        "commands.gamemode.success.self" => "Set own game mode to %s",
        "commands.gamemode.success.other" => "Set %s's game mode to %s",
        "gameMode.changed" => "Your game mode has been updated to %s",
        "commands.defaultgamemode.success" => "The default game mode is now %s",
        "commands.difficulty.success" => "The difficulty has been set to %s",
        "commands.difficulty.query" => "The difficulty is %s",
        "commands.time.set.absolute" => "Set %s to %s tick(s)",
        "commands.time.query.absolute" => "Clock %s is at %s tick(s)",
        "commands.time.query.gametime" => "The game time is %s tick(s)",
        "commands.weather.set.clear" => "Set the weather to clear",
        "commands.weather.set.rain" => "Set the weather to rain",
        "commands.weather.set.thunder" => "Set the weather to rain and thunder",
        "commands.seed.success" => "Seed: %s",
        "commands.list.players" => "There are %s of a max of %s players online: %s",
        "chat.type.announcement" => "[%s] %s",
        "chat.type.emote" => "* %s %s",
        _ => "",
    }
}

fn render(template: &str, args: &[String]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut args = args.iter();
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.peek() {
            Some('%') => {
                chars.next();
                out.push('%');
            }
            Some('s') => {
                chars.next();
                if let Some(value) = args.next() {
                    out.push_str(value);
                }
            }
            _ => out.push('%'),
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    GameMode {
        mode: i32,
        targets: Vec<Uuid>,
    },
    DefaultGameMode(i32),
    Difficulty(i32),
    TimeSet(i32),
    TimeAdd(i32),
    Weather {
        kind: Weather,
        duration: Option<i32>,
    },
    /// Feedback for the command source.
    Feedback(Feedback),
    /// Feedback for one player (for example `gameMode.changed`).
    PlayerFeedback {
        uuid: Uuid,
        feedback: Feedback,
    },
    /// Chat visible to every player (`say`, `me`).
    Broadcast(Feedback),
    Teleport([f64; 3]),
    Tps,
    Stop,
}

#[derive(Clone)]
pub struct Source {
    pub permission: u8,
    pub console: bool,
    pub uuid: Option<Uuid>,
    pub name: Option<String>,
    pub state: Arc<Mutex<GameState>>,
    pub actions: UnboundedSender<Action>,
}

impl Source {
    fn sender_name(&self) -> String {
        self.name.clone().unwrap_or_else(|| SERVER_NAME.into())
    }
}

fn send(source: &Source, action: Action) -> Result<i32, CommandError> {
    source
        .actions
        .send(action)
        .map_err(|_| CommandError::failed("command target unavailable"))?;
    Ok(1)
}

fn emit(source: &Source, action: Action) -> Result<(), CommandError> {
    send(source, action).map(|_| ())
}

fn syntax_error(key: &'static str, args: Vec<String>, message: impl Into<String>) -> CommandError {
    CommandError::Syntax(CommandSyntaxError::new(key, args, message.into()))
}

fn player_only() -> CommandError {
    syntax_error(
        "permissions.requires.player",
        Vec::new(),
        "A player is required to run this command here",
    )
}

fn player_not_found() -> CommandError {
    syntax_error(
        "argument.entity.notfound.player",
        Vec::new(),
        "No player was found",
    )
}

fn game_mode_id(mode: GameMode) -> i32 {
    match mode {
        GameMode::Survival => 0,
        GameMode::Creative => 1,
        GameMode::Adventure => 2,
        GameMode::Spectator => 3,
    }
}

fn game_mode_display(mode: GameMode) -> &'static str {
    match mode {
        GameMode::Survival => "Survival Mode",
        GameMode::Creative => "Creative Mode",
        GameMode::Adventure => "Adventure Mode",
        GameMode::Spectator => "Spectator Mode",
    }
}

fn difficulty_display(value: i32) -> &'static str {
    match value {
        0 => "Peaceful",
        1 => "Easy",
        2 => "Normal",
        3 => "Hard",
        _ => "Normal",
    }
}

fn resolve_players(
    players: &[PlayerEntry],
    target: &EntityTarget,
    source: Option<Uuid>,
) -> Result<Vec<Uuid>, CommandError> {
    let selected: Vec<&PlayerEntry> = match target {
        EntityTarget::Name(name) => players
            .iter()
            .filter(|player| player.name.eq_ignore_ascii_case(name))
            .collect(),
        EntityTarget::Uuid(id) => {
            let Ok(id) = Uuid::parse_str(id) else {
                return Err(player_not_found());
            };
            players.iter().filter(|player| player.uuid == id).collect()
        }
        EntityTarget::Selector { kind, options } => {
            let mut selected: Vec<&PlayerEntry> = match kind {
                SelectorKind::Myself => source
                    .and_then(|uuid| players.iter().find(|player| player.uuid == uuid))
                    .into_iter()
                    .collect(),
                SelectorKind::NearestPlayer | SelectorKind::NearestEntity => {
                    players.iter().take(1).collect()
                }
                SelectorKind::RandomPlayer => players.iter().take(1).collect(),
                SelectorKind::AllPlayers | SelectorKind::AllEntities => players.iter().collect(),
            };
            for option in options {
                match option.key.as_str() {
                    "name" => selected.retain(|player| player.name == option.value),
                    "limit" => {
                        if let Ok(limit) = option.value.parse::<usize>() {
                            selected.truncate(limit.max(1));
                        }
                    }
                    "gamemode" => {
                        if let Some(mode) = GameMode::from_name(&option.value) {
                            selected.retain(|player| player.game_mode == game_mode_id(mode));
                        }
                    }
                    _ => {}
                }
            }
            selected
        }
    };
    if selected.is_empty() {
        return Err(player_not_found());
    }
    Ok(selected.into_iter().map(|player| player.uuid).collect())
}

fn gamemode(ctx: &CommandContext<Source>) -> Result<i32, CommandError> {
    let mode = ctx
        .argument("gamemode")
        .and_then(|value| value.as_gamemode())
        .ok_or_else(|| CommandError::failed("missing gamemode"))?;
    let target = ctx.argument("target").and_then(|value| value.as_entity());
    let source = ctx.source();
    let mode_id = game_mode_id(mode);
    let mut state = source.state.lock().unwrap();
    let targets = match target {
        Some(target) => resolve_players(&state.players, target, source.uuid)?,
        None => vec![source.uuid.ok_or_else(player_only)?],
    };
    let mut changed = Vec::new();
    let mut feedback = Vec::new();
    for uuid in targets {
        let Some(index) = state.players.iter().position(|player| player.uuid == uuid) else {
            continue;
        };
        if state.players[index].game_mode == mode_id {
            continue;
        }
        state.players[index].game_mode = mode_id;
        changed.push(uuid);
        if Some(uuid) == source.uuid {
            feedback.push(Action::Feedback(Feedback::new(
                "commands.gamemode.success.self",
                vec![game_mode_display(mode).into()],
            )));
        } else {
            feedback.push(Action::PlayerFeedback {
                uuid,
                feedback: Feedback::new("gameMode.changed", vec![game_mode_display(mode).into()]),
            });
            feedback.push(Action::Feedback(Feedback::new(
                "commands.gamemode.success.other",
                vec![
                    state.players[index].name.clone(),
                    game_mode_display(mode).into(),
                ],
            )));
        }
    }
    drop(state);
    if changed.is_empty() {
        return Ok(0);
    }
    emit(
        source,
        Action::GameMode {
            mode: mode_id,
            targets: changed.clone(),
        },
    )?;
    for action in feedback {
        emit(source, action)?;
    }
    Ok(changed.len() as i32)
}

fn default_gamemode(ctx: &CommandContext<Source>) -> Result<i32, CommandError> {
    let mode = ctx
        .argument("gamemode")
        .and_then(|value| value.as_gamemode())
        .ok_or_else(|| CommandError::failed("missing gamemode"))?;
    let source = ctx.source();
    source.state.lock().unwrap().default_gamemode = game_mode_id(mode);
    emit(source, Action::DefaultGameMode(game_mode_id(mode)))?;
    emit(
        source,
        Action::Feedback(Feedback::new(
            "commands.defaultgamemode.success",
            vec![game_mode_display(mode).into()],
        )),
    )?;
    Ok(1)
}

fn difficulty_query(ctx: &CommandContext<Source>) -> Result<i32, CommandError> {
    let source = ctx.source();
    let value = source.state.lock().unwrap().difficulty;
    emit(
        source,
        Action::Feedback(Feedback::new(
            "commands.difficulty.query",
            vec![difficulty_display(value).into()],
        )),
    )?;
    Ok(value)
}

fn set_difficulty(ctx: &CommandContext<Source>, value: i32) -> Result<i32, CommandError> {
    let source = ctx.source();
    {
        let mut state = source.state.lock().unwrap();
        if state.difficulty == value {
            let display = difficulty_display(value);
            return Err(syntax_error(
                "commands.difficulty.failure",
                vec![display.into()],
                format!("The difficulty did not change; it is already set to {display}"),
            ));
        }
        state.difficulty = value;
    }
    emit(source, Action::Difficulty(value))?;
    emit(
        source,
        Action::Feedback(Feedback::new(
            "commands.difficulty.success",
            vec![difficulty_display(value).into()],
        )),
    )?;
    Ok(0)
}

fn time_set(ctx: &CommandContext<Source>) -> Result<i32, CommandError> {
    let ticks = ctx
        .integer("time")
        .ok_or_else(|| CommandError::failed("missing time"))?;
    let source = ctx.source();
    {
        let mut state = source.state.lock().unwrap();
        if state.clock_time == i64::from(ticks) {
            return Err(syntax_error(
                "commands.time.set.already_at_time",
                vec![OVERWORLD_CLOCK.into(), ticks.to_string()],
                format!("Clock {OVERWORLD_CLOCK} is already set to {ticks} tick(s)"),
            ));
        }
        state.clock_time = i64::from(ticks);
    }
    emit(source, Action::TimeSet(ticks))?;
    emit(
        source,
        Action::Feedback(Feedback::new(
            "commands.time.set.absolute",
            vec![OVERWORLD_CLOCK.into(), ticks.to_string()],
        )),
    )?;
    Ok(ticks)
}

fn time_add(ctx: &CommandContext<Source>) -> Result<i32, CommandError> {
    let delta = ctx
        .integer("time")
        .ok_or_else(|| CommandError::failed("missing time"))?;
    let source = ctx.source();
    let total = {
        let mut state = source.state.lock().unwrap();
        state.clock_time = state.clock_time.saturating_add(i64::from(delta));
        state.clock_time
    };
    emit(source, Action::TimeAdd(delta))?;
    emit(
        source,
        Action::Feedback(Feedback::new(
            "commands.time.set.absolute",
            vec![OVERWORLD_CLOCK.into(), total.to_string()],
        )),
    )?;
    Ok(total as i32)
}

fn time_query(ctx: &CommandContext<Source>, game_time: bool) -> Result<i32, CommandError> {
    let source = ctx.source();
    let ticks = {
        let state = source.state.lock().unwrap();
        if game_time {
            state.game_time
        } else {
            state.clock_time
        }
    };
    let feedback = if game_time {
        Feedback::new("commands.time.query.gametime", vec![ticks.to_string()])
    } else {
        Feedback::new(
            "commands.time.query.absolute",
            vec![OVERWORLD_CLOCK.into(), ticks.to_string()],
        )
    };
    emit(source, Action::Feedback(feedback))?;
    Ok(ticks as i32)
}

fn set_weather(ctx: &CommandContext<Source>, kind: Weather) -> Result<i32, CommandError> {
    let duration = ctx.integer("duration");
    let source = ctx.source();
    {
        let mut state = source.state.lock().unwrap();
        state.weather = kind;
        if let Some(duration) = duration {
            state.weather_duration = duration;
        }
    }
    emit(source, Action::Weather { kind, duration })?;
    emit(
        source,
        Action::Feedback(Feedback::new(kind.feedback_key(), Vec::new())),
    )?;
    Ok(duration.unwrap_or(0))
}

fn seed(ctx: &CommandContext<Source>) -> Result<i32, CommandError> {
    let source = ctx.source();
    let seed = source.state.lock().unwrap().seed;
    emit(
        source,
        Action::Feedback(Feedback::new(
            "commands.seed.success",
            vec![seed.to_string()],
        )),
    )?;
    Ok(seed as i32)
}

fn list_players(ctx: &CommandContext<Source>, uuids: bool) -> Result<i32, CommandError> {
    let source = ctx.source();
    let (count, max_players, list) = {
        let state = source.state.lock().unwrap();
        let names: Vec<String> = state
            .players
            .iter()
            .map(|player| {
                if uuids {
                    format!("{} ({})", player.name, player.uuid)
                } else {
                    player.name.clone()
                }
            })
            .collect();
        (state.players.len(), state.max_players, names.join(", "))
    };
    emit(
        source,
        Action::Feedback(Feedback::new(
            "commands.list.players",
            vec![count.to_string(), max_players.to_string(), list],
        )),
    )?;
    Ok(count as i32)
}

fn say(ctx: &CommandContext<Source>) -> Result<i32, CommandError> {
    let message = ctx.string("message").unwrap_or_default().to_owned();
    let sender = ctx.source().sender_name();
    emit(
        ctx.source(),
        Action::Broadcast(Feedback::new(
            "chat.type.announcement",
            vec![sender, message],
        )),
    )?;
    Ok(1)
}

fn me(ctx: &CommandContext<Source>) -> Result<i32, CommandError> {
    let action = ctx.string("action").unwrap_or_default().to_owned();
    let sender = ctx.source().sender_name();
    emit(
        ctx.source(),
        Action::Broadcast(Feedback::new("chat.type.emote", vec![sender, action])),
    )?;
    Ok(1)
}

pub fn dispatcher() -> CommandDispatcher<Source> {
    let mut tree = CommandDispatcher::new();
    tree.register(
        literal::<Source>("gamemode")
            .requires(|source| source.permission >= 2)
            .then(
                argument("gamemode", ArgumentType::Gamemode)
                    .executes(gamemode)
                    .then(argument("target", ArgumentType::players()).executes(gamemode)),
            ),
    );
    tree.register(
        literal::<Source>("defaultgamemode")
            .requires(|source| source.permission >= 2)
            .then(argument("gamemode", ArgumentType::Gamemode).executes(default_gamemode)),
    );
    tree.register(
        literal::<Source>("difficulty")
            .requires(|source| source.permission >= 2)
            .executes(difficulty_query)
            .then(literal("peaceful").executes(|ctx| set_difficulty(ctx, 0)))
            .then(literal("easy").executes(|ctx| set_difficulty(ctx, 1)))
            .then(literal("normal").executes(|ctx| set_difficulty(ctx, 2)))
            .then(literal("hard").executes(|ctx| set_difficulty(ctx, 3))),
    );
    tree.register(
        literal::<Source>("time")
            .requires(|source| source.permission >= 2)
            .then(
                literal("set")
                    .then(argument("time", ArgumentType::Time { min: 0 }).executes(time_set)),
            )
            .then(
                literal("add").then(
                    argument("time", ArgumentType::Time { min: i32::MIN }).executes(time_add),
                ),
            )
            .then(
                literal("query")
                    .then(literal("time").executes(|ctx| time_query(ctx, false)))
                    .then(literal("gametime").executes(|ctx| time_query(ctx, true))),
            ),
    );
    tree.register(
        literal::<Source>("weather")
            .requires(|source| source.permission >= 2)
            .then(
                literal("clear")
                    .executes(|ctx| set_weather(ctx, Weather::Clear))
                    .then(
                        argument("duration", ArgumentType::Time { min: 1 })
                            .executes(|ctx| set_weather(ctx, Weather::Clear)),
                    ),
            )
            .then(
                literal("rain")
                    .executes(|ctx| set_weather(ctx, Weather::Rain))
                    .then(
                        argument("duration", ArgumentType::Time { min: 1 })
                            .executes(|ctx| set_weather(ctx, Weather::Rain)),
                    ),
            )
            .then(
                literal("thunder")
                    .executes(|ctx| set_weather(ctx, Weather::Thunder))
                    .then(
                        argument("duration", ArgumentType::Time { min: 1 })
                            .executes(|ctx| set_weather(ctx, Weather::Thunder)),
                    ),
            ),
    );
    tree.register(
        literal::<Source>("seed")
            .requires(|source| source.permission >= 2)
            .executes(seed),
    );
    tree.register(
        literal::<Source>("list")
            .executes(|ctx| list_players(ctx, false))
            .then(literal("uuids").executes(|ctx| list_players(ctx, true))),
    );
    tree.register(
        literal::<Source>("say")
            .requires(|source| source.permission >= 2)
            .then(argument("message", ArgumentType::Message).executes(say)),
    );
    tree.register(
        literal::<Source>("me").then(argument("action", ArgumentType::Message).executes(me)),
    );
    tree.register(
        literal::<Source>("tp")
            .requires(|source| source.permission >= 2)
            .then(argument("x", ArgumentType::double()).then(
                argument("y", ArgumentType::double()).then(
                    argument("z", ArgumentType::double()).executes(|ctx| {
                        let coordinate = |name| {
                            ctx.argument(name)
                                .and_then(|v| v.as_f64())
                                .ok_or_else(|| CommandError::failed("missing coordinate"))
                        };
                        send(
                            ctx.source(),
                            Action::Teleport([
                                coordinate("x")?,
                                coordinate("y")?,
                                coordinate("z")?,
                            ]),
                        )
                    }),
                ),
            )),
    );
    tree.register(literal::<Source>("tps").executes(|ctx| send(ctx.source(), Action::Tps)));
    tree.register(
        literal::<Source>("stop")
            .requires(|source| source.console && source.permission >= 4)
            .executes(|ctx| send(ctx.source(), Action::Stop)),
    );
    tree
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

    const ALICE: u128 = 1;
    const BOB: u128 = 2;

    fn state_with(players: &[(u128, &str, i32)]) -> Arc<Mutex<GameState>> {
        let mut state = GameState::default();
        for &(uuid, name, mode) in players {
            state.join(Uuid::from_u128(uuid), name.into(), mode, 20);
        }
        Arc::new(Mutex::new(state))
    }

    fn source(
        permission: u8,
        player: Option<(u128, &str)>,
        state: Arc<Mutex<GameState>>,
    ) -> (Source, UnboundedReceiver<Action>) {
        let (actions, received) = unbounded_channel();
        let (uuid, name) = match player {
            Some((uuid, name)) => (Some(Uuid::from_u128(uuid)), Some(name.to_owned())),
            None => (None, None),
        };
        (
            Source {
                permission,
                console: player.is_none(),
                uuid,
                name,
                state,
                actions,
            },
            received,
        )
    }

    fn next_feedback(received: &mut UnboundedReceiver<Action>) -> Feedback {
        loop {
            if let Action::Feedback(feedback) = received.try_recv().expect("expected an action") {
                return feedback;
            }
        }
    }

    fn error_key(error: CommandError) -> String {
        match error {
            CommandError::Syntax(error) => error.translation_key().to_owned(),
            other => panic!("expected a syntax error, got {other:?}"),
        }
    }

    #[test]
    fn gamemode_sets_self_and_reports_missing_target() {
        let state = state_with(&[(ALICE, "Alice", 1), (BOB, "Bob", 1)]);
        let tree = dispatcher();
        let (alice, mut received) = source(2, Some((ALICE, "Alice")), state.clone());
        assert_eq!(tree.execute("gamemode survival", alice).unwrap(), 1);
        assert!(matches!(
            received.try_recv().unwrap(),
            Action::GameMode { mode: 0, ref targets } if targets == &[Uuid::from_u128(ALICE)]
        ));
        let feedback = next_feedback(&mut received);
        assert_eq!(feedback.key, "commands.gamemode.success.self");
        assert_eq!(feedback.text, "Set own game mode to Survival Mode");
        assert_eq!(
            state
                .lock()
                .unwrap()
                .players
                .iter()
                .find(|player| player.uuid == Uuid::from_u128(ALICE))
                .unwrap()
                .game_mode,
            0
        );

        let (alice, _) = source(2, Some((ALICE, "Alice")), state);
        let error = tree.execute("gamemode creative Nobody", alice).unwrap_err();
        assert_eq!(error_key(error), "argument.entity.notfound.player");
    }

    #[test]
    fn defaultgamemode_updates_state_and_requires_an_argument() {
        let state = state_with(&[]);
        let tree = dispatcher();
        let (console, mut received) = source(4, None, state.clone());
        assert_eq!(
            tree.execute("defaultgamemode creative", console).unwrap(),
            1
        );
        assert!(matches!(
            received.try_recv().unwrap(),
            Action::DefaultGameMode(1)
        ));
        let feedback = next_feedback(&mut received);
        assert_eq!(feedback.key, "commands.defaultgamemode.success");
        assert_eq!(feedback.text, "The default game mode is now Creative Mode");
        assert_eq!(state.lock().unwrap().default_gamemode, 1);

        let (console, _) = source(4, None, state);
        let error = tree.execute("defaultgamemode", console).unwrap_err();
        assert_eq!(error_key(error), "command.unknown.command");
    }

    #[test]
    fn difficulty_queries_sets_and_rejects_the_same_value() {
        let state = state_with(&[]);
        let tree = dispatcher();
        let (console, mut received) = source(2, None, state.clone());
        assert_eq!(tree.execute("difficulty", console).unwrap(), 2);
        let feedback = next_feedback(&mut received);
        assert_eq!(feedback.key, "commands.difficulty.query");
        assert_eq!(feedback.text, "The difficulty is Normal");

        let (console, mut received) = source(2, None, state.clone());
        assert_eq!(tree.execute("difficulty hard", console).unwrap(), 0);
        assert!(matches!(
            received.try_recv().unwrap(),
            Action::Difficulty(3)
        ));
        assert_eq!(
            next_feedback(&mut received).key,
            "commands.difficulty.success"
        );
        assert_eq!(state.lock().unwrap().difficulty, 3);

        let (console, _) = source(2, None, state);
        let error = tree.execute("difficulty hard", console).unwrap_err();
        assert_eq!(error_key(error), "commands.difficulty.failure");
    }

    #[test]
    fn time_sets_adds_queries_and_rejects_negative_ticks() {
        let state = state_with(&[]);
        let tree = dispatcher();
        let (console, mut received) = source(2, None, state.clone());
        assert_eq!(tree.execute("time set 1000", console).unwrap(), 1000);
        assert!(matches!(
            received.try_recv().unwrap(),
            Action::TimeSet(1000)
        ));
        let feedback = next_feedback(&mut received);
        assert_eq!(feedback.key, "commands.time.set.absolute");
        assert_eq!(feedback.text, "Set minecraft:overworld to 1000 tick(s)");

        let (console, mut received) = source(2, None, state.clone());
        assert_eq!(tree.execute("time add 20", console).unwrap(), 1020);
        assert!(matches!(received.try_recv().unwrap(), Action::TimeAdd(20)));
        assert_eq!(
            next_feedback(&mut received).text,
            "Set minecraft:overworld to 1020 tick(s)"
        );

        let (console, mut received) = source(2, None, state.clone());
        assert_eq!(tree.execute("time query time", console).unwrap(), 1020);
        assert_eq!(
            next_feedback(&mut received).text,
            "Clock minecraft:overworld is at 1020 tick(s)"
        );

        let (console, _) = source(2, None, state);
        let error = tree.execute("time set -1", console).unwrap_err();
        assert_eq!(error_key(error), "argument.time.tick_count_too_low");
    }

    #[test]
    fn weather_sets_duration_and_rejects_zero() {
        let state = state_with(&[]);
        let tree = dispatcher();
        let (console, mut received) = source(2, None, state.clone());
        assert_eq!(tree.execute("weather thunder 200", console).unwrap(), 200);
        assert!(matches!(
            received.try_recv().unwrap(),
            Action::Weather {
                kind: Weather::Thunder,
                duration: Some(200)
            }
        ));
        let feedback = next_feedback(&mut received);
        assert_eq!(feedback.key, "commands.weather.set.thunder");
        assert_eq!(feedback.text, "Set the weather to rain and thunder");
        assert_eq!(state.lock().unwrap().weather, Weather::Thunder);

        let (console, _) = source(2, None, state);
        let error = tree.execute("weather rain 0", console).unwrap_err();
        assert_eq!(error_key(error), "argument.time.tick_count_too_low");
    }

    #[test]
    fn seed_reports_world_seed_and_requires_permission() {
        let game_state = GameState {
            seed: 12345,
            ..GameState::default()
        };
        let state = Arc::new(Mutex::new(game_state));
        let tree = dispatcher();
        let (console, mut received) = source(2, None, state.clone());
        assert_eq!(tree.execute("seed", console).unwrap(), 12345);
        let feedback = next_feedback(&mut received);
        assert_eq!(feedback.key, "commands.seed.success");
        assert_eq!(feedback.text, "Seed: 12345");

        let (guest, _) = source(0, None, state);
        assert!(tree.execute("seed", guest).is_err());
    }

    #[test]
    fn list_formats_players_and_rejects_unknown_subcommands() {
        let state = state_with(&[(ALICE, "Alice", 1), (BOB, "Bob", 0)]);
        let tree = dispatcher();
        let (guest, mut received) = source(0, Some((ALICE, "Alice")), state.clone());
        assert_eq!(tree.execute("list", guest).unwrap(), 2);
        let feedback = next_feedback(&mut received);
        assert_eq!(feedback.key, "commands.list.players");
        assert_eq!(
            feedback.text,
            "There are 2 of a max of 20 players online: Alice, Bob"
        );

        let (guest, mut received) = source(0, Some((ALICE, "Alice")), state.clone());
        tree.execute("list uuids", guest).unwrap();
        let feedback = next_feedback(&mut received);
        assert_eq!(
            feedback.text,
            format!(
                "There are 2 of a max of 20 players online: Alice ({}), Bob ({})",
                Uuid::from_u128(ALICE),
                Uuid::from_u128(BOB)
            )
        );

        let (guest, _) = source(0, Some((ALICE, "Alice")), state);
        assert!(tree.execute("list nonsense", guest).is_err());
    }

    #[test]
    fn say_broadcasts_with_announcement_chat_type_and_checks_permission() {
        let state = state_with(&[(ALICE, "Alice", 1)]);
        let tree = dispatcher();
        let (alice, mut received) = source(2, Some((ALICE, "Alice")), state.clone());
        assert_eq!(tree.execute("say hello world", alice).unwrap(), 1);
        let Action::Broadcast(feedback) = received.try_recv().unwrap() else {
            panic!("expected broadcast")
        };
        assert_eq!(feedback.key, "chat.type.announcement");
        assert_eq!(feedback.args, ["Alice", "hello world"]);
        assert_eq!(feedback.text, "[Alice] hello world");

        let (guest, _) = source(0, Some((ALICE, "Alice")), state);
        assert!(tree.execute("say nope", guest).is_err());
    }

    #[test]
    fn me_broadcasts_as_emote_and_requires_an_action() {
        let state = state_with(&[(ALICE, "Alice", 1)]);
        let tree = dispatcher();
        let (alice, mut received) = source(0, Some((ALICE, "Alice")), state.clone());
        assert_eq!(tree.execute("me waves", alice).unwrap(), 1);
        let Action::Broadcast(feedback) = received.try_recv().unwrap() else {
            panic!("expected broadcast")
        };
        assert_eq!(feedback.key, "chat.type.emote");
        assert_eq!(feedback.args, ["Alice", "waves"]);
        assert_eq!(feedback.text, "* Alice waves");

        let (alice, _) = source(0, Some((ALICE, "Alice")), state);
        assert!(tree.execute("me", alice).is_err());
    }

    #[test]
    fn teleport_still_requires_an_operator() {
        let state = state_with(&[(ALICE, "Alice", 1)]);
        let tree = dispatcher();
        let (alice, mut received) = source(2, Some((ALICE, "Alice")), state.clone());
        tree.execute("tp 12 70 -4", alice).unwrap();
        assert!(matches!(
            received.try_recv().unwrap(),
            Action::Teleport([12.0, 70.0, -4.0])
        ));
        let (guest, _) = source(0, Some((ALICE, "Alice")), state);
        assert!(tree.execute("tp 1 2 3", guest).is_err());
    }
}
