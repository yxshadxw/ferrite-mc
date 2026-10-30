use ferrite_command::{ArgumentType, CommandDispatcher, CommandError, argument, literal};
use tokio::sync::mpsc::UnboundedSender;

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    GameMode(i32),
    GameRule(String, Option<String>),
    Teleport([f64; 3]),
    Say(String),
    Tps,
    Mspt,
    ProfileStart,
    ProfileStop,
    Stop,
}
#[derive(Clone)]
pub struct Source {
    pub permission: u8,
    pub console: bool,
    pub actions: UnboundedSender<Action>,
}
fn send(source: &Source, action: Action) -> Result<i32, CommandError> {
    source
        .actions
        .send(action)
        .map_err(|_| CommandError::failed("command target unavailable"))?;
    Ok(1)
}
pub fn dispatcher() -> CommandDispatcher<Source> {
    let mut tree = CommandDispatcher::new();
    tree.register(
        literal::<Source>("gamerule")
            .requires(|s| s.permission >= 2)
            .then(
                argument("rule", ArgumentType::word())
                    .executes(|ctx| {
                        let rule = ctx.string("rule").unwrap_or_default();
                        if ferrite_command::gamerule::GameRules::default()
                            .get(rule)
                            .is_none()
                        {
                            return Err(CommandError::failed("Unknown game rule"));
                        }
                        send(ctx.source(), Action::GameRule(rule.to_owned(), None))
                    })
                    .then(argument("value", ArgumentType::word()).executes(|ctx| {
                        let rule = ctx.string("rule").unwrap_or_default();
                        let value = ctx.string("value").unwrap_or_default();
                        ferrite_command::gamerule::GameRules::default()
                            .set(rule, value)
                            .map_err(CommandError::failed)?;
                        send(
                            ctx.source(),
                            Action::GameRule(rule.to_owned(), Some(value.to_owned())),
                        )
                    })),
            ),
    );
    tree.register(
        literal::<Source>("gamemode")
            .requires(|s| s.permission >= 2)
            .then(literal("creative").executes(|ctx| send(ctx.source(), Action::GameMode(1))))
            .then(literal("survival").executes(|ctx| send(ctx.source(), Action::GameMode(0)))),
    );
    tree.register(
        literal::<Source>("tp")
            .requires(|s| s.permission >= 2)
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
    tree.register(
        literal::<Source>("stop")
            .requires(|s| s.console && s.permission >= 4)
            .executes(|ctx| send(ctx.source(), Action::Stop)),
    );
    tree
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn permissions_and_actions_follow_command_tree() {
        let tree = dispatcher();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let guest = Source {
            permission: 0,
            console: false,
            actions: tx.clone(),
        };
        assert!(tree.execute("gamemode survival", guest.clone()).is_err());
        assert!(tree.execute("profile start", guest.clone()).is_err());
        tree.execute("tps", guest.clone()).unwrap();
        assert!(matches!(rx.try_recv().unwrap(), Action::Tps));
        tree.execute("mspt", guest).unwrap();
        assert!(matches!(rx.try_recv().unwrap(), Action::Mspt));

        let operator = Source {
            permission: 2,
            console: false,
            actions: tx,
        };
        tree.execute("gamemode survival", operator.clone()).unwrap();
        assert!(matches!(rx.try_recv().unwrap(), Action::GameMode(0)));
        tree.execute("tp 12 70 -4", operator.clone()).unwrap();
        assert!(matches!(
            rx.try_recv().unwrap(),
            Action::Teleport([12.0, 70.0, -4.0])
        ));
        tree.execute("profile start", operator.clone()).unwrap();
        assert!(matches!(rx.try_recv().unwrap(), Action::ProfileStart));
        tree.execute("profile stop", operator).unwrap();
        assert!(matches!(rx.try_recv().unwrap(), Action::ProfileStop));
    }
}
#[test]
fn gamerule_commands_validate_types_and_query() {
    let tree = dispatcher();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let op = Source {
        permission: 2,
        console: false,
        actions: tx,
    };
    tree.execute("gamerule randomTickSpeed 0", op.clone())
        .unwrap();
    assert_eq!(
        rx.try_recv().unwrap(),
        Action::GameRule("randomTickSpeed".into(), Some("0".into()))
    );
    tree.execute("gamerule doDaylightCycle", op.clone())
        .unwrap();
    assert_eq!(
        rx.try_recv().unwrap(),
        Action::GameRule("doDaylightCycle".into(), None)
    );
    assert!(
        tree.execute("gamerule randomTickSpeed -1", op.clone())
            .is_err()
    );
    assert!(
        tree.execute("gamerule doMobSpawning 42", op.clone())
            .is_err()
    );
    assert!(tree.execute("gamerule doesNotExist", op).is_err());
}
