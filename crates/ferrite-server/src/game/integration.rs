use super::World;
use ferrite_net::{ServerConfig, serve};
use ferrite_protocol::{
    Decode, Encode, FrameDecoder, FrameEncoder, Packet, VarInt, encode_packet,
    packets::{
        configuration::{
            AcknowledgeFinishConfiguration, ClientboundKnownPacks, ServerboundKnownPacks,
        },
        handshake::{Intent, Intention},
        login::{LoginAcknowledged, LoginStart},
        play::{ChunkBatchReceived, ConfirmTeleport, MovePosition, PositionSync, SystemChat},
    },
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
struct Bot {
    stream: TcpStream,
    encoder: FrameEncoder,
    decoder: FrameDecoder,
    commands: Option<Vec<u8>>,
}
impl Bot {
    async fn send<P: Packet>(&mut self, p: &P) {
        let mut body = Vec::new();
        encode_packet(p, &mut body);
        let mut wire = Vec::new();
        self.encoder.encode(&body, &mut wire);
        self.stream.write_all(&wire).await.unwrap();
    }
    async fn raw(&mut self, id: i32, text: &str) {
        let mut body = Vec::new();
        VarInt(id).encode(&mut body);
        text.encode(&mut body);
        let mut wire = Vec::new();
        self.encoder.encode(&body, &mut wire);
        self.stream.write_all(&wire).await.unwrap();
    }
    async fn read(&mut self) -> Vec<u8> {
        tokio::time::timeout(std::time::Duration::from_secs(60), async {
            loop {
                if let Some(frame) = self.decoder.next_frame().unwrap() {
                    return frame;
                }
                let mut bytes = [0u8; 8192];
                let n = self.stream.read(&mut bytes).await.unwrap();
                assert_ne!(n, 0);
                self.decoder.feed(&bytes[..n]);
            }
        })
        .await
        .unwrap()
    }
    async fn next_play(&mut self) -> (i32, Vec<u8>) {
        let bytes = self.read().await;
        let mut body = bytes.as_slice();
        let id = VarInt::decode(&mut body).unwrap().0;
        let rest = body.to_vec();
        match id {
            16 => self.commands = Some(rest.clone()),
            73 => {
                let mut b = rest.as_slice();
                let p = PositionSync::decode(&mut b).unwrap();
                self.send(&ConfirmTeleport {
                    id: p.teleport_id,
                    x: p.x,
                    y: p.y,
                    z: p.z,
                    yaw: p.yaw,
                    pitch: p.pitch,
                })
                .await;
            }
            11 => {
                self.send(&ChunkBatchReceived {
                    chunks_per_tick: 64.0,
                })
                .await
            }
            _ => {}
        }
        (id, rest)
    }
    async fn until(&mut self, id: i32) -> Vec<u8> {
        loop {
            let (got, body) = self.next_play().await;
            if got == id {
                return body;
            }
        }
    }
    /// Waits for the next `SystemChat` whose text contains `needle`.
    async fn chat_containing(&mut self, needle: &str) -> String {
        tokio::time::timeout(std::time::Duration::from_secs(20), async {
            loop {
                let body = self.until(124).await;
                let message = SystemChat::decode(&mut body.as_slice()).unwrap();
                if message.content.0.contains(needle) {
                    return message.content.0;
                }
            }
        })
        .await
        .expect("expected chat message")
    }
    async fn join(addr: std::net::SocketAddr, name: &str) -> Self {
        let stream = TcpStream::connect(addr).await.unwrap();
        let mut b = Self {
            stream,
            encoder: FrameEncoder::default(),
            decoder: FrameDecoder::default(),
            commands: None,
        };
        b.send(&Intention {
            protocol_version: 777,
            server_address: "localhost".into(),
            server_port: addr.port(),
            intent: Intent::Login,
        })
        .await;
        b.send(&LoginStart {
            name: name.into(),
            uuid: uuid::Uuid::nil(),
        })
        .await;
        let first = b.read().await;
        assert_eq!(VarInt::decode(&mut first.as_slice()).unwrap().0, 3);
        b.encoder.set_compression(Some(256), 6);
        b.decoder.set_compression(Some(256));
        let login = b.read().await;
        assert_eq!(VarInt::decode(&mut login.as_slice()).unwrap().0, 2);
        b.send(&LoginAcknowledged).await;
        loop {
            let frame = b.read().await;
            let mut s = frame.as_slice();
            let id = VarInt::decode(&mut s).unwrap().0;
            if id == ClientboundKnownPacks::ID {
                let p = ClientboundKnownPacks::decode(&mut s).unwrap();
                b.send(&ServerboundKnownPacks { packs: p.packs }).await;
            }
            if id == 3 {
                break;
            }
        }
        b.send(&AcknowledgeFinishConfiguration).await;
        b.until(73).await;
        b
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_bots_move_stream_chat_and_disconnect() {
    let world = World::start();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(serve(
        listener,
        ServerConfig {
            online_mode: false,
            view_distance: 2,
            game: Some(world.sender.clone()),
            ..Default::default()
        },
    ));
    let mut a = Bot::join(addr, "Alice").await;
    let mut b = Bot::join(addr, "Bob").await;
    loop {
        let body = a.until(46).await;
        if i32::decode(&mut body.as_slice()).unwrap() < 0 {
            break;
        }
    }
    b.until(1).await;
    a.send(&MovePosition {
        x: 5.5,
        y: 65.0,
        z: 0.5,
        flags: 1,
    })
    .await;
    b.until(54).await;
    a.send(&MovePosition {
        x: 40.5,
        y: 65.0,
        z: 0.5,
        flags: 1,
    })
    .await;
    let mut forget = false;
    let mut new_chunk = false;
    while !(forget && new_chunk) {
        let (id, body) = a.next_play().await;
        if id == 38 {
            forget = true;
        }
        if id == 46 {
            let mut s = body.as_slice();
            let x = i32::decode(&mut s).unwrap();
            let _z = i32::decode(&mut s).unwrap();
            if x >= 3 {
                new_chunk = true;
            }
        }
    }
    a.raw(9, "hello from Alice").await;
    loop {
        let body = b.until(124).await;
        let mut s = body.as_slice();
        let msg = SystemChat::decode(&mut s).unwrap();
        if msg.content.0 == "<Alice> hello from Alice" {
            break;
        }
    }
    drop(a);
    let mut remove_info = false;
    let mut remove_entity = false;
    while !(remove_info && remove_entity) {
        let (id, _) = b.next_play().await;
        remove_info |= id == 70;
        remove_entity |= id == 78;
    }
    drop(b);
    world.shutdown().await.unwrap();
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn duplicate_login_kicks_old_session_and_keeps_new_session() {
    let world = World::start();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(serve(
        listener,
        ServerConfig {
            online_mode: false,
            view_distance: 2,
            game: Some(world.sender.clone()),
            ..Default::default()
        },
    ));
    let mut old = Bot::join(addr, "Alice").await;
    let mut new = Bot::join(addr, "Alice").await;
    let body = old
        .until(ferrite_protocol::packets::play::PlayDisconnect::ID)
        .await;
    let reason =
        ferrite_protocol::packets::play::PlayDisconnect::decode(&mut body.as_slice()).unwrap();
    assert_eq!(reason.reason.0, "You logged in from another location");
    new.raw(9, "still here").await;
    loop {
        let body = new.until(124).await;
        let msg = SystemChat::decode(&mut body.as_slice()).unwrap();
        if msg.content.0 == "<Alice> still here" {
            break;
        }
    }
    drop(new);
    world.shutdown().await.unwrap();
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn crossing_region_boundary_keeps_chat_and_movement_routed() {
    let world = World::start();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(serve(
        listener,
        ServerConfig {
            online_mode: false,
            view_distance: 2,
            game: Some(world.sender.clone()),
            ..Default::default()
        },
    ));
    let mut bot = Bot::join(addr, "Walker").await;
    for x in [80.5, 160.5, 240.5, 260.5, 300.5] {
        bot.send(&MovePosition {
            x,
            y: 65.0,
            z: 0.5,
            flags: 1,
        })
        .await;
        bot.raw(9, "walking").await;
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    }
    bot.raw(9, "across").await;
    let mut reached = false;
    let mut chatted = false;
    while !(reached && chatted) {
        let (id, body) = bot.next_play().await;
        if id == 96 {
            let mut body = body.as_slice();
            reached |= VarInt::decode(&mut body).unwrap().0 == 18;
        }
        if id == 124 {
            let message = SystemChat::decode(&mut body.as_slice()).unwrap();
            chatted |= message.content.0 == "<Walker> across";
        }
    }
    drop(bot);
    world.shutdown().await.unwrap();
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fixture_world_streams_disk_chunk() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let path = root.join("vanilla/26.3/fixture-world/world");
    let registry =
        ferrite_world::RuntimeRegistry::load(root.join("vanilla/26.3/generated")).unwrap();
    let expected = super::anvil::read_chunk(&path, &registry, 0, 7)
        .unwrap()
        .unwrap();
    let disk = ferrite_world::RegionFile::open(
        path.join("dimensions/minecraft/overworld/region/r.0.0.mca"),
        0,
        0,
    )
    .unwrap()
    .read_chunk(0, 7)
    .unwrap()
    .unwrap();
    let (_, tag) = ferrite_nbt::binary::read_named(&disk).unwrap();
    let chunk = ferrite_world::chunk_from_nbt(&tag, -64, 384, &registry).unwrap();
    let disk_block = chunk.sections()[8].blocks().get(4 * 256).unwrap();
    let spawn = super::anvil::read_spawn(&path).unwrap();
    let source = super::anvil::AnvilChunkSource::new(path, registry);
    let world = World::start_with(Some(source), spawn);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(serve(
        listener,
        ServerConfig {
            online_mode: false,
            view_distance: 2,
            game: Some(world.sender.clone()),
            ..Default::default()
        },
    ));
    let mut bot = Bot::join(addr, "Reader").await;
    let actual = bot.until(46).await;
    assert_eq!(actual, expected);
    let mut body = actual.as_slice();
    assert_eq!(i32::decode(&mut body).unwrap(), 0);
    assert_eq!(i32::decode(&mut body).unwrap(), 7);
    let _ = Vec::<ferrite_protocol::packets::play::Heightmap>::decode(&mut body).unwrap();
    let sections = Vec::<u8>::decode(&mut body).unwrap();
    let mut data = sections.as_slice();
    let mut actual_block = 0;
    for section in 0..=8 {
        let _ = i16::decode(&mut data).unwrap();
        let _ = i16::decode(&mut data).unwrap();
        let block = read_palette_value(&mut data, 4096, 8, 4 * 256);
        let _ = read_palette_value(&mut data, 64, 3, 0);
        if section == 8 {
            actual_block = block;
        }
    }
    assert_eq!(actual_block, disk_block);
    drop(bot);
    world.shutdown().await.unwrap();
    server.abort();
}

fn read_palette_value(data: &mut &[u8], entries: usize, max_local_bits: u8, index: usize) -> u32 {
    let bits = u8::decode(data).unwrap();
    if bits == 0 {
        return VarInt::decode(data).unwrap().0 as u32;
    }
    let palette = if bits <= max_local_bits {
        let n = VarInt::decode(data).unwrap().0 as usize;
        (0..n)
            .map(|_| VarInt::decode(data).unwrap().0 as u32)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let per = 64 / bits as usize;
    let mut value = 0;
    for i in 0..entries.div_ceil(per) {
        let word = i64::decode(data).unwrap() as u64;
        if i == index / per {
            value = ((word >> ((index % per) * bits as usize)) & ((1u64 << bits) - 1)) as usize;
        }
    }
    if palette.is_empty() {
        value as u32
    } else {
        palette[value]
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn operator_teleport_command_moves_player() {
    let world = World::start_configured(
        None,
        [0, 65, 0],
        super::WorldSettings {
            gamemode: 1,
            ops: vec![super::Operator {
                uuid: uuid::Uuid::parse_str("10920508-d5d8-3eed-93d2-92f193afe7d7").unwrap(),
                level: 4,
            }],
            ..Default::default()
        },
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(serve(
        listener,
        ServerConfig {
            online_mode: false,
            view_distance: 2,
            game: Some(world.sender.clone()),
            ..Default::default()
        },
    ));
    let mut bot = Bot::join(addr, "Alice").await;
    bot.raw(7, "tp 10 70 20").await;
    let body = bot.until(73).await;
    let p = PositionSync::decode(&mut body.as_slice()).unwrap();
    // Vec3 centers absolute integer x/z on the block center, like vanilla.
    assert_eq!([p.x, p.y, p.z], [10.5, 70.0, 20.5]);
    drop(bot);
    world.shutdown().await.unwrap();
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn creative_slot_updates_player_inventory() {
    use ferrite_protocol::packets::play::{ContainerSlot, CreativeSlot, ItemStack};
    let world = World::start();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(serve(
        listener,
        ServerConfig {
            online_mode: false,
            view_distance: 2,
            game: Some(world.sender.clone()),
            ..Default::default()
        },
    ));
    let mut bot = Bot::join(addr, "Alice").await;
    bot.send(&CreativeSlot {
        slot: 36,
        item: ItemStack {
            count: 3,
            item_id: 1,
        },
    })
    .await;
    let body = tokio::time::timeout(std::time::Duration::from_secs(3), bot.until(20))
        .await
        .unwrap();
    let slot = ContainerSlot::decode(&mut body.as_slice()).unwrap();
    assert_eq!(slot.slot, 36);
    assert_eq!(
        slot.item,
        ItemStack {
            count: 3,
            item_id: 1
        }
    );
    drop(bot);
    world.shutdown().await.unwrap();
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn creative_break_broadcasts_to_tracking_player() {
    use ferrite_protocol::{
        packets::play::{BlockUpdate, MoveRotation, PlayerAction},
        types::BlockPos,
    };
    let world = World::start();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(serve(
        listener,
        ServerConfig {
            online_mode: false,
            view_distance: 2,
            game: Some(world.sender.clone()),
            ..Default::default()
        },
    ));
    let mut miner = Bot::join(addr, "Miner").await;
    let mut observer = Bot::join(addr, "Observer").await;
    observer.until(46).await;
    miner
        .send(&MoveRotation {
            yaw: 0.0,
            pitch: 90.0,
            flags: 1,
        })
        .await;
    miner
        .send(&PlayerAction {
            status: VarInt(0),
            location: BlockPos::new(0, 64, 0).pack(),
            face: 1,
            sequence: VarInt(8),
        })
        .await;
    let body = tokio::time::timeout(std::time::Duration::from_secs(20), observer.until(8))
        .await
        .unwrap();
    let update = BlockUpdate::decode(&mut body.as_slice()).unwrap();
    assert_eq!(BlockPos::unpack(update.location), BlockPos::new(0, 64, 0));
    assert_eq!(update.block_state.0, 0);
    let mut newcomer = Bot::join(addr, "Newcomer").await;
    let chunk = newcomer.until(46).await;
    let mut body = chunk.as_slice();
    assert_eq!(i32::decode(&mut body).unwrap(), 0);
    assert_eq!(i32::decode(&mut body).unwrap(), 0);
    let _ = Vec::<ferrite_protocol::packets::play::Heightmap>::decode(&mut body).unwrap();
    let sections = Vec::<u8>::decode(&mut body).unwrap();
    let mut data = sections.as_slice();
    for section in 0..=8 {
        let _ = i16::decode(&mut data).unwrap();
        let _ = i16::decode(&mut data).unwrap();
        let block = read_palette_value(&mut data, 4096, 8, 0);
        let _ = read_palette_value(&mut data, 64, 3, 0);
        if section == 8 {
            assert_eq!(block, 0);
        }
    }
    drop(newcomer);
    drop(miner);
    drop(observer);
    world.shutdown().await.unwrap();
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn interaction_reach_and_placement() {
    use ferrite_protocol::{
        packets::play::{CreativeSlot, ItemStack, MoveRotation, PlayerAction, UseItemOn},
        types::BlockPos,
    };
    let source = super::anvil::AnvilChunkSource::new_memory();
    let world = World::start_with(Some(source.clone()), [0, 65, 0]);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(serve(
        listener,
        ServerConfig {
            online_mode: false,
            view_distance: 2,
            game: Some(world.sender.clone()),
            ..Default::default()
        },
    ));
    let mut bot = Bot::join(addr, "Builder").await;
    bot.until(46).await;
    bot.send(&MoveRotation {
        yaw: 0.0,
        pitch: 90.0,
        flags: 1,
    })
    .await;
    bot.send(&PlayerAction {
        status: VarInt(0),
        location: BlockPos::new(10, 64, 0).pack(),
        face: 1,
        sequence: VarInt(1),
    })
    .await;
    bot.until(4).await;
    assert_eq!(source.block(10, 64, 0), Some(9));
    bot.send(&PlayerAction {
        status: VarInt(0),
        location: BlockPos::new(0, 64, 0).pack(),
        face: 1,
        sequence: VarInt(2),
    })
    .await;
    bot.until(4).await;
    assert_eq!(source.block(0, 64, 0), Some(0));
    let stone = ferrite_data::registries::id("minecraft:item", "minecraft:stone").unwrap();
    bot.send(&CreativeSlot {
        slot: 36,
        item: ItemStack {
            count: 1,
            item_id: stone,
        },
    })
    .await;
    bot.send(&UseItemOn {
        hand: VarInt(0),
        location: BlockPos::new(0, 63, 0).pack(),
        face: VarInt(1),
        cursor_x: 0.5,
        cursor_y: 1.0,
        cursor_z: 0.5,
        inside: false,
        world_border_hit: false,
        sequence: VarInt(3),
    })
    .await;
    bot.until(4).await;
    assert_eq!(
        source.block(0, 64, 0),
        Some(u32::from(
            ferrite_data::blocks::default_state("minecraft:stone").unwrap()
        ))
    );
    bot.send(&UseItemOn {
        hand: VarInt(0),
        location: BlockPos::new(0, 64, 0).pack(),
        face: VarInt(1),
        cursor_x: 0.5,
        cursor_y: 1.0,
        cursor_z: 0.5,
        inside: false,
        world_border_hit: false,
        sequence: VarInt(4),
    })
    .await;
    bot.until(4).await;
    assert_eq!(source.block(0, 65, 0), Some(0));
    drop(bot);
    world.shutdown().await.unwrap();
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn survival_break_requires_elapsed_ticks() {
    use ferrite_protocol::{
        packets::play::{MoveRotation, PlayerAction},
        types::BlockPos,
    };
    let source = super::anvil::AnvilChunkSource::new_memory();
    let world = World::start_configured(
        Some(source.clone()),
        [0, 65, 0],
        super::WorldSettings {
            gamemode: 0,
            ops: vec![],
            ..Default::default()
        },
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(serve(
        listener,
        ServerConfig {
            online_mode: false,
            view_distance: 2,
            game: Some(world.sender.clone()),
            ..Default::default()
        },
    ));
    let mut bot = Bot::join(addr, "Miner").await;
    bot.until(46).await;
    bot.send(&MoveRotation {
        yaw: 0.0,
        pitch: 90.0,
        flags: 1,
    })
    .await;
    let location = BlockPos::new(0, 64, 0).pack();
    bot.send(&PlayerAction {
        status: VarInt(0),
        location,
        face: 1,
        sequence: VarInt(1),
    })
    .await;
    bot.send(&PlayerAction {
        status: VarInt(2),
        location,
        face: 1,
        sequence: VarInt(2),
    })
    .await;
    bot.until(4).await;
    bot.until(4).await;
    assert_ne!(source.block(0, 64, 0), Some(0));
    bot.send(&PlayerAction {
        status: VarInt(0),
        location,
        face: 1,
        sequence: VarInt(3),
    })
    .await;
    bot.until(4).await;
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    bot.send(&PlayerAction {
        status: VarInt(2),
        location,
        face: 1,
        sequence: VarInt(4),
    })
    .await;
    bot.until(4).await;
    assert_eq!(source.block(0, 64, 0), Some(0));
    drop(bot);
    world.shutdown().await.unwrap();
    server.abort();
}

#[tokio::test]
async fn console_stop_is_dispatched() {
    let world = World::start();
    assert!(world.console_command("/stop".into()).await.unwrap());
    assert!(world.console_command("unknown".into()).await.is_err());
    world.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn message_command_reaches_target_and_sender() {
    let world = World::start();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(serve(
        listener,
        ServerConfig {
            online_mode: false,
            view_distance: 2,
            game: Some(world.sender.clone()),
            ..Default::default()
        },
    ));
    let mut alice = Bot::join(addr, "Alice").await;
    let mut bob = Bot::join(addr, "Bob").await;
    alice.until(46).await;
    bob.until(46).await;

    alice.raw(7, "msg Bob hello there").await;
    assert_eq!(
        bob.chat_containing("Alice whispers to you: hello there")
            .await,
        "Alice whispers to you: hello there"
    );
    assert_eq!(
        alice
            .chat_containing("You whisper to Bob: hello there")
            .await,
        "You whisper to Bob: hello there"
    );

    drop((alice, bob));
    world.shutdown().await.unwrap();
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn kill_command_disconnects_the_target() {
    use ferrite_protocol::packets::play::PlayDisconnect;

    let world = World::start_configured(
        None,
        [0, 65, 0],
        super::WorldSettings {
            gamemode: 1,
            ops: vec![super::Operator {
                uuid: uuid::Uuid::parse_str("10920508-d5d8-3eed-93d2-92f193afe7d7").unwrap(),
                level: 4,
            }],
            ..Default::default()
        },
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(serve(
        listener,
        ServerConfig {
            online_mode: false,
            view_distance: 2,
            game: Some(world.sender.clone()),
            ..Default::default()
        },
    ));
    let mut alice = Bot::join(addr, "Alice").await;
    let mut bob = Bot::join(addr, "Bob").await;
    alice.until(46).await;
    bob.until(46).await;

    alice.raw(7, "kill Bob").await;
    let body = bob.until(32).await;
    let disconnect = PlayDisconnect::decode(&mut body.as_slice()).unwrap();
    assert_eq!(disconnect.reason.0, "You died!");
    alice.chat_containing("Killed Bob").await;

    drop((alice, bob));
    world.shutdown().await.unwrap();
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn operator_stop_shuts_the_world_down() {
    let world = World::start_configured(
        None,
        [0, 65, 0],
        super::WorldSettings {
            ops: vec![super::Operator {
                uuid: uuid::Uuid::parse_str("10920508-d5d8-3eed-93d2-92f193afe7d7").unwrap(),
                level: 4,
            }],
            ..Default::default()
        },
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(serve(
        listener,
        ServerConfig {
            online_mode: false,
            view_distance: 2,
            game: Some(world.sender.clone()),
            ..Default::default()
        },
    ));
    let mut bot = Bot::join(addr, "Alice").await;
    bot.until(46).await;
    bot.raw(7, "stop").await;
    tokio::time::timeout(std::time::Duration::from_secs(15), world.shutdown())
        .await
        .expect("world stops after /stop")
        .unwrap();
    server.abort();
}

#[tokio::test]
async fn admin_commands_persist_to_the_lists() {
    use ferrite_admin::AdminLists;

    let dir = std::env::temp_dir().join(format!("ferrite-admin-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let lists = std::sync::Arc::new(AdminLists::load(&dir).unwrap());
    let alice = uuid::Uuid::parse_str("10920508-d5d8-3eed-93d2-92f193afe7d7").unwrap();
    let bob = uuid::Uuid::parse_str("22e0d6a0-1f4a-3a9e-8a2e-8f7a5d9d3b0e").unwrap();
    lists.usercache.cache("Alice", alice);
    lists.usercache.cache("Bob", bob);
    lists.flush().unwrap();

    let world = World::start_configured(
        None,
        [0, 65, 0],
        super::WorldSettings {
            admin: Some(lists.clone()),
            ..Default::default()
        },
    );
    for command in [
        "op Alice",
        "whitelist add Bob",
        "whitelist remove Bob",
        "whitelist add Bob",
        "ban Bob griefing",
        "banlist players",
        "ban-ip 10.0.0.1",
        "banlist ips",
        "pardon-ip 10.0.0.1",
        "pardon Bob",
        "whitelist on",
        "whitelist off",
        "save-all",
        "save-off",
        "save-on",
        "kick Ghost",
        "deop Alice",
        "whitelist reload",
    ] {
        world
            .console_command(command.into())
            .await
            .unwrap_or_else(|error| panic!("{command}: {error}"));
    }

    assert_eq!(lists.ops.level_of(alice), None);
    assert!(lists.bans.player_bans().is_empty());
    assert!(!lists.bans.is_ip_banned("10.0.0.1"));
    // The reload reads whitelist.json back; Bob was added before it.
    let reloaded = AdminLists::load(&dir).unwrap();
    assert!(reloaded.whitelist.contains(bob));
    world.shutdown().await.unwrap();
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bot_break_is_saved_on_shutdown() {
    use ferrite_protocol::{
        packets::play::{MoveRotation, PlayerAction},
        types::BlockPos,
    };
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let registry =
        ferrite_world::RuntimeRegistry::load(root.join("vanilla/26.3/generated")).unwrap();
    let temp = std::env::temp_dir().join(format!("ferrite-bot-save-{}", uuid::Uuid::new_v4()));
    let source = super::anvil::AnvilChunkSource::new(temp.clone(), registry);
    let world = World::start_with(Some(source), [0, 65, 0]);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(serve(
        listener,
        ServerConfig {
            online_mode: false,
            view_distance: 2,
            game: Some(world.sender.clone()),
            ..Default::default()
        },
    ));
    let mut bot = Bot::join(addr, "Saver").await;
    bot.until(46).await;
    bot.send(&MoveRotation {
        yaw: 0.0,
        pitch: 90.0,
        flags: 1,
    })
    .await;
    bot.send(&PlayerAction {
        status: VarInt(0),
        location: BlockPos::new(0, 64, 0).pack(),
        face: 1,
        sequence: VarInt(1),
    })
    .await;
    bot.until(4).await;
    drop(bot);
    world.shutdown().await.unwrap();
    server.abort();
    let registry =
        ferrite_world::RuntimeRegistry::load(root.join("vanilla/26.3/generated")).unwrap();
    let region = ferrite_world::RegionFile::open(
        temp.join("dimensions/minecraft/overworld/region/r.0.0.mca"),
        0,
        0,
    )
    .unwrap();
    let bytes = region.read_chunk(0, 0).unwrap().unwrap();
    let (_, tag) = ferrite_nbt::binary::read_named(&bytes).unwrap();
    let chunk = ferrite_world::chunk_from_nbt(&tag, -64, 384, &registry).unwrap();
    assert_eq!(chunk.block(0, 64, 0), Some(0));
    std::fs::remove_dir_all(temp).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn commands_packet_decodes_and_filters_permissions() {
    let world = World::start();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(serve(
        listener,
        ServerConfig {
            online_mode: false,
            view_distance: 2,
            game: Some(world.sender.clone()),
            ..Default::default()
        },
    ));
    let bot = Bot::join(addr, "Guest").await;
    let bytes = bot.commands.as_ref().expect("commands packet");
    let mut input = bytes.as_slice();
    // The guest sees the unrestricted root commands and the argument nodes under `msg`.
    assert_eq!(VarInt::decode(&mut input).unwrap().0, 8);
    assert_eq!(u8::decode(&mut input).unwrap(), 0);
    assert_eq!(
        Vec::<VarInt>::decode(&mut input).unwrap(),
        (1..=5).map(VarInt).collect::<Vec<_>>()
    );
    let mut names = Vec::new();
    for _ in 0..7 {
        let flags = u8::decode(&mut input).unwrap();
        let children = Vec::<VarInt>::decode(&mut input).unwrap();
        let redirect = if flags & 0x08 != 0 {
            Some(VarInt::decode(&mut input).unwrap().0)
        } else {
            None
        };
        let name = String::decode(&mut input).unwrap();
        if flags & 3 == 2 {
            let parser = VarInt::decode(&mut input).unwrap().0;
            match parser {
                6 => {
                    // minecraft:entity properties: single, players-only.
                    assert_eq!(u8::decode(&mut input).unwrap(), 2);
                }
                20 => {}
                other => panic!("unexpected parser {other} for {name}"),
            }
        }
        if matches!(name.as_str(), "tps" | "mspt" | "tell" | "w") {
            assert!(children.is_empty(), "literal {name}");
        }
        if name == "tell" || name == "w" {
            assert_eq!(redirect, Some(1), "{name} redirects to msg");
        } else {
            assert_eq!(redirect, None, "{name}");
        }
        names.push(name);
    }
    assert_eq!(
        names,
        ["msg", "tell", "w", "tps", "mspt", "targets", "message"]
    );
    assert_eq!(VarInt::decode(&mut input).unwrap().0, 0);
    assert!(input.is_empty());
    drop(bot);
    world.shutdown().await.unwrap();
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tps_and_mspt_commands_report_live_tick_statistics() {
    let world = World::start();
    let profiler = world.profiler();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(serve(
        listener,
        ServerConfig {
            online_mode: false,
            view_distance: 2,
            game: Some(world.sender.clone()),
            ..Default::default()
        },
    ));
    let mut bot = Bot::join(addr, "Profiler").await;
    bot.until(46).await;
    assert!(profiler.monitor().ticks() > 0, "global tick was recorded");
    let region_slots = (0..profiler.monitor().regions() as u32)
        .filter(|slot| profiler.monitor().region_ticks(*slot).unwrap_or(0) > 0)
        .count();
    assert!(region_slots > 0, "a region tick was recorded");

    bot.raw(7, "tps").await;
    let global = bot.chat_containing("TPS from last 1m, 5m, 15m:").await;
    assert!(global.starts_with("§6TPS"), "{global}");
    let region = bot.chat_containing("Region ").await;
    assert!(
        region.contains("Region 0 TPS from last 5s, 1m, 5m:"),
        "{region}"
    );

    bot.raw(7, "mspt").await;
    let ten_seconds = bot.chat_containing("MSPT from last 10s:").await;
    assert!(ten_seconds.contains("avg "), "{ten_seconds}");
    let minute = bot.chat_containing("MSPT from last 1m:").await;
    assert!(minute.contains("p99 "), "{minute}");

    drop(bot);
    world.shutdown().await.unwrap();
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn profile_start_and_stop_collect_tick_scopes() {
    let world = World::start();
    let profiler = world.profiler();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(serve(
        listener,
        ServerConfig {
            online_mode: false,
            view_distance: 2,
            game: Some(world.sender.clone()),
            ..Default::default()
        },
    ));
    let bot = Bot::join(addr, "Profiler").await;

    assert!(!profiler.profile_running());
    world.console_command("profile start".into()).await.unwrap();
    assert!(profiler.profile_running());
    // Starting twice is refused, but the console command itself still runs.
    world.console_command("profile start".into()).await.unwrap();
    assert!(profiler.profile_running());

    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    world.console_command("profile stop".into()).await.unwrap();
    assert!(!profiler.profile_running());

    let report = profiler.timings().report_all(50.0);
    let paths: Vec<&str> = report.rows.iter().map(|row| row.path.as_str()).collect();
    assert!(paths.contains(&"region-tick"), "{paths:?}");
    assert!(paths.contains(&"global-tick"), "{paths:?}");
    assert!(paths.contains(&"global-tick/entity-tracking"), "{paths:?}");

    drop(bot);
    world.shutdown().await.unwrap();
    server.abort();
}
