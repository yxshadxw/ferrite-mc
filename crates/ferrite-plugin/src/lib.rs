//! Hosts plugins in an embedded JVM.
use ferrite_jvm::jni::{
    JNIEnv,
    objects::{JClass, JObject, JString, JValue},
};
use ferrite_jvm::{Jvm, JvmConfig, NativeMethod};
pub use scheduling::{EntityTask, RegionTask};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

static LOGS: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
static REGION_QUEUE: OnceLock<Mutex<scheduling::Queue>> = OnceLock::new();
static READY: OnceLock<Mutex<Vec<RegionTask>>> = OnceLock::new();
static READY_ENTITIES: OnceLock<Mutex<Vec<EntityTask>>> = OnceLock::new();
static MODES: OnceLock<HashMap<String, bool>> = OnceLock::new();
static REGION_CONTEXT: OnceLock<Option<std::sync::Arc<dyn RegionContext>>> = OnceLock::new();
static COMMAND_HANDLER: OnceLock<Option<CommandHandler>> = OnceLock::new();
thread_local! { static REGION: std::cell::RefCell<Option<(String, i32, i32)>> = const { std::cell::RefCell::new(None) }; }
const BOOTSTRAP: &str = "dev/ferrite/server/Bootstrap";

#[derive(Default)]
pub struct PluginConfig {
    pub bridge_jar: PathBuf,
    pub java_home: Option<PathBuf>,
    pub command_handler: Option<CommandHandler>,
    pub region_context: Option<std::sync::Arc<dyn RegionContext>>,
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
    #[error("event bridge is not implemented")]
    EventUnsupported,
    #[error("task was not issued or has already been consumed")]
    InvalidTask,
    #[error("region context is not configured")]
    MissingRegionContext,
}
pub enum CommandSender {
    Console,
    Player(uuid::Uuid),
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
        let _ = REGION_CONTEXT.set(cfg.region_context.clone());
        jvm.register_natives(
            "dev/ferrite/server/Native$Bindings",
            &[
                NativeMethod::new("log", "(Ljava/lang/String;)V", log as *mut _),
                NativeMethod::new(
                    "dispatchCommand",
                    "(Ljava/lang/String;Ljava/lang/String;)Z",
                    dispatch_back as *mut _,
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
    /// Placeholder until server events have a Java representation.
    pub fn call_event(&self, _event: &str) -> Result<(), Error> {
        Err(Error::EventUnsupported)
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
