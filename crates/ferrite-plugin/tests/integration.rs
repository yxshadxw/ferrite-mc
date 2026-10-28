use ferrite_plugin::{CommandSender, PluginConfig, PluginHost};
use std::{path::PathBuf, process::Command};

#[test]
fn plugin_lifecycle() {
    let java_home = match ferrite_jvm::find_java_home(None) {
        Ok(home) => home.root().to_owned(),
        Err(error) => {
            eprintln!("Java 25 unavailable; skipping plugin integration test: {error}");
            return;
        }
    };
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../java/ferrite-api-impl");
    let mut command = if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.args(["/C", "gradlew.bat", "build"]);
        c
    } else {
        let mut c = Command::new("./gradlew");
        c.arg("build");
        c
    };
    let result = command
        .current_dir(&project)
        .env("JAVA_HOME", &java_home)
        .status()
        .unwrap();
    assert!(result.success());
    let dir = tempfile::tempdir().unwrap();
    std::fs::copy(
        project.join("../test-plugin/build/libs/test-plugin.jar"),
        dir.path().join("test.jar"),
    )
    .unwrap();
    std::fs::copy(
        project.join("../test-plugin/build/libs/test-plugin-legacy.jar"),
        dir.path().join("legacy.jar"),
    )
    .unwrap();
    std::fs::copy(
        project.join("../test-dependency/build/libs/test-dependency.jar"),
        dir.path().join("dependency.jar"),
    )
    .unwrap();
    let called = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let captured = called.clone();
    let cfg = PluginConfig {
        command_handler: Some(std::sync::Arc::new(move |_, line| {
            captured.lock().unwrap().push(line.to_owned());
            true
        })),
        bridge_jar: project.join("build/libs/ferrite-api-impl-all.jar"),
        java_home: Some(java_home),
    };
    let host = PluginHost::start(dir.path(), cfg).unwrap();
    assert!(
        host.logs()
            .iter()
            .any(|line| line.contains("FerriteTest enabled"))
    );
    assert!(
        !host
            .dispatch_command(
                CommandSender::Player(uuid::Uuid::new_v4()),
                "ferrite-test hello"
            )
            .unwrap()
    );
    assert!(
        host.dispatch_command(CommandSender::Console, "ferrite-test hello")
            .unwrap()
    );
    assert!(host.logs().iter().any(|line| line.contains("hello hello")));
    assert_eq!(&*called.lock().unwrap(), &["bridge-command"]);
    for _ in 0..3 {
        host.tick().unwrap();
    }
    assert_eq!(
        host.logs()
            .iter()
            .filter(|line| line.contains("FerriteTest tick"))
            .count(),
        6
    );
    let entities = host.take_entity_tasks();
    assert_eq!(entities.len(), 2);
    assert!(entities.iter().any(|task| task.region_aware));
    assert!(entities.iter().any(|task| !task.region_aware));
    assert!(
        entities
            .iter()
            .all(|task| task.entity == "00000000-0000-0000-0000-000000000002")
    );
    assert!(
        !host
            .logs()
            .iter()
            .any(|line| line.contains("FerriteTest entity"))
    );
    for task in entities {
        host.run_entity_task(task).unwrap();
    }
    assert_eq!(
        host.logs()
            .iter()
            .filter(|line| line.contains("FerriteTest entity"))
            .count(),
        2
    );
    let tasks = host.take_region_tasks();
    assert_eq!(tasks.len(), 2);
    assert!(tasks.iter().any(|task| task.region_aware));
    assert!(tasks.iter().any(|task| !task.region_aware));
    assert!(
        tasks
            .iter()
            .all(|task| (task.chunk_x, task.chunk_z) == (-4, 8)
                && task.world == "00000000-0000-0000-0000-000000000001")
    );
    assert!(
        !host
            .logs()
            .iter()
            .any(|line| line.contains("FerriteTest region"))
    );
    for task in tasks {
        host.run_region_task(task).unwrap();
    }
    assert_eq!(
        host.logs()
            .iter()
            .filter(|line| line.contains("FerriteTest region"))
            .count(),
        2
    );
    assert_eq!(
        host.logs()
            .iter()
            .filter(|line| line.contains("FerriteTest owned"))
            .count(),
        2
    );
    host.shutdown().unwrap();
    assert!(
        host.logs()
            .iter()
            .any(|line| line.contains("FerriteTest disabled"))
    );
    assert!(!host.drain_logs().is_empty());
    assert!(host.logs().is_empty());
}
