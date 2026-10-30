use ferrite_command::{ArgumentType, CommandDispatcher, CommandError, argument, literal};
use tokio::sync::mpsc::UnboundedSender;

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    GameMode(i32),
    Teleport([f64; 3]),
    Say(String),
    Tps,
    Mspt,
    ProfileStart,
    ProfileStop,
    Stop,
    Plugin { name: String, args: Vec<String> },
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
/// Adds every command a plugin declared in its descriptor, including aliases.
///
/// Bukkit resolves the name and every alias to the same `PluginCommand`, so each literal is
/// registered on its own and runs the plugin under the label the sender actually typed.
pub fn register_plugin_commands(
    tree: &mut CommandDispatcher<Source>,
    commands: &[ferrite_plugin::PluginCommand],
) {
    for command in commands {
        tree.register(plugin_command_node(&command.name));
        for alias in &command.aliases {
            tree.register(plugin_command_node(alias));
        }
    }
}
/// One literal (a plugin command name or alias); the plugin sees it as the label.
fn plugin_command_node(literal_name: &str) -> ferrite_command::NodeBuilder<Source> {
    let bare = literal_name.to_owned();
    let with_args = literal_name.to_owned();
    literal::<Source>(literal_name)
        .executes(move |ctx| {
            send(
                ctx.source(),
                Action::Plugin {
                    name: bare.clone(),
                    args: Vec::new(),
                },
            )
        })
        .then(
            argument::<Source>("args", ArgumentType::greedy_string()).executes(move |ctx| {
                let args = ctx
                    .string("args")
                    .unwrap_or_default()
                    .split_whitespace()
                    .map(str::to_owned)
                    .collect();
                send(
                    ctx.source(),
                    Action::Plugin {
                        name: with_args.clone(),
                        args,
                    },
                )
            }),
        )
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

    #[test]
    fn plugin_commands_register_aliases_and_forward_arguments() {
        let mut tree = dispatcher();
        register_plugin_commands(
            &mut tree,
            &[ferrite_plugin::PluginCommand {
                name: "testping".into(),
                aliases: vec!["tping".into()],
                permission: None,
                usage: Some("/testping".into()),
            }],
        );
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let source = Source {
            permission: 0,
            console: false,
            actions: tx,
        };
        tree.execute("testping", source.clone()).unwrap();
        assert_eq!(
            rx.try_recv().unwrap(),
            Action::Plugin {
                name: "testping".into(),
                args: Vec::new()
            }
        );
        tree.execute("tping one two", source).unwrap();
        assert_eq!(
            rx.try_recv().unwrap(),
            Action::Plugin {
                name: "tping".into(),
                args: vec!["one".into(), "two".into()]
            }
        );
    }
}
