//! A datapack `minecraft:tick` function runs every tick and its block change reaches a
//! live bot session.
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    path::PathBuf,
    time::{Duration, Instant},
};

use ferrite_protocol::{
    Decode, FrameDecoder, FrameEncoder, Packet, VarInt, encode_packet,
    packets::{
        configuration::{
            AcknowledgeFinishConfiguration, ClientboundKnownPacks, ServerboundKnownPacks,
        },
        handshake::{Intent, Intention},
        login::{LoginAcknowledged, LoginStart},
        play::{BlockUpdate, ChunkBatchReceived, ConfirmTeleport, PositionSync},
    },
    types::BlockPos,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("crate lives in <repo>/crates/<name>")
        .to_owned()
}

/// A minimal protocol bot: enough to log in and read play packets.
struct Bot {
    stream: TcpStream,
    encoder: FrameEncoder,
    decoder: FrameDecoder,
}

impl Bot {
    fn join(addr: SocketAddr, name: &str) -> Self {
        let deadline = Instant::now() + Duration::from_secs(60);
        let stream = loop {
            match TcpStream::connect(addr) {
                Ok(stream) => break stream,
                Err(_) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(error) => panic!("connecting to {addr}: {error}"),
            }
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(60)))
            .unwrap();
        let mut bot = Self {
            stream,
            encoder: FrameEncoder::default(),
            decoder: FrameDecoder::default(),
        };
        bot.send(&Intention {
            protocol_version: 777,
            server_address: "localhost".into(),
            server_port: addr.port(),
            intent: Intent::Login,
        });
        bot.send(&LoginStart {
            name: name.into(),
            uuid: uuid::Uuid::nil(),
        });
        let first = bot.read_frame().expect("no set compression packet");
        assert_eq!(VarInt::decode(&mut first.as_slice()).unwrap().0, 3);
        bot.encoder.set_compression(Some(256), 6);
        bot.decoder.set_compression(Some(256));
        let login = bot.read_frame().expect("no login success packet");
        assert_eq!(VarInt::decode(&mut login.as_slice()).unwrap().0, 2);
        bot.send(&LoginAcknowledged);
        loop {
            let frame = bot.read_frame().expect("no finish configuration packet");
            let mut body = frame.as_slice();
            let id = VarInt::decode(&mut body).unwrap().0;
            if id == ClientboundKnownPacks::ID {
                let packs = ClientboundKnownPacks::decode(&mut body).unwrap();
                bot.send(&ServerboundKnownPacks { packs: packs.packs });
            }
            if id == 3 {
                break;
            }
        }
        bot.send(&AcknowledgeFinishConfiguration);
        bot.until(73);
        bot.stream
            .set_read_timeout(Some(Duration::from_millis(150)))
            .unwrap();
        bot
    }

    fn send<P: Packet>(&mut self, packet: &P) {
        let mut body = Vec::new();
        encode_packet(packet, &mut body);
        let mut wire = Vec::new();
        self.encoder.encode(&body, &mut wire);
        self.stream.write_all(&wire).unwrap();
    }

    fn read_frame(&mut self) -> Option<Vec<u8>> {
        loop {
            if let Some(frame) = self.decoder.next_frame().unwrap() {
                return Some(frame);
            }
            let mut bytes = [0_u8; 8192];
            match self.stream.read(&mut bytes) {
                Ok(0) => return None,
                Ok(read) => self.decoder.feed(&bytes[..read]),
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    return None;
                }
                Err(error) => panic!("reading from the server: {error}"),
            }
        }
    }

    fn next_play(&mut self) -> Option<(i32, Vec<u8>)> {
        let bytes = self.read_frame()?;
        let mut body = bytes.as_slice();
        let id = VarInt::decode(&mut body).unwrap().0;
        let rest = body.to_vec();
        match id {
            73 => {
                let mut body = rest.as_slice();
                let position = PositionSync::decode(&mut body).unwrap();
                self.send(&ConfirmTeleport {
                    id: position.teleport_id,
                    x: position.x,
                    y: position.y,
                    z: position.z,
                    yaw: position.yaw,
                    pitch: position.pitch,
                });
            }
            11 => self.send(&ChunkBatchReceived {
                chunks_per_tick: 64.0,
            }),
            _ => {}
        }
        Some((id, rest))
    }

    fn until(&mut self, wanted: i32) -> Vec<u8> {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            assert!(
                Instant::now() < deadline,
                "timed out waiting for packet {wanted}"
            );
            if let Some((id, body)) = self.next_play()
                && id == wanted
            {
                return body;
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tick_function_changes_a_block_in_a_bot_session() {
    let dir = tempfile::tempdir().unwrap();
    let world_dir = dir.path().join("world");
    let functions = world_dir.join("datapacks/test/data/test/function");
    let tags = world_dir.join("datapacks/test/data/minecraft/tags/function");
    std::fs::create_dir_all(&functions).unwrap();
    std::fs::create_dir_all(&tags).unwrap();
    std::fs::write(
        functions.join("tick.mcfunction"),
        "# place a block above spawn\n\nsetblock 0 70 0 minecraft:stone\n",
    )
    .unwrap();
    std::fs::write(functions.join("load.mcfunction"), "say functions loaded\n").unwrap();
    std::fs::write(tags.join("tick.json"), r#"{"values":["test:tick"]}"#).unwrap();
    std::fs::write(tags.join("load.json"), r#"{"values":["test:load"]}"#).unwrap();

    ferrite_server::game::playerdata::init(dir.path().join("playerdata"));
    let registry =
        ferrite_world::RuntimeRegistry::load(repo_root().join("vanilla/26.3/generated")).unwrap();
    let source = ferrite_server::game::new_anvil_source(world_dir, registry);
    let world = ferrite_server::game::World::start_configured(
        Some(source.clone()),
        [0, 65, 0],
        ferrite_server::game::WorldSettings::default(),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = SocketAddr::from(([127, 0, 0, 1], listener.local_addr().unwrap().port()));
    let server = tokio::spawn(ferrite_net::serve(
        listener,
        ferrite_net::ServerConfig {
            online_mode: false,
            enforce_secure_profile: false,
            view_distance: 4,
            game: Some(world.sender.clone()),
            ..Default::default()
        },
    ));

    let stone = ferrite_data::blocks::default_state("minecraft:stone").unwrap();
    let observed = tokio::task::spawn_blocking(move || {
        let mut bot = Bot::join(addr, "Bot");
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            assert!(
                Instant::now() < deadline,
                "no block update from the tick function"
            );
            if let Some((id, body)) = bot.next_play()
                && id == BlockUpdate::ID
            {
                let update = BlockUpdate::decode(&mut body.as_slice()).unwrap();
                if BlockPos::unpack(update.location) == BlockPos::new(0, 70, 0) {
                    return update.block_state.0;
                }
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(observed, i32::from(stone));
    assert_eq!(
        source
            .block(0, 70, 0)
            .map(|state| u16::try_from(state).unwrap()),
        Some(stone)
    );
    world.shutdown().await.unwrap();
    server.abort();
}
