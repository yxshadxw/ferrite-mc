use ferrite_command::{ArgumentType, CommandDispatcher, CommandError, argument, literal};
use tokio::sync::mpsc::UnboundedSender;

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    GameMode(i32),
    Teleport([f64; 3]),
    Say(String),
    Tps,
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
        assert!(tree.execute("gamemode survival", guest).is_err());
        let operator = Source {
            permission: 2,
            console: false,
            actions: tx,
        };
        tree.execute("gamemode survival", operator.clone()).unwrap();
        assert!(matches!(rx.try_recv().unwrap(), Action::GameMode(0)));
        tree.execute("tp 12 70 -4", operator).unwrap();
        assert!(matches!(
            rx.try_recv().unwrap(),
            Action::Teleport([12.0, 70.0, -4.0])
        ));
    }
}
