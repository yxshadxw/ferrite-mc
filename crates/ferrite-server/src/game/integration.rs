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
        play::{
            ChunkBatchReceived, ClientboundPlayerChat, ConfirmTeleport, LastSeenUpdate,
            MovePosition, PositionSync, ServerboundChat, SystemChat,
        },
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
    /// Chunks per tick sent in automatic `ChunkBatchReceived` acknowledgements.
    chunks_per_tick: f32,
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
    async fn chat(&mut self, text: &str) {
        self.send(&ServerboundChat {
            message: text.into(),
            timestamp: 1_700_000_000_000,
            salt: 0,
            signature: None,
            last_seen: LastSeenUpdate::new(0, &[], 0),
        })
        .await;
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
                    chunks_per_tick: self.chunks_per_tick,
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
    /// Completes login and configuration but stops before the play position packet.
    async fn begin_join(addr: std::net::SocketAddr, name: &str) -> Self {
        let stream = TcpStream::connect(addr).await.unwrap();
        let mut b = Self {
            stream,
            encoder: FrameEncoder::default(),
            decoder: FrameDecoder::default(),
            commands: None,
            chunks_per_tick: 64.0,
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
        b
    }
    /// Joins and waits until the server positions the player in play state.
    async fn join(addr: std::net::SocketAddr, name: &str) -> Self {
        Self::join_with_rate(addr, name, 64.0).await
    }
    /// Joins with a specific chunk-batch rate, so a test can keep the per-connection queue
    /// small without a normal reader overflowing it during the initial chunk burst.
    async fn join_with_rate(addr: std::net::SocketAddr, name: &str, chunks_per_tick: f32) -> Self {
        let mut b = Self::begin_join(addr, name).await;
        b.chunks_per_tick = chunks_per_tick;
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
    a.chat("hello from Alice").await;
    let body = b.until(ClientboundPlayerChat::ID).await;
    let msg = ClientboundPlayerChat::decode(&mut body.as_slice()).unwrap();
    assert_eq!(msg.body.content, "hello from Alice");
    assert!(msg.signature.is_none());
    assert!(msg.body.last_seen.is_empty());
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
async fn offline_chat_broadcasts_unsigned_player_chat_to_others() {
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
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let before = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    alice.chat("hello offline").await;

    // Both the sender and the other player receive a vanilla-shaped unsigned player_chat.
    for bot in [&mut bob, &mut alice] {
        let body = bot.until(ClientboundPlayerChat::ID).await;
        let msg = ClientboundPlayerChat::decode(&mut body.as_slice()).unwrap();
        assert_eq!(msg.body.content, "hello offline");
        assert!(!msg.sender.is_nil()); // offline-mode UUID derived from the name
        assert_eq!(msg.index.0, 0);
        assert!(msg.signature.is_none());
        assert!(msg.unsigned_content.is_none());
        assert!(msg.body.timestamp >= before);
        assert_eq!(msg.body.salt, 0);
        assert!(msg.body.last_seen.is_empty());
        assert_eq!(msg.chat_type.chat_type.0, 0);
        assert_eq!(msg.chat_type.name.0, "Alice");
        assert!(msg.chat_type.target_name.is_none());
    }
    drop(alice);
    drop(bob);
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
    new.chat("still here").await;
    let body = new.until(ClientboundPlayerChat::ID).await;
    let msg = ClientboundPlayerChat::decode(&mut body.as_slice()).unwrap();
    assert_eq!(msg.body.content, "still here");
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
        bot.chat("walking").await;
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    }
    bot.chat("across").await;
    let mut reached = false;
    let mut chatted = false;
    while !(reached && chatted) {
        let (id, body) = bot.next_play().await;
        if id == 96 {
            let mut body = body.as_slice();
            reached |= VarInt::decode(&mut body).unwrap().0 == 18;
        }
        if id == ClientboundPlayerChat::ID {
            let message = ClientboundPlayerChat::decode(&mut body.as_slice()).unwrap();
            chatted |= message.body.content == "across";
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
            max_players: 20,
            ops: vec![super::Operator {
                uuid: uuid::Uuid::parse_str("10920508-d5d8-3eed-93d2-92f193afe7d7").unwrap(),
                level: 4,
            }],
            plugins: None,
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
    bot.send(&ferrite_protocol::packets::play::ServerboundChatCommand {
        command: "tp 10 70 20".into(),
    })
    .await;
    let body = bot.until(73).await;
    let p = PositionSync::decode(&mut body.as_slice()).unwrap();
    // Vec3 centers absolute integer x/z on the block center, like vanilla.
    assert_eq!([p.x, p.y, p.z], [10.5, 70.0, 20.5]);

    bot.raw(7, "tp @s ~ ~5 ~ rotation 90 10").await;
    let body = bot.until(73).await;
    let p = PositionSync::decode(&mut body.as_slice()).unwrap();
    assert_eq!([p.x, p.y, p.z], [10.5, 75.0, 20.5]);
    assert_eq!((p.yaw, p.pitch), (90.0, 10.0));

    bot.raw(7, "tp @s 10 70 20 facing 10 70 30").await;
    let body = bot.until(73).await;
    let p = PositionSync::decode(&mut body.as_slice()).unwrap();
    assert!((p.yaw - 0.0).abs() < 1e-4, "yaw {}", p.yaw);
    assert!(p.pitch > 0.0, "looks down at the facing point: {}", p.pitch);

    drop(bot);
    world.shutdown().await.unwrap();
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn creative_slot_updates_player_inventory() {
    use ferrite_protocol::packets::play::{CreativeSlot, ItemStack};
    let (world, addr, server) = op_world(None, 1, &[]).await;
    let mut bot = join_with_inventory(addr, "Creator").await;
    let stack = ItemStack {
        count: 3,
        item_id: 1,
    };
    bot.send(&CreativeSlot {
        slot: 36,
        item: stack,
    })
    .await;
    // The client already shows the stack, so (like vanilla) nothing is echoed; the server's
    // full view has it.
    assert_eq!(inventory_of(&mut bot).await[36], stack);
    // Out-of-range slots and oversized stacks are ignored.
    for (slot, count) in [(0, 1), (46, 1), (37, 65)] {
        bot.send(&CreativeSlot {
            slot,
            item: ItemStack { count, item_id: 1 },
        })
        .await;
    }
    let slots = inventory_of(&mut bot).await;
    assert_eq!(slots.iter().filter(|s| s.count > 0).count(), 1);
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
            max_players: 20,
            ops: vec![],
            plugins: None,
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
async fn kill_command_shows_the_death_screen_even_in_creative() {
    use ferrite_protocol::packets::play::{
        PlayerCombatKill, Respawn, ServerboundClientCommand, SetHealth,
    };

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
    // `DamageTypes.GENERIC_KILL` is in `bypasses_invulnerability`, so creative mode dies too;
    // the client gets `ClientboundPlayerCombatKillPacket`, not a disconnect.
    let body = bob.until(PlayerCombatKill::ID).await;
    let kill = PlayerCombatKill::decode(&mut body.as_slice()).unwrap();
    assert_eq!(kill.message.0, "Bob was killed");
    let body = bob.until(SetHealth::ID).await;
    let health = SetHealth::decode(&mut body.as_slice()).unwrap();
    assert_eq!(health.health, 0.0);
    // The connection survives death: a respawn request is answered.
    bob.send(&ServerboundClientCommand { action: VarInt(0) })
        .await;
    bob.until(Respawn::ID).await;
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
        "op Ghost",
        "deop Ghost",
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
    assert_eq!(
        lists.ops.level_of(super::commands::offline_uuid("Ghost")),
        None
    );
    assert!(lists.bans.player_bans().is_empty());
    assert!(!lists.bans.is_ip_banned("10.0.0.1"));
    // The reload reads whitelist.json back; Bob was added before it.
    let reloaded = AdminLists::load(&dir).unwrap();
    assert!(reloaded.whitelist.contains(bob));
    world.shutdown().await.unwrap();
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn banned_player_is_disconnected_before_play() {
    use ferrite_admin::AdminLists;
    use ferrite_protocol::packets::play::PlayDisconnect;

    let dir = std::env::temp_dir().join(format!("ferrite-ban-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let lists = std::sync::Arc::new(AdminLists::load(&dir).unwrap());
    let alice = uuid::Uuid::parse_str("10920508-d5d8-3eed-93d2-92f193afe7d7").unwrap();
    lists.bans.ban(alice, "Alice", "griefing", "Server", None);
    lists.flush().unwrap();

    let world = World::start_configured(
        None,
        [0, 65, 0],
        super::WorldSettings {
            admin: Some(lists),
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
    let mut bot = Bot::begin_join(addr, "Alice").await;
    let body = bot.until(32).await;
    let disconnect = PlayDisconnect::decode(&mut body.as_slice()).unwrap();
    assert_eq!(
        disconnect.reason.0,
        "You are banned from this server!\nReason: griefing"
    );

    drop(bot);
    world.shutdown().await.unwrap();
    server.abort();
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ip_banned_player_is_disconnected_before_play() {
    use ferrite_admin::AdminLists;
    use ferrite_protocol::packets::play::PlayDisconnect;

    let dir = std::env::temp_dir().join(format!("ferrite-ban-ip-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let lists = std::sync::Arc::new(AdminLists::load(&dir).unwrap());
    lists.bans.ban_ip("127.0.0.1", "proxies", "Server", None);
    lists.flush().unwrap();

    let world = World::start_configured(
        None,
        [0, 65, 0],
        super::WorldSettings {
            admin: Some(lists),
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
    let mut bot = Bot::begin_join(addr, "Bob").await;
    let body = bot.until(32).await;
    let disconnect = PlayDisconnect::decode(&mut body.as_slice()).unwrap();
    assert_eq!(
        disconnect.reason.0,
        "Your IP address is banned from this server!\nReason: proxies"
    );

    drop(bot);
    world.shutdown().await.unwrap();
    server.abort();
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
    let node_count = VarInt::decode(&mut input).unwrap().0 as usize;
    assert!(node_count >= 8, "guest tree has {node_count} nodes");
    assert_eq!(u8::decode(&mut input).unwrap(), 0, "root flags");
    let roots = Vec::<VarInt>::decode(&mut input).unwrap();
    assert!(!roots.is_empty());
    let registry = ferrite_command::ParserRegistry::load(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../vanilla/26.3/generated/reports"),
    )
    .unwrap();
    // The root node's flags and children were decoded above; the rest follow it.
    let mut nodes = vec![(String::new(), 0, roots, None, None)];
    for _ in 1..node_count {
        let flags = u8::decode(&mut input).unwrap();
        let children = Vec::<VarInt>::decode(&mut input).unwrap();
        let redirect = if flags & 0x08 != 0 {
            Some(VarInt::decode(&mut input).unwrap().0)
        } else {
            None
        };
        let name = String::decode(&mut input).unwrap();
        let mut parser = None;
        if flags & 3 == 2 {
            let id = VarInt::decode(&mut input).unwrap().0;
            if Some(id) == registry.id("minecraft:entity") {
                // minecraft:entity properties: single/players-only flags.
                let _ = u8::decode(&mut input).unwrap();
            }
            parser = Some(id);
        }
        nodes.push((name, flags & 3, children, redirect, parser));
    }

    let named = |want: &str| nodes.iter().find(|node| node.0 == want);
    for visible in [
        "msg", "tell", "w", "tps", "mspt", "list", "me", "targets", "message", "uuids", "action",
    ] {
        assert!(named(visible).is_some(), "guest should see {visible}");
    }
    for hidden in [
        "gamemode",
        "defaultgamemode",
        "difficulty",
        "time",
        "weather",
        "seed",
        "say",
        "tp",
        "teleport",
        "kill",
        "op",
        "ban",
        "kick",
        "stop",
        "save-all",
        "whitelist",
    ] {
        assert!(named(hidden).is_none(), "guest should not see {hidden}");
    }
    let msg_index = nodes.iter().position(|node| node.0 == "msg").unwrap();
    for redirect in ["tell", "w"] {
        let node = named(redirect).unwrap();
        assert_eq!(
            node.3,
            Some(msg_index as i32),
            "{redirect} redirects to msg"
        );
    }
    assert!(
        nodes
            .iter()
            .filter(|node| node.2.is_empty() && node.1 == 1)
            .count()
            >= 4,
        "root literal commands are leaves"
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn water_bucket_spread_reaches_tracking_bot_on_fixture_world() {
    use ferrite_protocol::{
        packets::play::{BlockUpdate, CreativeSlot, ItemStack, MoveRotation, UseItemOn},
        types::BlockPos,
    };
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vanilla/26.3");
    let temp = std::env::temp_dir().join(format!("ferrite-fluid-{}", uuid::Uuid::new_v4()));
    let dir = temp.join("dimensions/minecraft/overworld/region");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(
        root.join("fixture-world/world/dimensions/minecraft/overworld/region/r.0.0.mca"),
        dir.join("r.0.0.mca"),
    )
    .unwrap();
    let registry = ferrite_world::RuntimeRegistry::load(root.join("generated")).unwrap();
    let source = super::anvil::AnvilChunkSource::new(temp.clone(), registry);
    loop {
        if let Some(result) = source.get(0, 0, 0) {
            result.unwrap();
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let air = u32::from(ferrite_data::blocks::default_state("air").unwrap());
    let stone = u32::from(ferrite_data::blocks::default_state("stone").unwrap());
    for x in 0..=2 {
        for z in 0..=2 {
            source.set_block(x, 64, z, stone).unwrap();
            source.set_block(x, 65, z, air).unwrap();
            source.set_block(x, 66, z, air).unwrap();
        }
    }
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
    let mut bot = Bot::join(addr, "FluidTester").await;
    bot.until(46).await;
    bot.send(&MoveRotation {
        yaw: 0.0,
        pitch: 90.0,
        flags: 1,
    })
    .await;
    let item = ferrite_data::registries::id("minecraft:item", "minecraft:water_bucket").unwrap();
    bot.send(&CreativeSlot {
        slot: 36,
        item: ItemStack {
            count: 1,
            item_id: item,
        },
    })
    .await;
    bot.send(&UseItemOn {
        hand: VarInt(0),
        location: BlockPos::new(0, 64, 0).pack(),
        face: VarInt(1),
        cursor_x: 0.5,
        cursor_y: 1.0,
        cursor_z: 0.5,
        inside: false,
        world_border_hit: false,
        sequence: VarInt(1),
    })
    .await;
    let got = tokio::time::timeout(std::time::Duration::from_secs(15), async {
        loop {
            let body = bot.until(8).await;
            let update = BlockUpdate::decode(&mut body.as_slice()).unwrap();
            let p = BlockPos::unpack(update.location);
            if (p.x, p.y, p.z) == (1, 65, 0)
                && ferrite_fluid::fluid_state(update.block_state.0 as u32).kind
                    == ferrite_fluid::FluidKind::Water
            {
                break;
            }
        }
    })
    .await;
    drop(bot);
    world.shutdown().await.unwrap();
    server.abort();
    assert!(got.is_ok(), "tracking bot did not receive the spread water");
    let temp_root = std::env::temp_dir().canonicalize().unwrap();
    let resolved = temp.canonicalize().unwrap();
    assert_eq!(resolved.parent(), Some(temp_root.as_path()));
    std::fs::remove_dir_all(resolved).unwrap();
}

/// Runs `f` inside the region owning `chunk` and returns its result, handing off entities that
/// regions report as crossing (the world loop's job) while waiting.
fn in_region<T: Send + 'static>(
    scheduler: &ferrite_region::Scheduler<super::Region, super::GlobalRegion>,
    signals: &mut tokio::sync::mpsc::UnboundedReceiver<super::Signal>,
    chunk: ferrite_region::ChunkPos,
    f: impl FnOnce(&mut super::Region) -> T + Send + 'static,
) -> T {
    let (tx, rx) = std::sync::mpsc::channel();
    scheduler.schedule_at_chunk(chunk, move |r| {
        let _ = tx.send(f(r));
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        while let Ok(signal) = signals.try_recv() {
            if let super::Signal::EntityTransfer { to, entity } = signal {
                super::hand_off(scheduler, to, entity);
            }
        }
        if let Ok(value) = rx.recv_timeout(std::time::Duration::from_millis(10)) {
            return value;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "region task timed out"
        );
    }
}

#[test]
fn entities_follow_region_merges_splits_and_hand_offs() {
    use ferrite_entity::{EntityData, EntityRecord, Flags, allocate_id, random_uuid};
    use ferrite_math::Vec3;
    use ferrite_region::{ChunkPos, Regionizer, Scheduler};
    let (tx, mut signals) = tokio::sync::mpsc::unbounded_channel();
    let ctx = super::TickContext {
        signals: tx,
        tracking: Default::default(),
        source: None,
        rules: Default::default(),
        spawn: [0.0, 65.0, 0.0],
    };
    let scheduler = Scheduler::new(
        Regionizer::<super::Region>::default(),
        super::GlobalRegion::default(),
        2,
        move |r| r.tick(&ctx),
        super::GlobalRegion::tick,
    );
    let west = ChunkPos(0, 0);
    let east = ChunkPos(200, 0);
    scheduler.add_chunk(west);
    scheduler.add_chunk(east);
    assert_eq!(scheduler.region_count(), 2);
    let spawn = |x: f64| EntityRecord {
        pos: Vec3::new(x, 64.0, 8.0),
        vel: Vec3::ZERO,
        flags: Flags::default(),
        data: EntityData::new(allocate_id(), random_uuid(), 71, 0.25, 0.25),
    };
    let (walker, stayer, resident) = (spawn(8.0), spawn(9.0), spawn(3208.0));
    let ids = [walker.data.id, stayer.data.id, resident.data.id];
    in_region(&scheduler, &mut signals, west, move |r| {
        r.entities.insert(walker);
        r.entities.insert(stayer);
    });
    in_region(&scheduler, &mut signals, east, move |r| {
        r.entities.insert(resident);
    });
    // The walker crosses into the east region; the west region hands it off.
    in_region(&scheduler, &mut signals, west, move |r| {
        let key = r.entities.key_of(ids[0]).unwrap();
        r.entities.set_pos(key, Vec3::new(3210.0, 64.0, 8.0));
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !in_region(&scheduler, &mut signals, east, move |r| {
        r.entities.key_of(ids[0]).is_some()
    }) {
        assert!(std::time::Instant::now() < deadline, "walker never arrived");
    }
    assert!(in_region(&scheduler, &mut signals, west, move |r| {
        r.entities.key_of(ids[0]).is_none() && r.entities.key_of(ids[1]).is_some()
    }));
    // Bridge the gap: both regions merge and so do their entities.
    let bridge: Vec<_> = (1..7).map(|i| ChunkPos(i * 30, 0)).collect();
    for chunk in &bridge {
        scheduler.add_chunk(*chunk);
    }
    while scheduler.region_count() != 1 {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(in_region(&scheduler, &mut signals, west, move |r| {
        r.entities.check_invariants().unwrap();
        ids.iter().all(|id| r.entities.key_of(*id).is_some())
    }));
    // Remove the bridge: the region splits and entities follow their sections.
    for chunk in &bridge {
        scheduler.remove_chunk(*chunk);
    }
    while scheduler.region_count() != 2 {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(in_region(&scheduler, &mut signals, west, move |r| {
        r.entities.len() == 1 && r.entities.key_of(ids[1]).is_some()
    }));
    assert!(in_region(&scheduler, &mut signals, east, move |r| {
        r.entities.len() == 2 && r.entities.key_of(ids[0]).is_some()
    }));
    let stopped = scheduler.shutdown();
    let total: usize = stopped.regions.iter().map(|r| r.entities.len()).sum();
    assert_eq!(total, 3);
}

/// The next packet `packet` about entity `id` (any entity when `id` is negative).
async fn entity_packet(bot: &mut Bot, packet: i32, id: i32) -> Vec<u8> {
    let fut = async {
        loop {
            let body = bot.until(packet).await;
            if id < 0 || VarInt::decode(&mut body.as_slice()).unwrap().0 == id {
                return body;
            }
        }
    };
    tokio::time::timeout(std::time::Duration::from_secs(20), fut)
        .await
        .expect("entity packet")
}

/// An item entity record with the given stack, as block drops create them.
fn item_record(pos: [f64; 3], item_id: i32) -> ferrite_entity::EntityRecord {
    use ferrite_entity::{EntityData, EntityRecord, Flags, metadata};
    use ferrite_protocol::packets::play::{ItemStack, MetaValue};
    let kind = ferrite_data::entity_props::by_name("item").unwrap().id;
    let mut data = EntityData::new(
        ferrite_entity::allocate_id(),
        ferrite_entity::random_uuid(),
        kind,
        0.25,
        0.25,
    );
    data.metadata.define(
        metadata::item::ITEM,
        MetaValue::ItemStack(ItemStack::default()),
    );
    data.metadata.set(
        metadata::item::ITEM,
        MetaValue::ItemStack(ItemStack { count: 1, item_id }),
    );
    EntityRecord {
        pos: ferrite_math::Vec3::new(pos[0], pos[1], pos[2]),
        vel: ferrite_math::Vec3::ZERO,
        flags: Flags::default(),
        data,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tracker_spawns_and_removes_entities_by_range() {
    use ferrite_protocol::packets::play::{AddEntity, MetaValue, RemoveEntities, SetEntityData};
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
    let mut bot = Bot::join(addr, "Watcher").await;
    bot.until(46).await;
    let record = item_record([8.5, 65.0, 8.5], 1);
    let id = record.data.id;
    // Arrive through the region hand-off path, as an entity walking in would.
    world
        .signals
        .send(super::Signal::EntityTransfer {
            to: ferrite_region::ChunkPos(0, 0),
            entity: Box::new(record),
        })
        .unwrap();
    let add = entity_packet(&mut bot, AddEntity::ID, id).await;
    let add = AddEntity::decode(&mut add.as_slice()).unwrap();
    assert_eq!(
        add.entity_type.0,
        i32::from(ferrite_data::entity_props::by_name("item").unwrap().id)
    );
    assert_eq!([add.x, add.y, add.z], [8.5, 65.0, 8.5]);
    let data = entity_packet(&mut bot, SetEntityData::ID, id).await;
    let data = SetEntityData::decode(&mut data.as_slice()).unwrap();
    assert!(
        data.values
            .iter()
            .any(|v| v.index == 8 && matches!(v.value, MetaValue::ItemStack(s) if s.item_id == 1))
    );
    // Out of range (view distance 2 caps it at 32 blocks): removed.
    bot.send(&MovePosition {
        x: 60.5,
        y: 65.0,
        z: 0.5,
        flags: 1,
    })
    .await;
    loop {
        let body = entity_packet(&mut bot, RemoveEntities::ID, -1).await;
        let removed = RemoveEntities::decode(&mut body.as_slice()).unwrap();
        if removed.ids.contains(&VarInt(id)) {
            break;
        }
    }
    // Back in range: spawned again.
    bot.send(&MovePosition {
        x: 2.5,
        y: 65.0,
        z: 0.5,
        flags: 1,
    })
    .await;
    entity_packet(&mut bot, AddEntity::ID, id).await;
    drop(bot);
    world.shutdown().await.unwrap();
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn survival_break_drops_an_item_that_falls_and_is_picked_up() {
    use ferrite_protocol::{
        packets::play::{
            AddEntity, ContainerSlot, EntityTeleport, ItemStack, MoveRotation, PlayerAction,
            RemoveEntities, TakeItemEntity,
        },
        types::BlockPos,
    };
    let source = super::anvil::AnvilChunkSource::new_memory();
    let world = World::start_configured(
        Some(source.clone()),
        [0, 65, 0],
        super::WorldSettings {
            gamemode: 0,
            max_players: 20,
            ops: vec![],
            plugins: None,
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
    let mut miner = Bot::join(addr, "Miner").await;
    let mut walker = Bot::join(addr, "Walker").await;
    miner.until(46).await;
    walker.until(46).await;
    // The walker waits off to the side; the miner hovers above the block it breaks, out of
    // pickup reach of the drop.
    walker
        .send(&MovePosition {
            x: 3.5,
            y: 65.0,
            z: 0.5,
            flags: 1,
        })
        .await;
    miner
        .send(&MovePosition {
            x: 0.5,
            y: 66.0,
            z: 0.5,
            flags: 0,
        })
        .await;
    miner
        .send(&MoveRotation {
            yaw: 0.0,
            pitch: 90.0,
            flags: 0,
        })
        .await;
    let location = BlockPos::new(0, 64, 0).pack();
    miner
        .send(&PlayerAction {
            status: VarInt(0),
            location,
            face: 1,
            sequence: VarInt(1),
        })
        .await;
    miner.until(4).await;
    // Grass takes 18 ticks by hand.
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    miner
        .send(&PlayerAction {
            status: VarInt(2),
            location,
            face: 1,
            sequence: VarInt(2),
        })
        .await;
    miner.until(4).await;
    assert_eq!(source.block(0, 64, 0), Some(0));
    // The walker sees the dropped dirt appear inside the broken block.
    let item_type = i32::from(ferrite_data::entity_props::by_name("item").unwrap().id);
    let add = loop {
        let body = entity_packet(&mut walker, AddEntity::ID, -1).await;
        let add = AddEntity::decode(&mut body.as_slice()).unwrap();
        if add.entity_type.0 == item_type {
            break add;
        }
    };
    assert!((0.0..1.0).contains(&add.x) && (0.0..1.0).contains(&add.z));
    assert!(add.y > 64.0 && add.y < 65.0, "spawned at {}", add.y);
    // It falls into the hole and lands on the block below.
    loop {
        let body = entity_packet(&mut walker, EntityTeleport::ID, add.id.0).await;
        let sync = EntityTeleport::decode(&mut body.as_slice()).unwrap();
        if sync.on_ground {
            assert!((sync.y - 64.0).abs() < 1e-9, "landed at {}", sync.y);
            break;
        }
    }
    // Walking over it picks it up into the first hotbar slot.
    walker
        .send(&MovePosition {
            x: 0.5,
            y: 64.0,
            z: 0.5,
            flags: 1,
        })
        .await;
    let body = entity_packet(&mut walker, TakeItemEntity::ID, add.id.0).await;
    let take = TakeItemEntity::decode(&mut body.as_slice()).unwrap();
    assert_eq!(take.amount.0, 1);
    let dirt = ferrite_data::registries::id("minecraft:item", "minecraft:dirt").unwrap();
    let slot = loop {
        let body = entity_packet(&mut walker, ContainerSlot::<ItemStack>::ID, -1).await;
        let slot = ContainerSlot::<ItemStack>::decode(&mut body.as_slice()).unwrap();
        if slot.slot == 36 {
            break slot;
        }
    };
    assert_eq!(
        slot.item,
        ItemStack {
            count: 1,
            item_id: dirt
        }
    );
    // The picked-up item is gone for everyone.
    loop {
        let body = entity_packet(&mut miner, RemoveEntities::ID, -1).await;
        let removed = RemoveEntities::decode(&mut body.as_slice()).unwrap();
        if removed.ids.contains(&add.id) {
            break;
        }
    }
    drop(miner);
    drop(walker);
    world.shutdown().await.unwrap();
    server.abort();
}

/// A placed sand block finds no support and becomes a `FallingBlockEntity`; the tracking bot
/// sees the block removal, the entity and the block placed where it lands.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn placed_sand_falls_and_lands_for_tracking_bot() {
    use ferrite_protocol::{
        packets::play::{AddEntity, BlockUpdate, CreativeSlot, ItemStack, MoveRotation, UseItemOn},
        types::BlockPos,
    };
    let source = super::anvil::AnvilChunkSource::new_memory();
    loop {
        if let Some(result) = source.get(0, 0, 0) {
            result.unwrap();
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let block = |name| u32::from(ferrite_data::blocks::default_state(name).unwrap());
    // A stone block to click, a floor three blocks below the free spot, air in between.
    source.set_block(0, 64, 0, block("stone")).unwrap();
    for y in 61..=64 {
        source.set_block(1, y, 0, block("air")).unwrap();
    }
    source.set_block(1, 60, 0, block("stone")).unwrap();
    let world = World::start_with(Some(source.clone()), [3, 65, 0]);
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
    let mut bot = Bot::join(addr, "SandTester").await;
    bot.until(46).await;
    let sand_item = ferrite_data::registries::id("minecraft:item", "minecraft:sand").unwrap();
    bot.send(&CreativeSlot {
        slot: 36,
        item: ItemStack {
            count: 1,
            item_id: sand_item,
        },
    })
    .await;
    // Look west and down at the east face of the stone block at (0, 64, 0).
    bot.send(&MoveRotation {
        yaw: 90.0,
        pitch: 35.3,
        flags: 1,
    })
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    bot.send(&UseItemOn {
        hand: VarInt(0),
        location: BlockPos::new(0, 64, 0).pack(),
        face: VarInt(5),
        cursor_x: 0.5,
        cursor_y: 0.5,
        cursor_z: 0.5,
        inside: false,
        world_border_hit: false,
        sequence: VarInt(1),
    })
    .await;
    let sand = block("sand");
    let falling_type = i32::from(ferrite_entity::falling::falling_type());
    let mut saw_falling = false;
    let landed = tokio::time::timeout(std::time::Duration::from_secs(20), async {
        loop {
            let (id, body) = bot.next_play().await;
            match id {
                AddEntity::ID => {
                    let add = AddEntity::decode(&mut body.as_slice()).unwrap();
                    if add.entity_type.0 == falling_type {
                        saw_falling = true;
                    }
                }
                BlockUpdate::ID => {
                    let update = BlockUpdate::decode(&mut body.as_slice()).unwrap();
                    if BlockPos::unpack(update.location) == BlockPos::new(1, 61, 0)
                        && update.block_state.0 == sand as i32
                    {
                        break;
                    }
                }
                _ => {}
            }
        }
    })
    .await;
    drop(bot);
    world.shutdown().await.unwrap();
    server.abort();
    assert!(landed.is_ok(), "sand never landed for the tracking bot");
    assert_eq!(source.block(1, 61, 0), Some(sand));
    assert!(saw_falling, "the bot never saw the falling block entity");
}

/// TNT: flint and steel primes the block (`PrimedTnt`), the fuse ends in a vanilla `Explosion`,
/// and the tracking bot sees the entity, the destroyed block and the loot drop.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tnt_explodes_and_drops_blocks_for_tracking_bot() {
    use ferrite_protocol::{
        packets::play::{AddEntity, BlockUpdate, CreativeSlot, ItemStack, MoveRotation, UseItemOn},
        types::BlockPos,
    };
    let source = super::anvil::AnvilChunkSource::new_memory();
    loop {
        if let Some(result) = source.get(0, 0, 0) {
            result.unwrap();
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let block = |name| u32::from(ferrite_data::blocks::default_state(name).unwrap());
    for x in 0..=1 {
        for z in 0..=1 {
            source.set_block(x, 63, z, block("dirt")).unwrap();
            source.set_block(x, 64, z, block("air")).unwrap();
            source.set_block(x, 65, z, block("air")).unwrap();
        }
    }
    source.set_block(0, 64, 0, block("tnt")).unwrap();
    let world = World::start_with(Some(source.clone()), [0, 65, 3]);
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
    let mut bot = Bot::join(addr, "TntTester").await;
    bot.until(46).await;
    let flint =
        ferrite_data::registries::id("minecraft:item", "minecraft:flint_and_steel").unwrap();
    bot.send(&CreativeSlot {
        slot: 36,
        item: ItemStack {
            count: 1,
            item_id: flint,
        },
    })
    .await;
    bot.send(&MoveRotation {
        yaw: 180.0,
        pitch: 35.3,
        flags: 1,
    })
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    bot.send(&UseItemOn {
        hand: VarInt(0),
        location: BlockPos::new(0, 64, 0).pack(),
        face: VarInt(3),
        cursor_x: 0.5,
        cursor_y: 0.5,
        cursor_z: 0.5,
        inside: false,
        world_border_hit: false,
        sequence: VarInt(1),
    })
    .await;
    let tnt_type = i32::from(ferrite_data::entity_props::by_name("tnt").unwrap().id);
    let item_type = i32::from(ferrite_data::entity_props::by_name("item").unwrap().id);
    let mut saw_tnt = false;
    let mut saw_item = false;
    let mut saw_blast = false;
    let blast = tokio::time::timeout(std::time::Duration::from_secs(20), async {
        loop {
            if saw_blast && saw_item {
                break;
            }
            let (id, body) = bot.next_play().await;
            match id {
                AddEntity::ID => {
                    let add = AddEntity::decode(&mut body.as_slice()).unwrap();
                    saw_tnt |= add.entity_type.0 == tnt_type;
                    saw_item |= add.entity_type.0 == item_type;
                }
                BlockUpdate::ID => {
                    let update = BlockUpdate::decode(&mut body.as_slice()).unwrap();
                    if BlockPos::unpack(update.location) == BlockPos::new(0, 63, 0)
                        && update.block_state.0 == 0
                    {
                        saw_blast = true;
                    }
                }
                _ => {}
            }
        }
    })
    .await;
    drop(bot);
    world.shutdown().await.unwrap();
    server.abort();
    assert!(
        blast.is_ok(),
        "the TNT never destroyed the floor for the bot"
    );
    assert_eq!(source.block(0, 63, 0), Some(0));
    assert!(saw_tnt, "the bot never saw the primed TNT entity");
    assert!(saw_blast, "the bot never saw the destroyed floor");
    assert!(saw_item, "the bot never saw the dirt drop");
}

/// Vanilla `Level.markAndNotifyBlock` at work: breaking the block under a torch breaks the
/// torch in the same region tick, and the tracking bot sees both block updates.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn breaking_support_drops_torch_for_tracking_bot() {
    use ferrite_protocol::{
        packets::play::{BlockUpdate, PlayerAction},
        types::BlockPos,
    };
    let source = super::anvil::AnvilChunkSource::new_memory();
    loop {
        if let Some(result) = source.get(0, 0, 0) {
            result.unwrap();
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let block = |name| u32::from(ferrite_data::blocks::default_state(name).unwrap());
    // A wall torch facing east hangs on the block at its west, which is what the bot breaks.
    let wall_torch =
        u32::from(ferrite_data::blocks::state_id("wall_torch", &[("facing", "east")]).unwrap());
    source.set_block(0, 64, 0, block("stone")).unwrap();
    source.set_block(1, 64, 0, wall_torch).unwrap();
    source.set_block(0, 65, 0, block("air")).unwrap();
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
    let mut bot = Bot::join(addr, "TorchTester").await;
    bot.until(46).await;
    // Looking straight down at the block the torch is attached to.
    bot.send(&ferrite_protocol::packets::play::MoveRotation {
        yaw: 0.0,
        pitch: 90.0,
        flags: 0,
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
    assert_eq!(source.block(0, 64, 0), Some(0));
    let got = tokio::time::timeout(std::time::Duration::from_secs(15), async {
        loop {
            let body = bot.until(8).await;
            let update = BlockUpdate::decode(&mut body.as_slice()).unwrap();
            if BlockPos::unpack(update.location) == BlockPos::new(1, 64, 0) {
                break update;
            }
        }
    })
    .await;
    drop(bot);
    world.shutdown().await.unwrap();
    server.abort();
    let got = got.expect("tracking bot did not receive the torch removal");
    assert_eq!(got.block_state.0, 0);
    assert_eq!(source.block(1, 64, 0), Some(0));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn grass_spread_reaches_tracking_bot_on_fixture_world() {
    use ferrite_protocol::{packets::play::BlockUpdate, types::BlockPos};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vanilla/26.3");
    let temp = std::env::temp_dir().join(format!("ferrite-grass-{}", uuid::Uuid::new_v4()));
    let dir = temp.join("dimensions/minecraft/overworld/region");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(
        root.join("fixture-world/world/dimensions/minecraft/overworld/region/r.0.0.mca"),
        dir.join("r.0.0.mca"),
    )
    .unwrap();
    let registry = ferrite_world::RuntimeRegistry::load(root.join("generated")).unwrap();
    let source = super::anvil::AnvilChunkSource::new(temp.clone(), registry);
    loop {
        if let Some(result) = source.get(0, 0, 0) {
            result.unwrap();
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let y = (64..318)
        .find(|y| source.brightness(8, y + 1, 8).unwrap_or(0) >= 9)
        .expect("fixture contains lit position");
    let air = u32::from(ferrite_data::blocks::default_state("air").unwrap());
    let grass = u32::from(ferrite_data::blocks::default_state("grass_block").unwrap());
    let dirt = u32::from(ferrite_data::blocks::default_state("dirt").unwrap());
    for x in 7..=9 {
        for z in 7..=9 {
            source
                .set_block(x, y, z, if (x, z) == (8, 8) { dirt } else { grass })
                .unwrap();
            source.set_block(x, y + 1, z, air).unwrap();
        }
    }
    let world = World::start_with(Some(source), [8, y + 1, 8]);
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
    let mut bot = Bot::join(addr, "GrassTester").await;
    bot.until(46).await;
    world.set_random_tick_speed(256);
    let got = tokio::time::timeout(std::time::Duration::from_secs(25), async {
        loop {
            let body = bot.until(8).await;
            let update = BlockUpdate::decode(&mut body.as_slice()).unwrap();
            let p = BlockPos::unpack(update.location);
            if (p.x, p.y, p.z) == (8, y, 8) && update.block_state.0 as u32 == grass {
                break;
            }
        }
    })
    .await;
    drop(bot);
    world.shutdown().await.unwrap();
    server.abort();
    assert!(got.is_ok(), "tracking bot did not receive grass spread");
    let resolved = temp.canonicalize().unwrap();
    assert_eq!(
        resolved.parent(),
        Some(std::env::temp_dir().canonicalize().unwrap().as_path())
    );
    std::fs::remove_dir_all(resolved).unwrap();
}

/// A world in `gamemode` whose named players are operators (for `/gamemode`), served on a
/// local port.
async fn op_world(
    source: Option<std::sync::Arc<super::anvil::AnvilChunkSource>>,
    gamemode: i32,
    names: &[&str],
) -> (
    World,
    std::net::SocketAddr,
    tokio::task::JoinHandle<anyhow::Result<()>>,
) {
    let world = World::start_configured(
        source,
        [0, 65, 0],
        super::WorldSettings {
            gamemode,
            ops: names
                .iter()
                .map(|name| super::Operator {
                    uuid: ferrite_net::offline_uuid(name),
                    level: 4,
                })
                .collect(),
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
    (world, addr, server)
}

/// Joins and consumes the initial inventory contents.
async fn join_with_inventory(addr: std::net::SocketAddr, name: &str) -> Bot {
    let mut bot = Bot::join(addr, name).await;
    bot.until(18).await;
    bot
}

/// Sends a `container_click` with a stale state id, so the server answers with its whole view
/// of the menu (vanilla `broadcastFullState`), and returns that view.
async fn click_in(
    bot: &mut Bot,
    window: i32,
    slot: i16,
    button: i8,
    input: ferrite_inventory::ClickInput,
) -> Vec<ferrite_protocol::packets::play::ItemStack> {
    use ferrite_protocol::packets::play::{ContainerClick, ContainerContent, ItemStack};
    bot.send(&ContainerClick::<ferrite_item::HashedSlot> {
        window: VarInt(window),
        state: VarInt(-1),
        slot,
        button,
        input: VarInt(input.id()),
        changed: vec![],
        carried: None,
    })
    .await;
    let body = tokio::time::timeout(std::time::Duration::from_secs(20), bot.until(18))
        .await
        .expect("container contents");
    let content = ContainerContent::<ItemStack>::decode(&mut body.as_slice()).unwrap();
    assert_eq!(content.window.0, window);
    content.slots
}

/// [`click_in`] on the inventory menu.
async fn click(
    bot: &mut Bot,
    slot: i16,
    button: i8,
    input: ferrite_inventory::ClickInput,
) -> Vec<ferrite_protocol::packets::play::ItemStack> {
    click_in(bot, 0, slot, button, input).await
}

/// The server view of the inventory menu, without changing anything.
async fn inventory_of(bot: &mut Bot) -> Vec<ferrite_protocol::packets::play::ItemStack> {
    click(bot, -1, 0, ferrite_inventory::ClickInput::Pickup).await
}

fn plain(name: &str, count: i32) -> ferrite_protocol::packets::play::ItemStack {
    ferrite_protocol::packets::play::ItemStack {
        count,
        item_id: ferrite_data::registries::id("minecraft:item", &format!("minecraft:{name}"))
            .unwrap(),
    }
}

/// Switches a bot to another game mode with `/gamemode` and waits for the change.
async fn set_mode(bot: &mut Bot, mode: &str, id: u8) {
    use ferrite_protocol::packets::play::GameEvent;
    bot.raw(7, &format!("gamemode {mode}")).await;
    loop {
        let body = bot.until(GameEvent::ID).await;
        let event = GameEvent::decode(&mut body.as_slice()).unwrap();
        if event.event == 3 && event.value == f32::from(id) {
            break;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn survival_inventory_clicks_drops_and_hotbar_swaps() {
    use ferrite_inventory::ClickInput::*;
    use ferrite_protocol::packets::play::{
        AddEntity, CreativeSlot, ItemStack, PlayerAction, SetHeldItem,
    };
    let (world, addr, server) = op_world(None, 1, &["Sorter"]).await;
    let mut bot = join_with_inventory(addr, "Sorter").await;
    bot.send(&CreativeSlot {
        slot: 36,
        item: plain("stone", 40),
    })
    .await;
    bot.send(&CreativeSlot {
        slot: 9,
        item: plain("dirt", 10),
    })
    .await;
    set_mode(&mut bot, "survival", 0).await;
    let empty = ItemStack::default();
    // Move a stack: pick it up, put it down elsewhere.
    let slots = click(&mut bot, 36, 0, Pickup).await;
    assert_eq!(slots[36], empty);
    let slots = click(&mut bot, 20, 0, Pickup).await;
    assert_eq!((slots[20], slots[36]), (plain("stone", 40), empty));
    // Shift-click from the main inventory goes to the hotbar.
    let slots = click(&mut bot, 20, 0, QuickMove).await;
    assert_eq!((slots[20], slots[36]), (empty, plain("stone", 40)));
    // Number key 3 swaps the hovered slot with hotbar slot 2.
    let slots = click(&mut bot, 9, 2, Swap).await;
    assert_eq!((slots[9], slots[38]), (empty, plain("dirt", 10)));
    // Right-click takes half; a left click puts it down elsewhere.
    click(&mut bot, 36, 1, Pickup).await;
    let slots = click(&mut bot, 10, 0, Pickup).await;
    assert_eq!(
        (slots[36], slots[10]),
        (plain("stone", 20), plain("stone", 20))
    );
    // Q drops one from the selected slot, Ctrl+Q the rest; both become item entities.
    bot.send(&SetHeldItem { slot: 0 }).await;
    for status in [4, 3] {
        bot.send(&PlayerAction {
            status: VarInt(status),
            location: 0,
            face: 0,
            sequence: VarInt(0),
        })
        .await;
    }
    let item_type = i32::from(ferrite_data::entity_props::by_name("item").unwrap().id);
    let mut spawned = 0;
    while spawned < 2 {
        let body = entity_packet(&mut bot, AddEntity::ID, -1).await;
        if AddEntity::decode(&mut body.as_slice())
            .unwrap()
            .entity_type
            .0
            == item_type
        {
            spawned += 1;
        }
    }
    let slots = inventory_of(&mut bot).await;
    assert_eq!(slots[36], empty);
    // Q over a slot of the inventory screen drops one item.
    let slots = click(&mut bot, 10, 0, Throw).await;
    assert_eq!(slots[10], plain("stone", 19));
    // F swaps the main hand with the offhand.
    bot.send(&SetHeldItem { slot: 2 }).await;
    bot.send(&PlayerAction {
        status: VarInt(6),
        location: 0,
        face: 0,
        sequence: VarInt(0),
    })
    .await;
    let slots = inventory_of(&mut bot).await;
    assert_eq!((slots[38], slots[45]), (empty, plain("dirt", 10)));
    drop(bot);
    world.shutdown().await.unwrap();
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn block_placement_uses_items_in_survival_only() {
    use ferrite_protocol::{
        packets::play::{CreativeSlot, MoveRotation, PlayerAction, UseItemOn},
        types::BlockPos,
    };
    let source = super::anvil::AnvilChunkSource::new_memory();
    let (world, addr, server) = op_world(Some(source.clone()), 1, &["Mason"]).await;
    let mut bot = join_with_inventory(addr, "Mason").await;
    bot.until(46).await;
    bot.send(&MoveRotation {
        yaw: 0.0,
        pitch: 90.0,
        flags: 1,
    })
    .await;
    bot.send(&CreativeSlot {
        slot: 36,
        item: plain("stone", 2),
    })
    .await;
    let stone = u32::from(ferrite_data::blocks::default_state("minecraft:stone").unwrap());
    let break_block = |sequence: i32| PlayerAction {
        status: VarInt(0),
        location: BlockPos::new(0, 64, 0).pack(),
        face: 1,
        sequence: VarInt(sequence),
    };
    let place = |sequence: i32| UseItemOn {
        hand: VarInt(0),
        location: BlockPos::new(0, 63, 0).pack(),
        face: VarInt(1),
        cursor_x: 0.5,
        cursor_y: 1.0,
        cursor_z: 0.5,
        inside: false,
        world_border_hit: false,
        sequence: VarInt(sequence),
    };
    // Creative placement keeps the stack.
    bot.send(&break_block(1)).await;
    bot.until(4).await;
    assert_eq!(source.block(0, 64, 0), Some(0));
    bot.send(&place(2)).await;
    bot.until(4).await;
    assert_eq!(source.block(0, 64, 0), Some(stone));
    assert_eq!(inventory_of(&mut bot).await[36], plain("stone", 2));
    // Break it again in creative, then place it in survival: one stone is used up.
    bot.send(&break_block(3)).await;
    bot.until(4).await;
    assert_eq!(source.block(0, 64, 0), Some(0));
    set_mode(&mut bot, "survival", 0).await;
    bot.send(&place(4)).await;
    bot.until(4).await;
    assert_eq!(source.block(0, 64, 0), Some(stone));
    assert_eq!(inventory_of(&mut bot).await[36], plain("stone", 1));
    drop(bot);
    world.shutdown().await.unwrap();
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn crafting_planks_and_sticks_in_the_grid_and_on_a_table() {
    use ferrite_inventory::ClickInput::*;
    use ferrite_protocol::{
        packets::play::{
            ClientboundContainerClose, ContainerContent, CreativeSlot, ItemStack, MoveRotation,
            OpenScreen, ServerboundContainerClose, UseItemOn,
        },
        types::BlockPos,
    };
    let source = super::anvil::AnvilChunkSource::new_memory();
    let (world, addr, server) = op_world(Some(source.clone()), 1, &["Joiner"]).await;
    let mut bot = join_with_inventory(addr, "Joiner").await;
    bot.until(46).await;
    bot.send(&CreativeSlot {
        slot: 36,
        item: plain("oak_log", 3),
    })
    .await;
    set_mode(&mut bot, "survival", 0).await;
    let empty = ItemStack::default();

    // 2x2 grid: one log shows four planks, a click takes one craft.
    click(&mut bot, 36, 0, Pickup).await;
    let slots = click(&mut bot, 1, 1, Pickup).await;
    assert_eq!(
        (slots[1], slots[0]),
        (plain("oak_log", 1), plain("oak_planks", 4))
    );
    click(&mut bot, 36, 0, Pickup).await;
    click(&mut bot, 0, 0, Pickup).await;
    let slots = click(&mut bot, 9, 0, Pickup).await;
    assert_eq!((slots[0], slots[1]), (empty, empty));
    assert_eq!(
        (slots[9], slots[36]),
        (plain("oak_planks", 4), plain("oak_log", 2))
    );
    // Two planks stacked vertically make sticks; shift-click crafts into the hotbar end.
    click(&mut bot, 9, 0, Pickup).await;
    click(&mut bot, 1, 1, Pickup).await;
    let slots = click(&mut bot, 3, 1, Pickup).await;
    assert_eq!(slots[0], plain("stick", 4));
    click(&mut bot, 9, 0, Pickup).await;
    let slots = click(&mut bot, 0, 0, QuickMove).await;
    assert_eq!((slots[44], slots[0]), (plain("stick", 4), empty));
    assert_eq!(
        (slots[1], slots[3], slots[9]),
        (empty, empty, plain("oak_planks", 2))
    );

    // Crafting table: using it opens the 3x3 menu instead of placing the held logs.
    let table = u32::from(ferrite_data::blocks::default_state("crafting_table").unwrap());
    source.set_block(0, 64, 0, table).unwrap();
    bot.send(&MoveRotation {
        yaw: 0.0,
        pitch: 90.0,
        flags: 1,
    })
    .await;
    let use_table = UseItemOn {
        hand: VarInt(0),
        location: BlockPos::new(0, 64, 0).pack(),
        face: VarInt(1),
        cursor_x: 0.5,
        cursor_y: 1.0,
        cursor_z: 0.5,
        inside: false,
        world_border_hit: false,
        sequence: VarInt(1),
    };
    bot.send(&use_table).await;
    let body = bot.until(OpenScreen::ID).await;
    let screen = OpenScreen::decode(&mut body.as_slice()).unwrap();
    let crafting = ferrite_data::registries::id("minecraft:menu", "minecraft:crafting").unwrap();
    assert_eq!((screen.window.0, screen.menu.0), (1, crafting));
    assert_eq!(screen.title.0, "container.crafting");
    let body = bot.until(18).await;
    let content = ContainerContent::<ItemStack>::decode(&mut body.as_slice()).unwrap();
    assert_eq!(content.window.0, 1);
    assert_eq!(content.slots.len(), 46);
    assert_eq!(content.slots[37], plain("oak_log", 2));
    assert_eq!(source.block(0, 65, 0), Some(0));
    // Log in the centre, shift-click the result: the planks join the stack in the inventory.
    click_in(&mut bot, 1, 37, 0, Pickup).await;
    let slots = click_in(&mut bot, 1, 5, 1, Pickup).await;
    assert_eq!(slots[0], plain("oak_planks", 4));
    click_in(&mut bot, 1, 37, 0, Pickup).await;
    let slots = click_in(&mut bot, 1, 0, 0, QuickMove).await;
    assert_eq!(
        (slots[0], slots[5], slots[10]),
        (empty, empty, plain("oak_planks", 6))
    );
    // Sticks in the 3x3 grid, taken with a plain click.
    click_in(&mut bot, 1, 10, 0, Pickup).await;
    click_in(&mut bot, 1, 2, 1, Pickup).await;
    let slots = click_in(&mut bot, 1, 5, 1, Pickup).await;
    assert_eq!(slots[0], plain("stick", 4));
    click_in(&mut bot, 1, 10, 0, Pickup).await;
    click_in(&mut bot, 1, 0, 0, Pickup).await;
    let slots = click_in(&mut bot, 1, 11, 0, Pickup).await;
    assert_eq!(
        (slots[11], slots[2], slots[5]),
        (plain("stick", 4), empty, empty)
    );
    // Closing gives back what is left in the grid.
    click_in(&mut bot, 1, 37, 0, Pickup).await;
    click_in(&mut bot, 1, 1, 0, Pickup).await;
    bot.send(&ServerboundContainerClose { window: VarInt(1) })
        .await;
    let slots = inventory_of(&mut bot).await;
    assert_eq!(slots[36], plain("oak_log", 1));
    assert_eq!(
        (slots[9], slots[10]),
        (plain("oak_planks", 4), plain("stick", 4))
    );
    assert_eq!(slots[44], plain("stick", 4));
    // Walking away from an open table closes it (vanilla `stillValid`).
    bot.send(&use_table).await;
    let body = bot.until(OpenScreen::ID).await;
    assert_eq!(
        OpenScreen::decode(&mut body.as_slice()).unwrap().window.0,
        2
    );
    bot.raw(7, "tp 30 65 0").await;
    let body = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        bot.until(ClientboundContainerClose::ID),
    )
    .await
    .expect("container close");
    let closed = ClientboundContainerClose::decode(&mut body.as_slice()).unwrap();
    assert_eq!(closed.window.0, 2);
    drop(bot);
    world.shutdown().await.unwrap();
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn chest_contents_are_shared_saved_and_spilled_when_broken() {
    use ferrite_inventory::ClickInput::*;
    use ferrite_protocol::{
        packets::play::{
            ContainerContent, ContainerSlot, CreativeSlot, ItemStack, MoveRotation, OpenScreen,
            PlayerAction, ServerboundContainerClose, UseItemOn,
        },
        types::BlockPos,
    };
    let source = super::anvil::AnvilChunkSource::new_memory();
    let (world, addr, server) = op_world(Some(source.clone()), 1, &[]).await;
    let mut keeper = join_with_inventory(addr, "Keeper").await;
    keeper.until(46).await;
    let chest = u32::from(ferrite_data::blocks::default_state("chest").unwrap());
    source.set_block(0, 64, 0, chest).unwrap();
    keeper
        .send(&CreativeSlot {
            slot: 36,
            item: plain("stone", 40),
        })
        .await;
    let open = |sequence: i32| UseItemOn {
        hand: VarInt(0),
        location: BlockPos::new(0, 64, 0).pack(),
        face: VarInt(1),
        cursor_x: 0.5,
        cursor_y: 0.875,
        cursor_z: 0.5,
        inside: false,
        world_border_hit: false,
        sequence: VarInt(sequence),
    };
    let look_down = MoveRotation {
        yaw: 0.0,
        pitch: 90.0,
        flags: 1,
    };
    let generic_9x3 =
        ferrite_data::registries::id("minecraft:menu", "minecraft:generic_9x3").unwrap();
    async fn opened(bot: &mut Bot) -> (OpenScreen, Vec<ItemStack>) {
        let body = bot.until(OpenScreen::ID).await;
        let screen = OpenScreen::decode(&mut body.as_slice()).unwrap();
        let body = bot.until(18).await;
        let content = ContainerContent::<ItemStack>::decode(&mut body.as_slice()).unwrap();
        assert_eq!(content.window, screen.window);
        (screen, content.slots)
    }
    keeper.send(&look_down).await;
    keeper.send(&open(1)).await;
    let (screen, slots) = opened(&mut keeper).await;
    assert_eq!(
        (screen.menu.0, screen.title.0.as_str()),
        (generic_9x3, "container.chest")
    );
    assert_eq!(slots.len(), 27 + 36);
    let window = screen.window.0;
    // Into the chest: hotbar slot 0 is menu slot 54.
    click_in(&mut keeper, window, 54, 0, Pickup).await;
    let slots = click_in(&mut keeper, window, 0, 0, Pickup).await;
    assert_eq!(
        (slots[0], slots[54]),
        (plain("stone", 40), ItemStack::default())
    );
    let entity = source.block_entity(0, 64, 0).expect("chest block entity");
    assert!(matches!(
        entity.data.get("Items"),
        Some(ferrite_nbt::Tag::List(ferrite_nbt::List::Compound(items))) if items.len() == 1
    ));
    // A second player opening the same chest sees the stone.
    let mut visitor = join_with_inventory(addr, "Visitor").await;
    visitor.until(46).await;
    visitor.send(&look_down).await;
    visitor.send(&open(1)).await;
    let (visitor_screen, slots) = opened(&mut visitor).await;
    assert_eq!(slots[0], plain("stone", 40));
    // The keeper splits the stack; the visitor is sent the change.
    click_in(&mut keeper, window, 0, 1, Pickup).await;
    click_in(&mut keeper, window, 1, 0, Pickup).await;
    loop {
        let body = visitor.until(ContainerSlot::<ItemStack>::ID).await;
        let slot = ContainerSlot::<ItemStack>::decode(&mut body.as_slice()).unwrap();
        if slot.window == visitor_screen.window && slot.slot == 1 {
            assert_eq!(slot.item, plain("stone", 20));
            break;
        }
    }
    keeper
        .send(&ServerboundContainerClose {
            window: VarInt(window),
        })
        .await;
    // Breaking the chest spills its items and removes the block entity.
    keeper
        .send(&PlayerAction {
            status: VarInt(0),
            location: BlockPos::new(0, 64, 0).pack(),
            face: 1,
            sequence: VarInt(2),
        })
        .await;
    keeper.until(4).await;
    assert_eq!(source.block(0, 64, 0), Some(0));
    assert!(source.block_entity(0, 64, 0).is_none());
    // The chest is gone, so the visitor's screen closes (the inventory clicks below are only
    // answered on window 0 once it has). Spilled items have no pickup delay, so the two players
    // standing on the chest collect all 40 stone again.
    let stone = plain("stone", 1).item_id;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        let mut total = 0;
        for bot in [&mut keeper, &mut visitor] {
            total += inventory_of(bot)
                .await
                .iter()
                .filter(|s| s.item_id == stone)
                .map(|s| s.count)
                .sum::<i32>();
        }
        if total == 40 {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "picked up {total} stone"
        );
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    drop(keeper);
    drop(visitor);
    world.shutdown().await.unwrap();
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gamerule_zero_stops_grass_spread_for_bot() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vanilla/26.3");
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("dimensions/minecraft/overworld/region");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(
        root.join("fixture-world/world/dimensions/minecraft/overworld/region/r.0.0.mca"),
        dir.join("r.0.0.mca"),
    )
    .unwrap();
    let registry = ferrite_world::RuntimeRegistry::load(root.join("generated")).unwrap();
    let source = super::anvil::AnvilChunkSource::new(temp.path().to_path_buf(), registry);
    loop {
        if let Some(result) = source.get(0, 0, 0) {
            result.unwrap();
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let y = (64..318)
        .find(|y| source.brightness(8, y + 1, 8).unwrap_or(0) >= 9)
        .unwrap();
    let block = |name| u32::from(ferrite_data::blocks::default_state(name).unwrap());
    for x in 7..=9 {
        for z in 7..=9 {
            source
                .set_block(
                    x,
                    y,
                    z,
                    block(if (x, z) == (8, 8) {
                        "dirt"
                    } else {
                        "grass_block"
                    }),
                )
                .unwrap();
            source.set_block(x, y + 1, z, block("air")).unwrap();
        }
    }
    let world = World::start_configured(
        Some(source.clone()),
        [8, y + 1, 8],
        super::WorldSettings {
            gamemode: 1,
            ops: vec![super::Operator {
                uuid: uuid::Uuid::parse_str("10920508-d5d8-3eed-93d2-92f193afe7d7").unwrap(),
                level: 4,
            }],
            ..Default::default()
        },
    );
    world.set_random_tick_speed(0);
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
    bot.raw(7, "gamerule random_tick_speed 0").await;
    bot.chat_containing("random_tick_speed = 0").await;
    tokio::time::sleep(std::time::Duration::from_millis(750)).await;
    assert_eq!(source.block(8, y, 8), Some(block("dirt")));
    drop(bot);
    world.shutdown().await.unwrap();
    server.abort();
}

/// `/setblock` writes through the shared block-change path, so another player tracking the
/// chunk receives the ordinary `BlockUpdate`, and the source gets vanilla feedback.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn setblock_command_updates_block_for_tracking_player() {
    use ferrite_protocol::{packets::play::BlockUpdate, types::BlockPos};
    let source = super::anvil::AnvilChunkSource::new_memory();
    let world = World::start_configured(
        Some(source.clone()),
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
    let mut builder = Bot::join(addr, "Alice").await;
    let mut observer = Bot::join(addr, "Observer").await;
    builder.until(46).await;
    observer.until(46).await;
    builder.raw(7, "setblock 10 64 0 minecraft:stone").await;
    let body = tokio::time::timeout(std::time::Duration::from_secs(20), observer.until(8))
        .await
        .unwrap();
    let update = BlockUpdate::decode(&mut body.as_slice()).unwrap();
    assert_eq!(BlockPos::unpack(update.location), BlockPos::new(10, 64, 0));
    let stone = ferrite_data::blocks::default_state("minecraft:stone").unwrap();
    assert_eq!(update.block_state.0, i32::from(stone));
    assert_eq!(source.block(10, 64, 0), Some(u32::from(stone)));
    let feedback = builder.chat_containing("Changed the block").await;
    assert!(feedback.contains("10, 64, 0"), "{feedback}");
    drop(builder);
    drop(observer);
    world.shutdown().await.unwrap();
    server.abort();
}

/// A bot that stops reading must be disconnected as timed out, and its blocked writer must not
/// slow down the region ticks or the other players' connections.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn slow_reader_is_disconnected_without_stalling_ticks() {
    use ferrite_protocol::packets::play::{ClientboundPlayerChat, PlayDisconnect};
    let world = World::start();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(serve(
        listener,
        ServerConfig {
            online_mode: false,
            view_distance: 24,
            send_queue_bytes: 128 * 1024,
            game: Some(world.sender.clone()),
            ..Default::default()
        },
    ));
    let mut slow = Bot::join_with_rate(addr, "Slow", 1.0).await;
    let mut fast = Bot::join_with_rate(addr, "Fast", 1.0).await;
    let before = world.tick_samples.lock().unwrap().len();
    // One acknowledgement makes the next tick queue a full 64-chunk batch; the bot never reads
    // it, so the per-connection queue fills while its writer is stuck on the socket. The ack is
    // fully written before the server can drop the connection, so the disconnect is not lost
    // to a reset.
    slow.send(&ChunkBatchReceived {
        chunks_per_tick: 64.0,
    })
    .await;
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    // Other players are unaffected: their chat still arrives while the slow bot is blocked.
    fast.chat("still moving").await;
    let chat = tokio::time::timeout(std::time::Duration::from_secs(20), async {
        loop {
            let body = fast.until(66).await;
            let message = ClientboundPlayerChat::decode(&mut body.as_slice()).unwrap();
            if message.body.content == "still moving" {
                return;
            }
        }
    })
    .await;
    assert!(chat.is_ok(), "chat stalled while a slow client was blocked");
    // Let the slow bot read again; it must receive the timed-out disconnect.
    let mut reason = None;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    while tokio::time::Instant::now() < deadline {
        let Ok((id, body)) =
            tokio::time::timeout(std::time::Duration::from_secs(10), slow.next_play()).await
        else {
            break;
        };
        if id == PlayDisconnect::ID {
            reason = Some(
                PlayDisconnect::decode(&mut body.as_slice())
                    .unwrap()
                    .reason
                    .0,
            );
            break;
        }
    }
    assert_eq!(
        reason.as_deref(),
        Some("Timed out"),
        "slow reader must be disconnected as timed out"
    );
    let samples = world.tick_samples.lock().unwrap().clone();
    assert!(
        samples.len() > before + 5,
        "ticks stopped while a slow client was writing: {before} -> {}",
        samples.len()
    );
    let worst = samples[before..].iter().copied().fold(0.0f64, f64::max);
    assert!(
        worst < 250.0,
        "a slow client stalled region ticks: worst {worst:.1} ms"
    );
    drop(fast);
    drop(slow);
    world.shutdown().await.unwrap();
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn clear_give_and_xp_commands_act_on_a_bot_session() {
    use ferrite_protocol::packets::play::SetExperience;
    let (world, addr, server) = op_world(None, 1, &["Giver"]).await;
    let mut bot = join_with_inventory(addr, "Giver").await;
    let stone = ferrite_data::registries::id("minecraft:item", "minecraft:stone").unwrap();

    // `/give` places the items in the live inventory and syncs the slots.
    bot.raw(7, "give @s stone 5").await;
    bot.chat_containing("Gave 5 Stone to Giver").await;
    let slots = inventory_of(&mut bot).await;
    assert!(
        slots
            .iter()
            .any(|stack| stack.item_id == stone && stack.count == 5)
    );

    // `/clear` removes matching items again.
    bot.raw(7, "clear @s minecraft:stone").await;
    bot.chat_containing("Removed 5 items from Giver").await;
    let slots = inventory_of(&mut bot).await;
    assert!(
        !slots
            .iter()
            .any(|stack| stack.item_id == stone && stack.count > 0)
    );

    // `/xp` set and add follow the vanilla curve and send `set_experience`.
    bot.raw(7, "xp set @s 7 points").await;
    let body = bot.until(SetExperience::ID).await;
    let xp = SetExperience::decode(&mut body.as_slice()).unwrap();
    assert_eq!((xp.level.0, xp.total.0), (1, 7));
    assert_eq!(xp.progress, 0.0);
    bot.raw(7, "xp add @s 1 levels").await;
    let body = bot.until(SetExperience::ID).await;
    let xp = SetExperience::decode(&mut body.as_slice()).unwrap();
    assert_eq!((xp.level.0, xp.total.0), (2, 16));
    bot.raw(7, "xp query @s levels").await;
    bot.chat_containing("Giver has 2 experience levels").await;

    drop(bot);
    world.shutdown().await.unwrap();
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn survival_fall_damage_and_natural_regeneration_with_bot() {
    use ferrite_protocol::packets::play::{MovePosition, SetHealth};
    // Survival mode: creative ignores fall damage (`Player.isInvulnerableTo`).
    let world = World::start_configured(
        None,
        [0, 65, 0],
        super::WorldSettings {
            gamemode: 0,
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
    let mut bot = Bot::join(addr, "Faller").await;
    // Superflat ground is y=64; step off at y=70, then land six blocks lower.
    bot.send(&MovePosition {
        x: 0.5,
        y: 70.0,
        z: 0.5,
        flags: 0,
    })
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    bot.send(&MovePosition {
        x: 0.5,
        y: 64.0,
        z: 0.5,
        flags: 1,
    })
    .await;
    let body = bot.until(SetHealth::ID).await;
    let health = SetHealth::decode(&mut body.as_slice()).unwrap();
    // `Player.causeFallDamage`: ceil(6 - 3) = 3 damage.
    assert_eq!(health.health, 17.0, "fall damage");
    assert_eq!((health.food.0, health.saturation), (20, 5.0));
    // Full food and saturation 5 heal `min(saturation, 6) / 6` after ten ticks (`FoodData.tick`).
    let body = bot.until(SetHealth::ID).await;
    let healed = SetHealth::decode(&mut body.as_slice()).unwrap();
    assert!(
        (healed.health - (17.0 + 5.0 / 6.0)).abs() < 1e-4,
        "natural regeneration: {}",
        healed.health
    );
    drop(bot);
    world.shutdown().await.unwrap();
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn survival_eating_uses_food_component_after_use_duration() {
    use ferrite_protocol::packets::play::{ServerboundUseItem, SetHealth, SetPlayerInventory};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let path = root.join("vanilla/26.3/fixture-world/world");
    let registry =
        ferrite_world::RuntimeRegistry::load(root.join("vanilla/26.3/generated")).unwrap();
    let spawn = super::anvil::read_spawn(&path).unwrap();
    let source = super::anvil::AnvilChunkSource::new(path, registry);
    let world = World::start_configured(
        Some(source),
        spawn,
        super::WorldSettings {
            gamemode: 0,
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
    let mut bot = Bot::join(addr, "Eater").await;
    // Enchanted golden apples have `can_always_eat`, so the full bar does not block eating.
    world
        .console_command("give Eater enchanted_golden_apple 2".into())
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    bot.send(&ServerboundUseItem {
        hand: VarInt(0),
        sequence: VarInt(1),
        yaw: 0.0,
        pitch: 0.0,
    })
    .await;
    // The stack is consumed after `minecraft:consumable`'s 1.6 s; the item update goes first.
    let body = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        bot.until(SetPlayerInventory::<ferrite_protocol::packets::play::ItemStack>::ID),
    )
    .await
    .expect("food use duration");
    let slot: SetPlayerInventory<ferrite_protocol::packets::play::ItemStack> =
        SetPlayerInventory::decode(&mut body.as_slice()).unwrap();
    assert_eq!(slot.slot.0, 0);
    assert_eq!(slot.item.count, 1, "survival eats one item");
    let body = bot.until(SetHealth::ID).await;
    let health = SetHealth::decode(&mut body.as_slice()).unwrap();
    assert_eq!(health.food.0, 20);
    assert_eq!(health.saturation, 20.0, "saturation capped by the food bar");
    drop(bot);
    world.shutdown().await.unwrap();
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn survival_kill_death_screen_and_keep_inventory_respawn() {
    use ferrite_protocol::packets::play::{
        PlayerCombatKill, PositionSync, Respawn, ServerboundClientCommand, SetExperience, SetHealth,
    };
    // The in-memory world keeps `gamerule` changes off the read-only fixture world.
    let spawn = [0, 65, 0];
    let world = World::start_configured(
        None,
        spawn,
        super::WorldSettings {
            gamemode: 0,
            ops: vec![super::Operator {
                uuid: ferrite_net::offline_uuid("Victim"),
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
    let mut bot = Bot::join(addr, "Victim").await;
    world
        .console_command("gamerule keep_inventory true".into())
        .await
        .unwrap();
    bot.raw(7, "xp set @s 10 levels").await;
    let body = bot.until(SetExperience::ID).await;
    let xp = SetExperience::decode(&mut body.as_slice()).unwrap();
    assert_eq!(xp.level.0, 10);
    world.console_command("kill Victim".into()).await.unwrap();
    // Vanilla `ClientboundPlayerCombatKillPacket` opens the death screen.
    let body = bot.until(PlayerCombatKill::ID).await;
    let kill = PlayerCombatKill::decode(&mut body.as_slice()).unwrap();
    assert!(
        kill.message.0.contains("Victim") && kill.message.0.contains("killed"),
        "death message: {}",
        kill.message.0
    );
    let body = bot.until(SetHealth::ID).await;
    let dead = SetHealth::decode(&mut body.as_slice()).unwrap();
    assert_eq!(dead.health, 0.0);
    // `ServerboundClientCommandPacket.Action.PERFORM_RESPAWN`.
    bot.send(&ServerboundClientCommand { action: VarInt(0) })
        .await;
    let body = bot.until(Respawn::ID).await;
    let respawn = Respawn::decode(&mut body.as_slice()).unwrap();
    assert_eq!(respawn.data_to_keep, 3);
    assert_eq!(respawn.dimension, "minecraft:overworld");
    let body = bot.until(SetHealth::ID).await;
    let health = SetHealth::decode(&mut body.as_slice()).unwrap();
    assert_eq!(
        (health.health, health.food.0, health.saturation),
        (20.0, 20, 5.0)
    );
    // keepInventory keeps the level through death and respawn.
    let body = bot.until(SetExperience::ID).await;
    let kept = SetExperience::decode(&mut body.as_slice()).unwrap();
    assert_eq!(kept.level.0, 10);
    let body = bot.until(PositionSync::ID).await;
    let pos = PositionSync::decode(&mut body.as_slice()).unwrap();
    assert_eq!(
        [pos.x, pos.y, pos.z],
        [
            f64::from(spawn[0]) + 0.5,
            f64::from(spawn[1]),
            f64::from(spawn[2]) + 0.5
        ]
    );
    drop(bot);
    world.shutdown().await.unwrap();
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn survival_void_damage_kills_and_sends_the_death_screen() {
    use ferrite_protocol::packets::play::{PlayerCombatKill, SetHealth};
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
    let mut bot = Bot::join(addr, "Voider").await;
    // Below min build height - 64 (`Player.isBelowWorld`): 4 bypassing damage per tick.
    world
        .console_command("tp Voider 0 -200 0".into())
        .await
        .unwrap();
    let body = bot.until(PlayerCombatKill::ID).await;
    let kill = PlayerCombatKill::decode(&mut body.as_slice()).unwrap();
    assert!(
        kill.message.0.contains("fell out of the world"),
        "{}",
        kill.message.0
    );
    let body = bot.until(SetHealth::ID).await;
    let health = SetHealth::decode(&mut body.as_slice()).unwrap();
    assert_eq!(health.health, 0.0);
    drop(bot);
    world.shutdown().await.unwrap();
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn effect_and_title_commands_reach_the_client() {
    use ferrite_protocol::packets::play::{
        ClearTitles, RemoveMobEffect, SetTitleText, SetTitlesAnimation, UpdateMobEffect,
    };

    let (world, addr, server) = op_world(None, 1, &["Healer"]).await;
    let mut bot = Bot::join(addr, "Healer").await;
    bot.until(46).await;

    let speed = ferrite_data::registries::id("minecraft:mob_effect", "minecraft:speed").unwrap();
    bot.raw(7, "effect give @s speed 5 1").await;
    let body = bot.until(UpdateMobEffect::ID).await;
    let effect = UpdateMobEffect::decode(&mut body.as_slice()).unwrap();
    assert_eq!(effect.effect.0, speed);
    assert_eq!(effect.amplifier.0, 1);
    assert_eq!(effect.duration.0, 100);
    assert_eq!(effect.flags, 6);
    bot.chat_containing("Applied effect Speed to Healer").await;

    bot.raw(7, "effect clear @s speed").await;
    let body = bot.until(RemoveMobEffect::ID).await;
    let removed = RemoveMobEffect::decode(&mut body.as_slice()).unwrap();
    assert_eq!(removed.effect.0, speed);
    bot.chat_containing("Removed effect Speed from Healer")
        .await;

    bot.raw(7, "title @s times 10 70 20").await;
    let body = bot.until(SetTitlesAnimation::ID).await;
    let times = SetTitlesAnimation::decode(&mut body.as_slice()).unwrap();
    assert_eq!((times.fade_in, times.stay, times.fade_out), (10, 70, 20));
    bot.chat_containing("Changed title display times for Healer")
        .await;

    bot.raw(7, "title @s title {\"text\":\"Hello\"}").await;
    let body = bot.until(SetTitleText::ID).await;
    let title = SetTitleText::decode(&mut body.as_slice()).unwrap();
    assert_eq!(title.text.0, "Hello");
    bot.chat_containing("Showing new title for Healer").await;

    bot.raw(7, "title @s clear").await;
    let body = bot.until(ClearTitles::ID).await;
    assert!(!ClearTitles::decode(&mut body.as_slice()).unwrap().reset);
    bot.chat_containing("Cleared titles for Healer").await;

    drop(bot);
    world.shutdown().await.unwrap();
    server.abort();
}
