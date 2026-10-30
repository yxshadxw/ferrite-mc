//! End-to-end M3: the server starts the embedded JVM, enables plugins, and disables them on stop.
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("crate lives in <repo>/crates/<name>")
        .to_owned()
}

/// Build the Java bridge, the test plugin and its dependency with the Gradle wrapper.
fn build_jars(repo: &Path, java_home: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let project = repo.join("java").join("ferrite-api-impl");
    let mut command = if cfg!(windows) {
        let mut command = Command::new("cmd");
        command.args([
            "/C",
            ".\\gradlew.bat",
            "--console=plain",
            ":test-plugin:jar",
            ":test-dependency:jar",
            "shadedJar",
        ]);
        command
    } else {
        let mut command = Command::new("./gradlew");
        command.args([
            "--console=plain",
            ":test-plugin:jar",
            ":test-dependency:jar",
            "shadedJar",
        ]);
        command
    };
    let output = command
        .current_dir(&project)
        .env("JAVA_HOME", java_home)
        .output()
        .expect("running the Gradle wrapper");
    assert!(
        output.status.success(),
        "gradle build failed in {}:\n{}\n{}",
        project.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let plugin = repo.join("java/test-plugin/build/libs/test-plugin.jar");
    let dependency = repo.join("java/test-dependency/build/libs/test-dependency.jar");
    let bridge = project.join("build/libs/ferrite-api-impl-all.jar");
    for jar in [&plugin, &dependency, &bridge] {
        assert!(jar.exists(), "missing built jar: {}", jar.display());
    }
    (plugin, dependency, bridge)
}

fn toml_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Kills the server if a test assertion panics before it exits.
struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

fn capture(
    mut reader: impl Read + Send + 'static,
) -> (Arc<Mutex<String>>, std::thread::JoinHandle<()>) {
    let log = Arc::new(Mutex::new(String::new()));
    let target = Arc::clone(&log);
    let handle = std::thread::spawn(move || {
        let mut buffer = [0_u8; 4096];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(read) => target
                    .lock()
                    .unwrap()
                    .push_str(&String::from_utf8_lossy(&buffer[..read])),
            }
        }
    });
    (log, handle)
}

#[test]
fn server_loads_plugins_at_startup_and_disables_them_on_stop() {
    let java_home = match ferrite_jvm::find_java_home(None) {
        Ok(home) => home.root().to_owned(),
        Err(error) => {
            eprintln!("Java 25 unavailable; skipping plugin lifecycle test: {error}");
            return;
        }
    };
    let repo = repo_root();
    let (plugin_jar, dependency_jar, bridge_jar) = build_jars(&repo, &java_home);
    let dir = tempfile::tempdir().unwrap();
    let plugins = dir.path().join("plugins");
    fs::create_dir(&plugins).unwrap();
    fs::copy(plugin_jar, plugins.join("test-plugin.jar")).unwrap();
    fs::copy(dependency_jar, plugins.join("test-dependency.jar")).unwrap();
    fs::write(
        dir.path().join("ferrite.toml"),
        format!(
            "bind-address = \"127.0.0.1:0\"\nonline-mode = false\nmotd = \"Plugin lifecycle\"\nplugins-dir = \"plugins\"\nplugin-bridge = \"{}\"\n",
            toml_path(&bridge_jar)
        ),
    )
    .unwrap();
    let mut process = Process(
        Command::new(env!("CARGO_BIN_EXE_ferrite-server"))
            .current_dir(dir.path())
            .env("JAVA_HOME", &java_home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("starting the server"),
    );
    let (stdout, stdout_reader) = capture(process.0.stdout.take().unwrap());
    let (stderr, stderr_reader) = capture(process.0.stderr.take().unwrap());
    let enabled = plugins.join("TestPlugin/enabled.txt");
    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        if enabled.exists() {
            break;
        }
        if let Some(status) = process.0.try_wait().unwrap() {
            panic!(
                "server exited before the plugin enabled ({status}):\n{}\n{}",
                stdout.lock().unwrap().clone(),
                stderr.lock().unwrap().clone()
            );
        }
        if Instant::now() >= deadline {
            panic!(
                "plugin did not write {} within 180 seconds:\n{}\n{}",
                enabled.display(),
                stdout.lock().unwrap().clone(),
                stderr.lock().unwrap().clone()
            );
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let mut stdin = process.0.stdin.take().unwrap();
    stdin.write_all(b"stop\n").unwrap();
    drop(stdin);
    let deadline = Instant::now() + Duration::from_secs(120);
    let status = loop {
        if let Some(status) = process.0.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            panic!("server did not stop within 120 seconds");
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    stdout_reader.join().unwrap();
    stderr_reader.join().unwrap();
    let logs = format!(
        "{}\n{}",
        stdout.lock().unwrap().clone(),
        stderr.lock().unwrap().clone()
    );
    assert!(status.success(), "server exited with {status}:\n{logs}");
    assert!(
        logs.contains("server name Ferrite"),
        "plugin did not see the Ferrite server:\n{logs}"
    );
    assert!(
        logs.contains("FerriteTest disabled"),
        "plugin onDisable did not run:\n{logs}"
    );
    assert!(
        plugins.join("TestPlugin/disabled.txt").exists(),
        "plugin did not write its disable marker:\n{logs}"
    );
}
