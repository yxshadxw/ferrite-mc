use ferrite_plugin::{
    CommandSender, EntityTask, PluginConfig, PluginHost, RegionContext, RegionTask,
};

#[derive(Default)]
struct FakeRegions {
    regions: std::sync::Mutex<Vec<RegionTask>>,
    entities: std::sync::Mutex<Vec<EntityTask>>,
}
impl RegionContext for FakeRegions {
    fn is_owned_by_current_region(&self, _: uuid::Uuid, x: i32, z: i32) -> bool {
        z == 8 && (-4..=-3).contains(&x)
    }
    fn schedule_on_region(&self, task: RegionTask) {
        self.regions.lock().unwrap().push(task);
    }
    fn schedule_on_entity(&self, task: EntityTask) {
        self.entities.lock().unwrap().push(task);
    }
    fn retire_entity(&self, _: uuid::Uuid) {}
}
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
    // Build a clean path: `cmd.exe` mishandles a cwd that mixes `\`, `/` and `..`.
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("crate lives in <repo>/crates/<name>")
        .to_owned();
    let project = repo_root.join("java").join("ferrite-api-impl");
    let mut command = if cfg!(windows) {
        let mut c = Command::new("cmd");
        // `cmd` may not search the cwd (NoDefaultCurrentDirectoryInExePath), so be explicit.
        c.args(["/C", ".\\gradlew.bat", "build"]);
        c
    } else {
        let mut c = Command::new("./gradlew");
        c.arg("build");
        c
    };
    let result = command
        .current_dir(&project)
        .env("JAVA_HOME", &java_home)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "gradle build failed in {}:
{}
{}",
        project.display(),
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let dir = tempfile::tempdir().unwrap();
    std::fs::copy(
        repo_root
            .join("java")
            .join("test-plugin")
            .join("build/libs/test-plugin.jar"),
        dir.path().join("test.jar"),
    )
    .unwrap();
    std::fs::copy(
        repo_root
            .join("java")
            .join("test-plugin")
            .join("build/libs/test-plugin-legacy.jar"),
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
    let regions = std::sync::Arc::new(FakeRegions::default());
    let cfg = PluginConfig {
        region_context: Some(regions.clone()),
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
    host.route_ready_tasks().unwrap();
    let entities = std::mem::take(&mut *regions.entities.lock().unwrap());
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
    let mut forged_entity = entities[0].clone();
    forged_entity.entity = "forged".into();
    assert!(host.run_entity_task(forged_entity).is_err());
    for task in entities {
        host.run_entity_task(task.clone()).unwrap();
        assert!(host.run_entity_task(task).is_err());
    }
    assert_eq!(
        host.logs()
            .iter()
            .filter(|line| line.contains("FerriteTest entity"))
            .count(),
        2
    );
    let tasks = std::mem::take(&mut *regions.regions.lock().unwrap());
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
    let mut forged_region = tasks[0].clone();
    forged_region.chunk_x += 1;
    assert!(host.run_region_task(forged_region).is_err());
    for task in tasks {
        host.run_region_task(task.clone()).unwrap();
        assert!(host.run_region_task(task).is_err());
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
    assert_eq!(
        host.logs()
            .iter()
            .filter(|line| line.contains("FerriteTest range owned"))
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
