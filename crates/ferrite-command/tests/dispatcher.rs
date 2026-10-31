//! Dispatcher behaviour: chains, redirects/forks, permissions, errors, suggestions.

use std::sync::{Arc, Mutex};

use ferrite_command::{
    ArgumentType, ArgumentValue, CommandDispatcher, CommandError, Suggester, SuggestionKind,
    argument, literal,
};

#[derive(Debug, Clone, PartialEq)]
struct Source {
    name: String,
    level: u8,
}

fn player(name: &str, level: u8) -> Source {
    Source {
        name: name.into(),
        level,
    }
}

type Log = Arc<Mutex<Vec<String>>>;

fn dispatcher(log: &Log) -> CommandDispatcher<Source> {
    let mut d = CommandDispatcher::new();
    let l = log.clone();
    d.register(
        literal::<Source>("give").then(
            argument::<Source>("item", ArgumentType::word())
                .executes({
                    let l = l.clone();
                    move |ctx| {
                        l.lock().unwrap().push(format!(
                            "{} gets 1 {}",
                            ctx.source().name,
                            ctx.string("item").unwrap()
                        ));
                        Ok(1)
                    }
                })
                .then(
                    argument::<Source>("count", ArgumentType::integer_range(1, 64)).executes(
                        move |ctx| {
                            let n = ctx.integer("count").unwrap();
                            l.lock().unwrap().push(format!(
                                "{} gets {n} {}",
                                ctx.source().name,
                                ctx.string("item").unwrap()
                            ));
                            Ok(n)
                        },
                    ),
                ),
        ),
    );
    let l = log.clone();
    d.register(literal("say").then(
        argument::<Source>("message", ArgumentType::greedy_string()).executes(move |ctx| {
            l.lock().unwrap().push(format!(
                "<{}> {}",
                ctx.source().name,
                ctx.string("message").unwrap()
            ));
            Ok(1)
        }),
    ));
    let l = log.clone();
    d.register(
        literal("tell").then(argument::<Source>("text", ArgumentType::string()).executes(
            move |ctx| {
                l.lock()
                    .unwrap()
                    .push(ctx.string("text").unwrap().to_owned());
                Ok(1)
            },
        )),
    );
    d.register(
        literal("stop")
            .requires(|s: &Source| s.level >= 4)
            .restricted(true)
            .executes(|_| Ok(1)),
    );
    // execute as <who...> run <command>; `who` forks into one source per name.
    let execute = d.register(literal("execute"));
    d.register(
        literal("execute")
            .then(
                literal("as").then(
                    argument::<Source>("who", ArgumentType::greedy_string()).executes(|_| Ok(0)),
                ),
            )
            .then(literal("run").redirect(d.root())),
    );
    // A second "as": a word list separated by commas, forking.
    d.register(literal("execute").then(literal("each").then(
        argument::<Source>("names", ArgumentType::word()).fork(execute, |ctx| {
            Ok(ctx
                .string("names")
                .unwrap()
                .split('+')
                .map(|n| player(n, ctx.source().level))
                .collect())
        }),
    )));
    d.register(literal("execute").then(literal("with").then(
        argument::<Source>("name", ArgumentType::word()).redirect_with(execute, |ctx| {
            Ok(player(ctx.string("name").unwrap(), ctx.source().level))
        }),
    )));
    d
}

#[test]
fn literal_argument_chain() {
    let log = Log::default();
    let d = dispatcher(&log);
    assert_eq!(d.execute("give diamond", player("Alex", 0)).unwrap(), 1);
    assert_eq!(d.execute("give diamond 32", player("Alex", 0)).unwrap(), 32);
    assert_eq!(
        *log.lock().unwrap(),
        ["Alex gets 1 diamond", "Alex gets 32 diamond"]
    );
    let parse = d.parse("give diamond 32", player("Alex", 0));
    assert!(parse.is_complete());
    assert_eq!(
        parse.context.parsed_argument("count").unwrap().range,
        ferrite_command::StringRange::new(13, 15)
    );
}

#[test]
fn unknown_and_incomplete_commands() {
    let log = Log::default();
    let d = dispatcher(&log);
    let err = d.execute("nope", player("A", 0)).unwrap_err();
    let CommandError::Syntax(e) = err else {
        panic!()
    };
    assert_eq!(e.translation_key(), "command.unknown.command");
    assert_eq!(
        e.to_string(),
        "Unknown or incomplete command. See below for error at position 0: <--[HERE]"
    );
    // Incomplete: "give" has no command of its own.
    let CommandError::Syntax(e) = d.execute("give", player("A", 0)).unwrap_err() else {
        panic!()
    };
    assert_eq!(e.translation_key(), "command.unknown.command");
    assert_eq!(e.cursor(), Some(4));
    // Trailing garbage after a complete argument.
    let CommandError::Syntax(e) = d
        .execute("give diamond 3 extra", player("A", 0))
        .unwrap_err()
    else {
        panic!()
    };
    assert_eq!(e.translation_key(), "command.unknown.argument");
    let ctx = e.error_context().unwrap();
    assert_eq!((ctx.ellipsis, ctx.before.as_str()), (true, "diamond 3 "));
    assert_eq!(ctx.after, "extra");
}

#[test]
fn int_range_errors_surface_through_dispatcher() {
    let log = Log::default();
    let d = dispatcher(&log);
    let CommandError::Syntax(e) = d.execute("give diamond 65", player("A", 0)).unwrap_err() else {
        panic!()
    };
    assert_eq!(e.message(), "Integer must not be more than 64: found 65");
    assert_eq!(e.cursor(), Some(13));
    assert_eq!(
        e.to_string(),
        "Integer must not be more than 64: found 65 at position 13: ...e diamond <--[HERE]"
    );
    let CommandError::Syntax(e) = d.execute("give diamond 0", player("A", 0)).unwrap_err() else {
        panic!()
    };
    assert_eq!(e.args(), ["1", "0"]);
    let CommandError::Syntax(e) = d.execute("give diamond x1", player("A", 0)).unwrap_err() else {
        panic!()
    };
    assert_eq!(e.message(), "Expected integer");
}

#[test]
fn quoted_escapes_and_greedy_strings() {
    let log = Log::default();
    let d = dispatcher(&log);
    d.execute(r#"tell "say \"hi\" \\o/""#, player("A", 0))
        .unwrap();
    d.execute("tell 'single \"ok\"'", player("A", 0)).unwrap();
    d.execute("tell bare", player("A", 0)).unwrap();
    d.execute("say  spaced   out \"quotes\" stay ", player("Bo", 0))
        .unwrap();
    assert_eq!(
        *log.lock().unwrap(),
        [
            r#"say "hi" \o/"#,
            "single \"ok\"",
            "bare",
            "<Bo>  spaced   out \"quotes\" stay "
        ]
    );
    let CommandError::Syntax(e) = d.execute(r#"tell "bad \n""#, player("A", 0)).unwrap_err() else {
        panic!()
    };
    assert_eq!(
        e.message(),
        r"Invalid escape sequence '\n' in quoted string"
    );
    assert_eq!(e.cursor(), Some(11));
    let CommandError::Syntax(e) = d.execute(r#"tell "open"#, player("A", 0)).unwrap_err() else {
        panic!()
    };
    assert_eq!(e.message(), "Unclosed quoted string");
}

#[test]
fn redirect_to_root_and_fork() {
    let log = Log::default();
    let d = dispatcher(&log);
    // Plain redirect back to the root keeps the source.
    assert_eq!(
        d.execute("execute run give stick", player("A", 0)).unwrap(),
        1
    );
    // Fork: one branch per source; the result is the number of successful branches.
    let n = d
        .execute(
            "execute each Steve+Alex+Bob run give apple 5",
            player("Op", 2),
        )
        .unwrap();
    assert_eq!(n, 3);
    // Single-source redirect with modifier, chained with a fork.
    let n = d
        .execute(
            "execute with Zed run execute each X+Y run give egg 2",
            player("Op", 2),
        )
        .unwrap();
    assert_eq!(n, 2);
    assert_eq!(
        *log.lock().unwrap(),
        [
            "A gets 1 stick",
            "Steve gets 5 apple",
            "Alex gets 5 apple",
            "Bob gets 5 apple",
            "X gets 2 egg",
            "Y gets 2 egg"
        ]
    );
    // A redirect with nothing after it is incomplete.
    assert!(d.execute("execute each A+B", player("Op", 2)).is_err());

    let parse = d.parse("execute each A+B run give x", player("Op", 2));
    let leaf = parse.context.last_child();
    assert_eq!(
        leaf.argument("item"),
        Some(&ArgumentValue::String("x".into()))
    );
}

#[test]
fn fork_tolerates_failing_branches() {
    let mut d = CommandDispatcher::<Source>::new();
    let root = d.root();
    d.register(
        literal("fail").executes(|ctx: &ferrite_command::CommandContext<Source>| {
            if ctx.source().name == "bad" {
                Err(CommandError::failed("nope"))
            } else {
                Ok(7)
            }
        }),
    );
    d.register(
        literal("each").then(argument("n", ArgumentType::word()).fork(root, |ctx| {
            Ok(ctx
                .string("n")
                .unwrap()
                .split('+')
                .map(|n| player(n, 0))
                .collect())
        })),
    );
    assert_eq!(d.execute("each a+bad+c fail", player("x", 0)).unwrap(), 2);
    assert_eq!(
        d.execute("fail", player("bad", 0)).unwrap_err(),
        CommandError::failed("nope")
    );
}

#[test]
fn permission_filtering() {
    let log = Log::default();
    let d = dispatcher(&log);
    let CommandError::Syntax(e) = d.execute("stop", player("A", 0)).unwrap_err() else {
        panic!()
    };
    assert_eq!(e.translation_key(), "command.unknown.command");
    assert_eq!(d.execute("stop", player("Op", 4)).unwrap(), 1);

    assert!(!d.suggest("st", player("A", 0)).texts().contains(&"stop"));
    assert_eq!(d.suggest("st", player("Op", 4)).texts(), ["stop"]);

    let usage = d.all_usage(d.root(), &player("A", 0), true);
    assert!(!usage.iter().any(|u| u == "stop"));
    assert!(usage.contains(&"give <item> <count>".to_owned()));
    assert!(usage.contains(&"execute run ...".to_owned()));
    assert!(usage.contains(&"execute each <names> -> execute".to_owned()));
    let usage = d.all_usage(d.root(), &player("Op", 4), true);
    assert!(usage.iter().any(|u| u == "stop"));
}

#[test]
fn smart_usage() {
    let log = Log::default();
    let d = dispatcher(&log);
    let usage: Vec<String> = d
        .smart_usage(d.root(), &player("A", 0))
        .into_iter()
        .map(|(_, u)| u)
        .collect();
    assert!(usage.contains(&"give <item> [<count>]".to_owned()));
    assert!(usage.contains(&"say <message>".to_owned()));
    assert!(usage.contains(&"execute (as|run|each|with)".to_owned()));
}

#[test]
fn suggestions_at_cursor() {
    let log = Log::default();
    let mut d = dispatcher(&log);
    d.register(
        literal("tp").then(
            argument("target", ArgumentType::word())
                .suggests(|_, b| {
                    for name in ["Alex", "alice", "Bob"] {
                        b.suggest_matching(name);
                    }
                })
                .then(argument("pos", ArgumentType::BlockPos).executes(|_| Ok(1))),
        ),
    );
    d.register(literal("toggle").then(argument("on", ArgumentType::bool()).executes(|_| Ok(1))));
    let src = player("A", 0);

    // Root literals.
    let s = d.suggest("t", src.clone());
    assert_eq!(s.texts(), ["tell", "toggle", "tp"]);
    assert_eq!(s.range, ferrite_command::StringRange::new(0, 1));

    // Custom provider, case-insensitive prefix.
    let s = d.suggest("tp al", src.clone());
    assert_eq!(s.texts(), ["Alex", "alice"]);
    assert_eq!(s.range, ferrite_command::StringRange::new(3, 5));

    // Built-in type suggestions after a space.
    assert_eq!(d.suggest("toggle ", src.clone()).texts(), ["false", "true"]);
    assert_eq!(
        d.suggest("tp Bob ", src.clone()).texts(),
        ["~", "~ ~", "~ ~ ~"]
    );

    // Cursor in the middle of the input: complete the first word only.
    let parse = d.parse("gi diamond", src.clone());
    let s = d.completion_suggestions(&parse, 2);
    assert_eq!(s.texts(), ["give"]);
    assert_eq!(s.list[0].apply("gi diamond"), "give diamond");

    // Through a redirect: children of the root after `execute run `.
    let s = d.suggest("execute run to", src.clone());
    assert_eq!(s.texts(), ["toggle"]);
    assert_eq!(s.range, ferrite_command::StringRange::new(12, 14));
    let s = d.suggest("execute ", src.clone());
    assert_eq!(s.texts(), ["as", "each", "run", "with"]);
}

#[test]
fn literal_preferred_over_argument_and_ambiguities() {
    let mut d = CommandDispatcher::<()>::new();
    d.register(
        literal("x")
            .then(literal("all").executes(|_| Ok(1)))
            .then(argument("name", ArgumentType::word()).executes(|_| Ok(2))),
    );
    assert_eq!(d.execute("x all", ()).unwrap(), 1);
    assert_eq!(d.execute("x other", ()).unwrap(), 2);

    let amb = d.find_ambiguities();
    assert_eq!(amb.len(), 1);
    let a = &amb[0];
    assert_eq!(d.node(a.child).name(), "all");
    assert_eq!(d.node(a.sibling).name(), "name");
    assert_eq!(a.inputs, ["all"]);
}

#[test]
fn ambiguity_between_argument_types() {
    let mut d = CommandDispatcher::<()>::new();
    d.register(
        literal("n")
            .then(argument("int", ArgumentType::integer()).executes(|_| Ok(1)))
            .then(argument("word", ArgumentType::word()).executes(|_| Ok(2))),
    );
    // Both accept "0", "123", "-123"; the first registered wins on parse.
    assert_eq!(d.execute("n 5", ()).unwrap(), 1);
    let amb = d.find_ambiguities();
    assert!(
        amb.iter()
            .any(|a| d.node(a.child).name() == "int" && a.inputs == ["0", "123", "-123"])
    );
}

#[test]
fn merging_registrations() {
    let mut d = CommandDispatcher::<()>::new();
    let a = d.register(literal("a").then(literal("b").executes(|_| Ok(1))));
    let a2 = d.register(literal("a").then(literal("c").executes(|_| Ok(2))));
    assert_eq!(a, a2);
    assert_eq!(d.execute("a b", ()).unwrap(), 1);
    assert_eq!(d.execute("a c", ()).unwrap(), 2);
    assert_eq!(
        d.path(d.find_node(&["a", "c"]).unwrap()).unwrap(),
        ["a", "c"]
    );
}

#[test]
fn selectors_require_the_configured_permission() {
    let log = Log::default();
    let mut d = dispatcher(&log);
    d.set_selector_requirement(|s: &Source| s.level >= 2);
    d.register(
        literal::<Source>("msg").then(argument::<Source>("targets", ArgumentType::players()).then(
            argument::<Source>("message", ArgumentType::greedy_string()).executes(|_| Ok(1)),
        )),
    );

    for input in ["msg @a hi", "msg @s hi", "msg @a[name=Alex] hi"] {
        let CommandError::Syntax(error) = d.execute(input, player("A", 1)).unwrap_err() else {
            panic!("{input} should be a syntax error");
        };
        assert_eq!(
            error.translation_key(),
            "argument.entity.selector.not_allowed",
            "{input}"
        );
        assert_eq!(error.message(), "Selector not allowed");
        assert_eq!(error.cursor(), Some(4), "{input}");
        assert_eq!(d.execute(input, player("Op", 2)).unwrap(), 1, "{input}");
    }
    // A player name is not a selector and stays available at every level.
    assert_eq!(d.execute("msg Alex hi", player("A", 0)).unwrap(), 1);
    // Entity-only selectors are gated the same way, for a command open to every player.
    d.register(
        literal::<Source>("kill")
            .then(argument::<Source>("targets", ArgumentType::entities()).executes(|_| Ok(1))),
    );
    for input in ["kill @e", "kill @e[type=minecraft:item]"] {
        let CommandError::Syntax(error) = d.execute(input, player("A", 1)).unwrap_err() else {
            panic!("{input} should be a syntax error");
        };
        assert_eq!(
            error.translation_key(),
            "argument.entity.selector.not_allowed",
            "{input}"
        );
        assert_eq!(d.execute(input, player("Op", 2)).unwrap(), 1, "{input}");
    }
}

#[test]
fn suggester_kind_is_kept() {
    let mut d = CommandDispatcher::<()>::new();
    d.register(literal("summon").then(
        argument("entity", ArgumentType::ResourceLocation).suggester(Suggester {
            kind: SuggestionKind::SummonableEntities,
            provider: None,
        }),
    ));
    let node = d.find_node(&["summon", "entity"]).unwrap();
    assert_eq!(
        d.node(node).suggester().unwrap().kind.id(),
        "minecraft:summonable_entities"
    );
}
