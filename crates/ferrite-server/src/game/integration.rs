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
    async fn join(addr: std::net::SocketAddr, name: &str) -> Self {
        let stream = TcpStream::connect(addr).await.unwrap();
        let mut b = Self {
            stream,
            encoder: FrameEncoder::default(),
            decoder: FrameDecoder::default(),
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
    assert_eq!([p.x, p.y, p.z], [10.0, 70.0, 20.0]);
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
    drop(miner);
    drop(observer);
    world.shutdown().await.unwrap();
    server.abort();
}
