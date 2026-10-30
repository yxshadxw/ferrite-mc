pub mod cfb8;
pub mod forwarding;
mod wire;
use anyhow::{Context, Result, bail, ensure};
use ferrite_protocol::{
    Decode, FrameDecoder, FrameEncoder, Packet, VarInt, encode_packet,
    packets::{
        handshake::{Intent, Intention},
        status::{PingRequest, PongResponse, StatusRequest, StatusResponse},
    },
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
use wire::Wire;

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub motd: String,
    pub max_players: u32,
    pub online_mode: bool,
    pub encrypt_offline: bool,
    /// Require signed chat from players whose profile key can be validated. Vanilla only
    /// enforces this on authenticated servers and when Mojang's services keys are available.
    pub enforce_secure_profile: bool,
    pub compression_threshold: usize,
    pub view_distance: u8,
    pub forwarding: forwarding::Forwarding,
    pub game: Option<tokio::sync::mpsc::UnboundedSender<game::GameMessage>>,
}
impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            motd: "A Ferrite server".into(),
            max_players: 20,
            online_mode: true,
            encrypt_offline: false,
            enforce_secure_profile: true,
            compression_threshold: 256,
            view_distance: 10,
            forwarding: forwarding::Forwarding::None,
            game: None,
        }
    }
}

pub async fn serve(listener: TcpListener, config: ServerConfig) -> Result<()> {
    i32::try_from(config.compression_threshold)
        .context("compression threshold exceeds protocol limit")?;
    i32::try_from(config.max_players).context("max players exceeds protocol limit")?;
    // A proxy already authenticated the player; never run Mojang session auth on
    // a forwarded connection.
    let key = if !config.forwarding.is_proxy() && (config.online_mode || config.encrypt_offline) {
        Some(std::sync::Arc::new(
            tokio::task::spawn_blocking(|| rsa::RsaPrivateKey::new(&mut rand::thread_rng(), 1024))
                .await??,
        ))
    } else {
        None
    };
    loop {
        let (stream, peer) = listener.accept().await?;
        stream.set_nodelay(true)?;
        let key = key.clone();
        let config = config.clone();
        tokio::spawn(async move {
            if let Err(error) = handle_connection(Wire::new(stream), config, key, peer.ip()).await {
                tracing::info!(%peer, %error, "connection closed");
            }
        });
    }
}

async fn read_packet<R: tokio::io::AsyncRead + Unpin>(
    stream: &mut R,
    decoder: &mut FrameDecoder,
) -> Result<Vec<u8>> {
    loop {
        if let Some(frame) = decoder.next_frame()? {
            return Ok(frame);
        }
        let mut bytes = [0u8; 4096];
        let n = stream.read(&mut bytes).await?;
        if n == 0 {
            bail!("connection closed");
        }
        decoder.feed(&bytes[..n]);
    }
}

async fn send_packet<P: Packet, W: tokio::io::AsyncWrite + Unpin>(
    stream: &mut W,
    packet: &P,
) -> Result<()> {
    let mut body = Vec::new();
    encode_packet(packet, &mut body);
    let mut wire = Vec::new();
    FrameEncoder::default().encode(&body, &mut wire);
    stream.write_all(&wire).await?;
    Ok(())
}

async fn handle_connection(
    mut stream: Wire,
    config: ServerConfig,
    key: Option<std::sync::Arc<rsa::RsaPrivateKey>>,
    peer: std::net::IpAddr,
) -> Result<()> {
    let mut decoder = FrameDecoder::default();
    let packet = read_packet(&mut stream, &mut decoder).await?;
    let mut body = packet.as_slice();
    if VarInt::decode(&mut body)?.0 != Intention::ID {
        bail!("expected handshake");
    }
    let handshake = Intention::decode(&mut body)?;
    if handshake.intent != Intent::Status {
        // BungeeCord legacy forwarding is appended to the handshake host. Paper's
        // Spigot patch in ServerHandshakePacketListenerImpl parses it before the
        // login listener is created; do the same here.
        let legacy = if matches!(&config.forwarding, forwarding::Forwarding::Bungee) {
            Some(
                forwarding::parse_legacy_host(&handshake.server_address)?.context(
                    "BungeeCord forwarding is enabled but the handshake has no forwarding data",
                )?,
            )
        } else {
            None
        };
        return offline_login(stream, decoder, config, key, legacy, peer).await;
    }
    let packet = read_packet(&mut stream, &mut decoder).await?;
    let mut body = packet.as_slice();
    if VarInt::decode(&mut body)?.0 != StatusRequest::ID {
        bail!("expected status request");
    }
    StatusRequest::decode(&mut body)?;
    let json = serde_json::json!({
        "version": { "name": ferrite_protocol::GAME_VERSION, "protocol": ferrite_protocol::PROTOCOL_VERSION },
        "players": { "max": config.max_players, "online": 0 },
        "description": { "text": config.motd }
    }).to_string();
    send_packet(&mut stream, &StatusResponse { json })
        .await
        .context("send status")?;
    let packet = read_packet(&mut stream, &mut decoder).await?;
    let mut body = packet.as_slice();
    if VarInt::decode(&mut body)?.0 != PingRequest::ID {
        bail!("expected ping");
    }
    let ping = PingRequest::decode(&mut body)?;
    send_packet(
        &mut stream,
        &PongResponse {
            payload: ping.payload,
        },
    )
    .await
}

async fn offline_login(
    mut stream: Wire,
    mut decoder: FrameDecoder,
    config: ServerConfig,
    key: Option<std::sync::Arc<rsa::RsaPrivateKey>>,
    legacy: Option<forwarding::LegacyForwarding>,
    peer: std::net::IpAddr,
) -> Result<()> {
    use ferrite_protocol::packets::login::{LoginStart, LoginSuccess, SetCompression};

    let packet = read_packet(&mut stream, &mut decoder).await?;
    let mut body = packet.as_slice();
    if VarInt::decode(&mut body)?.0 != LoginStart::ID {
        bail!("expected login start");
    }
    let start = LoginStart::decode(&mut body)?;
    if !valid_username(&start.name) {
        bail!("invalid player name");
    }
    let profile = match &config.forwarding {
        forwarding::Forwarding::None => {
            // Java's nameUUIDFromBytes hashes the input directly, without a namespace.
            let (uuid, name, properties) = if let Some(key) = key {
                authenticate(
                    &mut stream,
                    &mut decoder,
                    &key,
                    &start.name,
                    config.online_mode,
                )
                .await?
            } else {
                (offline_uuid(&start.name), start.name.clone(), vec![])
            };
            Profile {
                uuid,
                name,
                properties,
                // No proxy: the peer address is the player's address.
                address: Some(peer),
                chat_visibility: 0,
            }
        }
        forwarding::Forwarding::Velocity { secret } => {
            let forwarded = query_velocity(&mut stream, &mut decoder, secret).await?;
            Profile {
                uuid: forwarded.uuid,
                name: forwarded.name,
                properties: forwarded.properties,
                address: Some(forwarded.address),
                chat_visibility: 0,
            }
        }
        forwarding::Forwarding::Bungee => {
            let legacy = legacy.context(
                "BungeeCord forwarding is enabled but the handshake has no forwarding data",
            )?;
            // BungeeCord's legacy payload omits the name; LoginStart supplies it,
            // exactly as Paper's createOfflineProfile does.
            Profile {
                uuid: legacy.uuid,
                name: start.name.clone(),
                properties: legacy.properties,
                address: Some(legacy.address),
                chat_visibility: 0,
            }
        }
    };
    send_packet(
        &mut stream,
        &SetCompression {
            threshold: VarInt(config.compression_threshold as i32),
        },
    )
    .await?;
    decoder.set_compression(Some(config.compression_threshold));
    let mut encoder = FrameEncoder::default();
    encoder.set_compression(Some(config.compression_threshold), 6);
    let mut body = Vec::new();
    encode_packet(
        &LoginSuccess {
            uuid: profile.uuid,
            name: profile.name.clone(),
            properties: profile.properties.clone(),
            session_id: uuid::Uuid::new_v4(),
        },
        &mut body,
    );
    let mut wire = Vec::new();
    encoder.encode(&body, &mut wire);
    stream.write_all(&wire).await?;
    configure(stream, decoder, encoder, config, profile).await
}

/// Ask Velocity for its modern-forwarding payload and verify the HMAC.
///
/// Paper sends `ClientboundCustomQueryPacket` on `velocity:player_info` with one
/// byte containing `MAX_SUPPORTED_FORWARDING_VERSION`; see
/// `ServerLoginPacketListenerImpl.java.patch` and `VelocityProxy.java`.
async fn query_velocity(
    stream: &mut Wire,
    decoder: &mut FrameDecoder,
    secret: &str,
) -> Result<forwarding::VelocityProfile> {
    use ferrite_protocol::packets::login::CustomQueryAnswer;

    let message_id = rand::random::<i32>();
    let query = forwarding::CustomQuery {
        message_id: VarInt(message_id),
        channel: forwarding::VELOCITY_PLAYER_INFO_CHANNEL.into(),
        data: ferrite_protocol::RestBytes(vec![
            u8::try_from(forwarding::VELOCITY_MAX_FORWARDING_VERSION)
                .context("forwarding version byte")?,
        ]),
    };
    send_packet(stream, &query)
        .await
        .context("send Velocity forwarding query")?;
    let packet = read_packet(stream, decoder).await?;
    let mut body = packet.as_slice();
    ensure!(
        VarInt::decode(&mut body)?.0 == CustomQueryAnswer::ID,
        "expected Velocity forwarding answer"
    );
    let answer = CustomQueryAnswer::decode(&mut body)?;
    ensure!(
        answer.message_id.0 == message_id,
        "unexpected Velocity forwarding message id"
    );
    let data = answer
        .data
        .context("client did not answer the Velocity forwarding query")?;
    forwarding::parse_velocity_payload(secret.as_bytes(), &data.0)
}
async fn send_compressed<P: Packet, W: tokio::io::AsyncWrite + Unpin>(
    stream: &mut W,
    encoder: &FrameEncoder,
    packet: &P,
) -> Result<()> {
    let mut body = Vec::new();
    encode_packet(packet, &mut body);
    let mut wire = Vec::new();
    encoder.encode(&body, &mut wire);
    stream.write_all(&wire).await?;
    Ok(())
}
struct Profile {
    uuid: uuid::Uuid,
    name: String,
    properties: Vec<ferrite_protocol::packets::login::Property>,
    /// Address forwarded by the proxy, when one is configured.
    address: Option<std::net::IpAddr>,
    /// The client's chat visibility from configuration (`0` shown, `1` commands only, `2` hidden).
    chat_visibility: u8,
}
async fn configure(
    mut stream: Wire,
    mut decoder: FrameDecoder,
    encoder: FrameEncoder,
    config: ServerConfig,
    mut profile: Profile,
) -> Result<()> {
    use ferrite_protocol::{
        Encode, RestBytes,
        packets::{
            configuration::{
                AcknowledgeFinishConfiguration, ClientboundCustomPayload, ClientboundKnownPacks,
                FinishConfiguration, KnownPack, RegistryData, ServerboundKnownPacks,
            },
            login::LoginAcknowledged,
        },
    };
    let packet = read_packet(&mut stream, &mut decoder).await?;
    let mut body = packet.as_slice();
    if VarInt::decode(&mut body)?.0 != LoginAcknowledged::ID {
        bail!("expected login acknowledgement");
    }
    let mut brand = Vec::new();
    "Ferrite".encode(&mut brand);
    send_compressed(
        &mut stream,
        &encoder,
        &ClientboundCustomPayload {
            channel: "minecraft:brand".into(),
            data: RestBytes(brand),
        },
    )
    .await?;
    let pack = KnownPack {
        namespace: "minecraft".into(),
        id: "core".into(),
        version: ferrite_protocol::GAME_VERSION.into(),
    };
    send_compressed(
        &mut stream,
        &encoder,
        &ClientboundKnownPacks {
            packs: vec![pack.clone()],
        },
    )
    .await?;
    let mut timer = tokio::time::interval_at(
        tokio::time::Instant::now() + std::time::Duration::from_secs(10),
        std::time::Duration::from_secs(10),
    );
    loop {
        let packet = read_config_packet(
            &mut stream,
            &mut decoder,
            &encoder,
            &mut timer,
            Some(&mut profile),
        )
        .await?;
        let mut body = packet.as_slice();
        let id = VarInt::decode(&mut body)?.0;
        if id == ServerboundKnownPacks::ID {
            if !ServerboundKnownPacks::decode(&mut body)?
                .packs
                .contains(&pack)
            {
                bail!("client does not know minecraft:core 26.3");
            }
            break;
        }
    }
    let vanilla = tokio::task::spawn_blocking(registry::load_vanilla).await??;
    for (name, entries) in &vanilla.registries {
        send_compressed(
            &mut stream,
            &encoder,
            &RegistryData {
                registry: name.clone(),
                entries: entries.clone(),
            },
        )
        .await?;
    }
    send_compressed(&mut stream, &encoder, &vanilla.tags).await?;
    send_compressed(&mut stream, &encoder, &FinishConfiguration).await?;
    loop {
        let packet = read_config_packet(
            &mut stream,
            &mut decoder,
            &encoder,
            &mut timer,
            Some(&mut profile),
        )
        .await?;
        let mut body = packet.as_slice();
        if VarInt::decode(&mut body)?.0 == AcknowledgeFinishConfiguration::ID {
            break;
        }
    }
    if !config.online_mode && config.enforce_secure_profile {
        tracing::warn!(
            "enforce-secure-profile is ignored in offline mode: unsigned chat is accepted"
        );
    }
    play(stream, decoder, encoder, config, profile, vanilla).await
}
async fn read_config_packet(
    stream: &mut Wire,
    decoder: &mut FrameDecoder,
    encoder: &FrameEncoder,
    timer: &mut tokio::time::Interval,
    mut profile: Option<&mut Profile>,
) -> Result<Vec<u8>> {
    use ferrite_protocol::packets::configuration::{
        ClientInformation, ClientboundKeepAlive, ServerboundCustomPayload, ServerboundKeepAlive,
    };
    loop {
        tokio::select! {
            _=timer.tick()=>{send_compressed(stream,encoder,&ClientboundKeepAlive{id:1}).await?;}
            packet=read_packet(stream,decoder)=>{let packet=packet?;let mut body=packet.as_slice();let id=VarInt::decode(&mut body)?.0;
                match id {0=>{let info=ClientInformation::decode(&mut body)?;if let Some(profile)=profile.as_deref_mut(){profile.chat_visibility=match info.chat_mode.0{1=>1,2=>2,_=>0};}},2=>{ServerboundCustomPayload::decode(&mut body)?;},4=>{ServerboundKeepAlive::decode(&mut body)?;},_=>return Ok(packet)}
            }
        }
    }
}
#[cfg(test)]
fn flat_chunk(x: i32, z: i32, biome: i32) -> ferrite_protocol::packets::play::ChunkData {
    use ferrite_protocol::{
        Encode,
        packets::play::{ChunkData, Heightmap},
    };
    let mut sections = Vec::new();
    for section in 0..24 {
        let platform = section == 8;
        (if platform { 256i16 } else { 0i16 }).encode(&mut sections);
        0i16.encode(&mut sections);
        if platform {
            4u8.encode(&mut sections);
            VarInt(2).encode(&mut sections);
            VarInt(0).encode(&mut sections);
            VarInt(1).encode(&mut sections);
            for i in 0..256 {
                (if i < 16 { 0x1111_1111_1111_1111i64 } else { 0 }).encode(&mut sections);
            }
        } else {
            0u8.encode(&mut sections);
            VarInt(0).encode(&mut sections);
        }
        0u8.encode(&mut sections);
        VarInt(biome).encode(&mut sections);
    }
    let mut heights = vec![0i64; 37];
    for i in 0..256 {
        heights[i / 7] |= 129i64 << ((i % 7) * 9);
    }
    ChunkData {
        x,
        z,
        heightmaps: vec![
            Heightmap {
                kind: VarInt(1),
                data: heights.clone(),
            },
            Heightmap {
                kind: VarInt(4),
                data: heights,
            },
        ],
        data: sections,
        sky_mask: vec![],
        block_mask: vec![],
        empty_sky_mask: vec![(1 << 26) - 1],
        empty_block_mask: vec![(1 << 26) - 1],
        sky_arrays: vec![],
        block_arrays: vec![],
    }
}
async fn play(
    mut stream: Wire,
    mut decoder: FrameDecoder,
    encoder: FrameEncoder,
    config: ServerConfig,
    profile: Profile,
    vanilla: registry::VanillaData,
) -> Result<()> {
    use ferrite_protocol::packets::play::{PlayDisconnect, PlayKeepAlive};
    use game::{GameMessage, Outbound};
    tracing::info!(name = %profile.name, address = ?profile.address, "entering play");
    let game = config.game.context("game loop unavailable")?;
    let dimension_type = vanilla.registries["minecraft:dimension_type"]
        .iter()
        .position(|v| v == "minecraft:overworld")
        .context("missing overworld")? as i32;
    let biome = vanilla.registries["minecraft:worldgen/biome"]
        .iter()
        .position(|v| v == "minecraft:plains")
        .context("missing plains")? as i32;
    let chat_type = vanilla.registries["minecraft:chat_type"]
        .iter()
        .position(|v| v == "minecraft:chat")
        .context("missing chat type")? as i32;
    static NEXT_SESSION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let session = NEXT_SESSION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let uuid = profile.uuid;
    let (outbound, mut queue) = tokio::sync::mpsc::unbounded_channel();
    game.send(GameMessage::Joined {
        session,
        uuid,
        name: profile.name,
        properties: profile.properties,
        address: profile.address,
        dimension_type,
        biome,
        view_distance: config.view_distance.clamp(2, 32),
        online_mode: config.online_mode,
        // Vanilla: enforce-secure-profile only applies to authenticated servers.
        secure_chat: config.enforce_secure_profile && config.online_mode,
        chat_type,
        chat_visibility: profile.chat_visibility,
        max_players: config.max_players,
        outbound,
    })
    .map_err(|_| anyhow::anyhow!("game loop stopped"))?;
    let result=async {
        let mut tick=tokio::time::interval(std::time::Duration::from_millis(50));
        let mut keepalive=tokio::time::interval_at(tokio::time::Instant::now()+std::time::Duration::from_secs(10),std::time::Duration::from_secs(10));
        loop {tokio::select! {
            _=tick.tick()=>{let mut batch=Vec::new();let mut close=false;while let Ok(msg)=queue.try_recv(){match msg {Outbound::Packet(body)=>{tracing::debug!(id=?ferrite_protocol::VarInt::decode_partial(&body).ok().flatten().map(|v| v.0),len=body.len(),"play packet out");encoder.encode(&body,&mut batch)},Outbound::Disconnect(reason)=>{let mut body=Vec::new();encode_packet(&PlayDisconnect {reason:ferrite_protocol::types::PlainText(reason)},&mut body);encoder.encode(&body,&mut batch);close=true;break;}}} if !batch.is_empty(){stream.write_all(&batch).await?;}
                if close{return Ok(());}},
            _=keepalive.tick()=>{send_compressed(&mut stream,&encoder,&PlayKeepAlive{id:1}).await?;},
            frame=read_packet(&mut stream,&mut decoder)=>{let frame=frame?;if let Some(packet)=game::decode_play(&frame)?{game.send(GameMessage::Input {session,uuid,packet}).map_err(|_|anyhow::anyhow!("game loop stopped"))?;}}
        }}
    }.await;
    let _ = game.send(GameMessage::Left { session, uuid });
    result
}
fn minecraft_digest(digest: &[u8]) -> String {
    num_bigint::BigInt::from_signed_bytes_be(digest).to_str_radix(16)
}
async fn authenticate(
    stream: &mut Wire,
    decoder: &mut FrameDecoder,
    key: &rsa::RsaPrivateKey,
    name: &str,
    online: bool,
) -> Result<(
    uuid::Uuid,
    String,
    Vec<ferrite_protocol::packets::login::Property>,
)> {
    use ferrite_protocol::packets::login::{EncryptionRequest, EncryptionResponse, Property};
    use rand::RngCore;
    use rsa::{Pkcs1v15Encrypt, pkcs8::EncodePublicKey};
    use sha1::{Digest, Sha1};
    let der = rsa::RsaPublicKey::from(key)
        .to_public_key_der()?
        .as_bytes()
        .to_vec();
    let mut token = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut token);
    send_packet(
        stream,
        &EncryptionRequest {
            server_id: String::new(),
            public_key: der.clone(),
            verify_token: token.to_vec(),
            should_authenticate: online,
        },
    )
    .await?;
    // Do not read ahead across the encryption boundary.
    let response = loop {
        if let Some(frame) = decoder.next_frame()? {
            break frame;
        }
        let mut byte = [0u8; 1];
        let n = stream.read(&mut byte).await?;
        if n == 0 {
            bail!("connection closed during encryption");
        }
        decoder.feed(&byte);
    };
    let mut body = response.as_slice();
    if VarInt::decode(&mut body)?.0 != EncryptionResponse::ID {
        bail!("expected encryption response");
    }
    let response = EncryptionResponse::decode(&mut body)?;
    let secret = key
        .decrypt(Pkcs1v15Encrypt, &response.shared_secret)
        .context("invalid encrypted secret")?;
    let check = key
        .decrypt(Pkcs1v15Encrypt, &response.verify_token)
        .context("invalid encrypted token")?;
    if check != token {
        bail!("invalid verify token");
    }
    let secret: [u8; 16] = secret
        .try_into()
        .map_err(|_| anyhow::anyhow!("invalid secret length"))?;
    stream.enable_encryption(&secret);
    if !online {
        return Ok((offline_uuid(name), name.to_owned(), vec![]));
    }
    let mut hash = Sha1::new();
    hash.update(secret);
    hash.update(&der);
    let hash = minecraft_digest(&hash.finalize());
    let response = reqwest::Client::new()
        .get("https://sessionserver.mojang.com/session/minecraft/hasJoined")
        .query(&[("username", name), ("serverId", hash.as_str())])
        .send()
        .await?
        .error_for_status()?;
    let profile: serde_json::Value = response
        .json()
        .await
        .context("session authentication rejected")?;
    let uuid = uuid::Uuid::parse_str(profile["id"].as_str().context("session UUID")?)?;
    let authenticated = profile["name"].as_str().context("session name")?;
    if !authenticated.eq_ignore_ascii_case(name) {
        bail!("session name mismatch");
    }
    let mut properties = Vec::new();
    if let Some(values) = profile["properties"].as_array() {
        for value in values {
            properties.push(Property {
                name: value["name"].as_str().context("property name")?.into(),
                value: value["value"].as_str().context("property value")?.into(),
                signature: value["signature"].as_str().map(str::to_owned),
            });
        }
    }
    Ok((uuid, authenticated.into(), properties))
}
fn valid_username(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 16
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}
fn offline_uuid(name: &str) -> uuid::Uuid {
    let digest = md5::compute(format!("OfflinePlayer:{name}"));
    let mut bytes = digest.0;
    bytes[6] = (bytes[6] & 0x0f) | 0x30;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    uuid::Uuid::from_bytes(bytes)
}
#[cfg(test)]
mod tests {
    use super::*;
    use ferrite_protocol::{
        Decode, FrameDecoder, FrameEncoder, Packet, VarInt, encode_packet,
        packets::{
            handshake::{Intent, Intention},
            status::{PingRequest, PongResponse, StatusRequest, StatusResponse},
        },
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn status_ping_over_tcp() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(serve(listener, ServerConfig::default()));
        let mut socket = tokio::net::TcpStream::connect(addr).await.unwrap();
        let mut wire = Vec::new();
        for packet in [
            {
                let mut b = Vec::new();
                encode_packet(
                    &Intention {
                        protocol_version: 777,
                        server_address: "localhost".into(),
                        server_port: addr.port(),
                        intent: Intent::Status,
                    },
                    &mut b,
                );
                b
            },
            {
                let mut b = Vec::new();
                encode_packet(&StatusRequest, &mut b);
                b
            },
            {
                let mut b = Vec::new();
                encode_packet(&PingRequest { payload: 42 }, &mut b);
                b
            },
        ] {
            FrameEncoder::default().encode(&packet, &mut wire);
        }
        socket.write_all(&wire).await.unwrap();
        let mut decoder = FrameDecoder::default();
        let mut frames = Vec::new();
        while frames.len() < 2 {
            let mut buf = [0; 1024];
            let n = tokio::time::timeout(std::time::Duration::from_secs(5), socket.read(&mut buf))
                .await
                .unwrap()
                .unwrap();
            assert_ne!(n, 0);
            decoder.feed(&buf[..n]);
            while let Some(frame) = decoder.next_frame().unwrap() {
                frames.push(frame);
            }
        }
        let mut body = frames[0].as_slice();
        assert_eq!(VarInt::decode(&mut body).unwrap().0, StatusResponse::ID);
        let response: serde_json::Value =
            serde_json::from_str(&StatusResponse::decode(&mut body).unwrap().json).unwrap();
        assert_eq!(response["version"]["protocol"], 777);
        assert_eq!(response["version"]["name"], "26.3");
        let mut body = frames[1].as_slice();
        assert_eq!(VarInt::decode(&mut body).unwrap().0, PongResponse::ID);
        assert_eq!(PongResponse::decode(&mut body).unwrap().payload, 42);
        server.abort();
    }
}
#[cfg(test)]
mod login_test {
    use super::*;
    use ferrite_protocol::packets::login::{LoginStart, LoginSuccess, SetCompression};
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpStream;
    #[tokio::test]
    async fn offline_login_compresses_and_sends_profile() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(serve(
            listener,
            ServerConfig {
                online_mode: false,
                ..Default::default()
            },
        ));
        let mut stream = TcpStream::connect(addr).await.unwrap();
        let mut wire = Vec::new();
        for body in [
            {
                let mut b = Vec::new();
                encode_packet(
                    &Intention {
                        protocol_version: 777,
                        server_address: "localhost".into(),
                        server_port: addr.port(),
                        intent: Intent::Login,
                    },
                    &mut b,
                );
                b
            },
            {
                let mut b = Vec::new();
                encode_packet(
                    &LoginStart {
                        name: "Steve".into(),
                        uuid: uuid::Uuid::nil(),
                    },
                    &mut b,
                );
                b
            },
        ] {
            FrameEncoder::default().encode(&body, &mut wire);
        }
        stream.write_all(&wire).await.unwrap();
        let mut decoder = FrameDecoder::default();
        let packet = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            read_packet(&mut stream, &mut decoder),
        )
        .await
        .unwrap()
        .unwrap();
        let mut body = packet.as_slice();
        assert_eq!(VarInt::decode(&mut body).unwrap().0, SetCompression::ID);
        assert_eq!(SetCompression::decode(&mut body).unwrap().threshold.0, 256);
        decoder.set_compression(Some(256));
        let packet = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            read_packet(&mut stream, &mut decoder),
        )
        .await
        .unwrap()
        .unwrap();
        let mut body = packet.as_slice();
        assert_eq!(VarInt::decode(&mut body).unwrap().0, LoginSuccess::ID);
        let profile = LoginSuccess::decode(&mut body).unwrap();
        assert_eq!(profile.name, "Steve");
        assert_eq!(
            profile.uuid.to_string(),
            "5627dd98-e6be-3c21-b8a8-e92344183641"
        );
        server.abort();
    }

    #[tokio::test]
    async fn velocity_forwarding_replaces_the_login_profile() {
        use ferrite_protocol::packets::login::CustomQueryAnswer;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let secret = "hunter2";
        let server = tokio::spawn(serve(
            listener,
            ServerConfig {
                online_mode: false,
                forwarding: forwarding::Forwarding::Velocity {
                    secret: secret.into(),
                },
                ..Default::default()
            },
        ));
        let mut stream = TcpStream::connect(addr).await.unwrap();
        let mut wire = Vec::new();
        for body in [
            {
                let mut b = Vec::new();
                encode_packet(
                    &Intention {
                        protocol_version: 777,
                        server_address: "localhost".into(),
                        server_port: addr.port(),
                        intent: Intent::Login,
                    },
                    &mut b,
                );
                b
            },
            {
                let mut b = Vec::new();
                encode_packet(
                    &LoginStart {
                        name: "Steve".into(),
                        uuid: uuid::Uuid::nil(),
                    },
                    &mut b,
                );
                b
            },
        ] {
            FrameEncoder::default().encode(&body, &mut wire);
        }
        stream.write_all(&wire).await.unwrap();
        let mut decoder = FrameDecoder::default();
        let packet = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            read_packet(&mut stream, &mut decoder),
        )
        .await
        .unwrap()
        .unwrap();
        let mut body = packet.as_slice();
        assert_eq!(
            VarInt::decode(&mut body).unwrap().0,
            forwarding::CustomQuery::ID
        );
        let query = forwarding::CustomQuery::decode(&mut body).unwrap();
        assert_eq!(query.channel, forwarding::VELOCITY_PLAYER_INFO_CHANNEL);
        assert_eq!(query.data.0, vec![4]);
        let uuid = uuid::Uuid::parse_str("069a79f4-44e9-4726-a5be-fca90e38aaf5").unwrap();
        let payload = forwarding::signed_velocity_payload(
            secret.as_bytes(),
            4,
            "203.0.113.7",
            uuid,
            "Notch",
            &[],
        );
        send_packet(
            &mut stream,
            &CustomQueryAnswer {
                message_id: query.message_id,
                data: Some(ferrite_protocol::RestBytes(payload)),
            },
        )
        .await
        .unwrap();
        let packet = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            read_packet(&mut stream, &mut decoder),
        )
        .await
        .unwrap()
        .unwrap();
        let mut body = packet.as_slice();
        assert_eq!(VarInt::decode(&mut body).unwrap().0, SetCompression::ID);
        decoder.set_compression(Some(256));
        let packet = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            read_packet(&mut stream, &mut decoder),
        )
        .await
        .unwrap()
        .unwrap();
        let mut body = packet.as_slice();
        assert_eq!(VarInt::decode(&mut body).unwrap().0, LoginSuccess::ID);
        let profile = LoginSuccess::decode(&mut body).unwrap();
        assert_eq!(profile.uuid, uuid);
        assert_eq!(profile.name, "Notch");
        server.abort();
    }
}

#[cfg(test)]
mod registry_tests {
    #[test]
    fn vanilla_registries_and_tags_are_resolved() {
        let data = super::registry::load_vanilla().unwrap();
        let dimension = &data.registries["minecraft:dimension_type"];
        assert!(dimension.contains(&"minecraft:overworld".to_string()));
        let damage = data
            .tags
            .registries
            .iter()
            .find(|r| r.registry == "minecraft:damage_type")
            .unwrap();
        let tag = damage
            .tags
            .iter()
            .find(|t| t.name == "minecraft:is_fire")
            .unwrap();
        let damage_entries = &data.registries["minecraft:damage_type"];
        assert!(
            tag.entries
                .iter()
                .any(|id| damage_entries[id.0 as usize] == "minecraft:lava")
        );
    }
}
mod registry;
#[cfg(test)]
mod configuration_test {
    use super::*;
    use ferrite_protocol::packets::{
        configuration::{
            ClientboundKnownPacks, FinishConfiguration, KnownPack, RegistryData,
            ServerboundKnownPacks,
        },
        login::{LoginAcknowledged, LoginStart},
    };
    use tokio::net::TcpStream;
    #[tokio::test]
    async fn configuration_sends_known_pack_and_registries() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (game_tx, mut game_rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
            if let Some(game::GameMessage::Joined {
                outbound,
                dimension_type,
                biome,
                ..
            }) = game_rx.recv().await
            {
                use ferrite_protocol::packets::play::*;
                let login = PlayLogin {
                    entity_id: 1,
                    hardcore: false,
                    dimensions: vec!["minecraft:overworld".into()],
                    max_players: VarInt(20),
                    view_distance: VarInt(2),
                    simulation_distance: VarInt(2),
                    reduced_debug: false,
                    respawn_screen: true,
                    limited_crafting: false,
                    dimension_type: VarInt(dimension_type),
                    dimension: "minecraft:overworld".into(),
                    seed: 0,
                    game_mode: VarInt(0),
                    previous_game_mode: VarInt(0),
                    debug: false,
                    flat: true,
                    death_location: None,
                    portal_cooldown: VarInt(0),
                    sea_level: VarInt(63),
                    online_mode: false,
                    secure_chat: false,
                };
                let _ = outbound.send(game::packet(&login));
                let _ = outbound.send(game::packet(&PositionSync {
                    teleport_id: VarInt(1),
                    x: 0.5,
                    y: 65.0,
                    z: 0.5,
                    velocity_x: 0.0,
                    velocity_y: 0.0,
                    velocity_z: 0.0,
                    yaw: 0.0,
                    pitch: 0.0,
                    flags: 0,
                }));
                let _ = outbound.send(game::packet(&GameEvent {
                    event: 13,
                    value: 0.0,
                }));
                let _ = outbound.send(game::packet(&flat_chunk(0, 0, biome)));
            }
        });
        let server = tokio::spawn(serve(
            listener,
            ServerConfig {
                online_mode: false,
                game: Some(game_tx),
                ..Default::default()
            },
        ));
        let mut stream = TcpStream::connect(addr).await.unwrap();
        let mut encoder = FrameEncoder::default();
        let mut decoder = FrameDecoder::default();
        send_packet(
            &mut stream,
            &Intention {
                protocol_version: 777,
                server_address: "localhost".into(),
                server_port: addr.port(),
                intent: Intent::Login,
            },
        )
        .await
        .unwrap();
        send_packet(
            &mut stream,
            &LoginStart {
                name: "Steve".into(),
                uuid: uuid::Uuid::nil(),
            },
        )
        .await
        .unwrap();
        read_packet(&mut stream, &mut decoder).await.unwrap();
        encoder.set_compression(Some(256), 6);
        decoder.set_compression(Some(256));
        read_packet(&mut stream, &mut decoder).await.unwrap();
        send_compressed(&mut stream, &encoder, &LoginAcknowledged)
            .await
            .unwrap();
        let mut saw_pack = false;
        while !saw_pack {
            let bytes = read_packet(&mut stream, &mut decoder).await.unwrap();
            let mut body = bytes.as_slice();
            let id = VarInt::decode(&mut body).unwrap().0;
            if id == ClientboundKnownPacks::ID {
                let p = ClientboundKnownPacks::decode(&mut body).unwrap();
                assert_eq!(p.packs[0].version, "26.3");
                saw_pack = true;
            }
        }
        send_compressed(
            &mut stream,
            &encoder,
            &ServerboundKnownPacks {
                packs: vec![KnownPack {
                    namespace: "minecraft".into(),
                    id: "core".into(),
                    version: "26.3".into(),
                }],
            },
        )
        .await
        .unwrap();
        let mut found = false;
        loop {
            let bytes = tokio::time::timeout(
                std::time::Duration::from_secs(20),
                read_packet(&mut stream, &mut decoder),
            )
            .await
            .unwrap()
            .unwrap();
            let mut body = bytes.as_slice();
            let id = VarInt::decode(&mut body).unwrap().0;
            if id == RegistryData::ID {
                let r = RegistryData::decode(&mut body).unwrap();
                if r.registry == "minecraft:dimension_type" {
                    assert!(r.entries.contains(&"minecraft:overworld".to_string()));
                    found = true;
                }
            }
            if id == FinishConfiguration::ID {
                break;
            }
        }
        assert!(found);
        send_compressed(
            &mut stream,
            &encoder,
            &ferrite_protocol::packets::configuration::AcknowledgeFinishConfiguration,
        )
        .await
        .unwrap();
        let mut login = false;
        let mut position = false;
        let mut event = false;
        let mut chunk = false;
        while !(login && position && event && chunk) {
            let bytes = tokio::time::timeout(
                std::time::Duration::from_secs(20),
                read_packet(&mut stream, &mut decoder),
            )
            .await
            .unwrap()
            .unwrap();
            let mut body = bytes.as_slice();
            let id = VarInt::decode(&mut body).unwrap().0;
            match id {
                50 => {
                    let p = ferrite_protocol::packets::play::PlayLogin::decode(&mut body).unwrap();
                    assert_eq!(p.dimension, "minecraft:overworld");
                    login = true;
                }
                73 => {
                    let p =
                        ferrite_protocol::packets::play::PositionSync::decode(&mut body).unwrap();
                    assert_eq!((p.x, p.y, p.z), (0.5, 65.0, 0.5));
                    send_compressed(
                        &mut stream,
                        &encoder,
                        &ferrite_protocol::packets::play::ConfirmTeleport {
                            id: p.teleport_id,
                            x: p.x,
                            y: p.y,
                            z: p.z,
                            yaw: p.yaw,
                            pitch: p.pitch,
                        },
                    )
                    .await
                    .unwrap();
                    position = true;
                }
                39 => {
                    let p = ferrite_protocol::packets::play::GameEvent::decode(&mut body).unwrap();
                    event = p.event == 13;
                }
                46 => {
                    let p = ferrite_protocol::packets::play::ChunkData::decode(&mut body).unwrap();
                    assert!(!p.data.is_empty());
                    assert_eq!(p.heightmaps.len(), 2);
                    chunk = true;
                }
                _ => {}
            }
        }
        server.abort();
    }
}

#[cfg(test)]
mod encryption_tests {
    use super::*;
    use ferrite_protocol::packets::login::{
        EncryptionRequest, EncryptionResponse, LoginStart, LoginSuccess, SetCompression,
    };
    use rsa::{Pkcs1v15Encrypt, RsaPublicKey, pkcs8::DecodePublicKey};
    use sha1::{Digest, Sha1};
    use tokio::net::TcpStream;
    #[test]
    fn minecraft_digest_vectors() {
        for (input, expected) in [
            ("Notch", "4ed1f46bbe04bc756bcb17c0c7ce3e4632f06a48"),
            ("jeb_", "-7c9d5b0044c130109a5d7b5fb5c317c02b4e28c1"),
            ("simon", "88e16a1019277b15d58faf0541e11910eb756f6"),
        ] {
            assert_eq!(minecraft_digest(&Sha1::digest(input.as_bytes())), expected);
        }
    }
    #[tokio::test]
    async fn encrypted_login_roundtrip() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(serve(
            listener,
            ServerConfig {
                online_mode: false,
                encrypt_offline: true,
                ..Default::default()
            },
        ));
        let mut stream = Wire::new(TcpStream::connect(addr).await.unwrap());
        let mut decoder = FrameDecoder::default();
        send_packet(
            &mut stream,
            &Intention {
                protocol_version: 777,
                server_address: "localhost".into(),
                server_port: addr.port(),
                intent: Intent::Login,
            },
        )
        .await
        .unwrap();
        send_packet(
            &mut stream,
            &LoginStart {
                name: "Steve".into(),
                uuid: uuid::Uuid::nil(),
            },
        )
        .await
        .unwrap();
        let bytes = read_packet(&mut stream, &mut decoder).await.unwrap();
        let mut body = bytes.as_slice();
        assert_eq!(VarInt::decode(&mut body).unwrap().0, EncryptionRequest::ID);
        let request = EncryptionRequest::decode(&mut body).unwrap();
        assert!(!request.should_authenticate);
        let key = RsaPublicKey::from_public_key_der(&request.public_key).unwrap();
        let secret = [42u8; 16];
        let mut rng = rand::thread_rng();
        send_packet(
            &mut stream,
            &EncryptionResponse {
                shared_secret: key.encrypt(&mut rng, Pkcs1v15Encrypt, &secret).unwrap(),
                verify_token: key
                    .encrypt(&mut rng, Pkcs1v15Encrypt, &request.verify_token)
                    .unwrap(),
            },
        )
        .await
        .unwrap();
        stream.enable_encryption(&secret);
        let bytes = read_packet(&mut stream, &mut decoder).await.unwrap();
        let mut body = bytes.as_slice();
        assert_eq!(VarInt::decode(&mut body).unwrap().0, SetCompression::ID);
        decoder.set_compression(Some(256));
        let bytes = read_packet(&mut stream, &mut decoder).await.unwrap();
        let mut body = bytes.as_slice();
        assert_eq!(VarInt::decode(&mut body).unwrap().0, LoginSuccess::ID);
        assert_eq!(LoginSuccess::decode(&mut body).unwrap().name, "Steve");
        let mut encoder = FrameEncoder::default();
        encoder.set_compression(Some(256), 6);
        send_compressed(
            &mut stream,
            &encoder,
            &ferrite_protocol::packets::login::LoginAcknowledged,
        )
        .await
        .unwrap();
        let bytes = read_packet(&mut stream, &mut decoder).await.unwrap();
        let mut body = bytes.as_slice();
        assert_eq!(
            VarInt::decode(&mut body).unwrap().0,
            ferrite_protocol::packets::configuration::ClientboundCustomPayload::ID
        );
        assert_eq!(
            ferrite_protocol::packets::configuration::ClientboundCustomPayload::decode(&mut body)
                .unwrap()
                .channel,
            "minecraft:brand"
        );
        server.abort();
    }
}

#[cfg(test)]
mod chunk_tests {
    use super::*;
    #[test]
    fn chunk_contains_platform_and_correct_heightmap() {
        let chunk = flat_chunk(0, 0, 5);
        let mut input = chunk.data.as_slice();
        for section in 0..24 {
            assert_eq!(
                i16::decode(&mut input).unwrap(),
                if section == 8 { 256 } else { 0 }
            );
            assert_eq!(i16::decode(&mut input).unwrap(), 0);
            let bits = u8::decode(&mut input).unwrap();
            if section == 8 {
                assert_eq!(bits, 4);
                assert_eq!(VarInt::decode(&mut input).unwrap().0, 2);
                assert_eq!(VarInt::decode(&mut input).unwrap().0, 0);
                assert_eq!(VarInt::decode(&mut input).unwrap().0, 1);
                for i in 0..256 {
                    assert_eq!(
                        i64::decode(&mut input).unwrap(),
                        if i < 16 { 0x1111_1111_1111_1111 } else { 0 }
                    );
                }
            } else {
                assert_eq!(bits, 0);
                assert_eq!(VarInt::decode(&mut input).unwrap().0, 0);
            }
            assert_eq!(u8::decode(&mut input).unwrap(), 0);
            assert_eq!(VarInt::decode(&mut input).unwrap().0, 5);
        }
        assert!(input.is_empty());
        for h in chunk.heightmaps {
            for i in 0..256 {
                assert_eq!((h.data[i / 7] >> ((i % 7) * 9)) & 511, 129);
            }
        }
    }
}
#[cfg(test)]
mod keepalive_tests {
    use super::*;
    use ferrite_protocol::packets::configuration::{ClientboundKeepAlive, ServerboundKnownPacks};
    #[tokio::test]
    async fn configuration_wait_sends_keepalive() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let mut client = tokio::net::TcpStream::connect(address).await.unwrap();
        let (server, _) = listener.accept().await.unwrap();
        let task = tokio::spawn(async move {
            let mut wire = Wire::new(server);
            let mut decoder = FrameDecoder::default();
            let encoder = FrameEncoder::default();
            let mut timer = tokio::time::interval(std::time::Duration::from_millis(20));
            read_config_packet(&mut wire, &mut decoder, &encoder, &mut timer, None)
                .await
                .unwrap()
        });
        let mut decoder = FrameDecoder::default();
        let packet = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            read_packet(&mut client, &mut decoder),
        )
        .await
        .unwrap()
        .unwrap();
        let mut body = packet.as_slice();
        assert_eq!(
            VarInt::decode(&mut body).unwrap().0,
            ClientboundKeepAlive::ID
        );
        assert_eq!(ClientboundKeepAlive::decode(&mut body).unwrap().id, 1);
        send_packet(&mut client, &ServerboundKnownPacks { packs: vec![] })
            .await
            .unwrap();
        let packet = task.await.unwrap();
        assert_eq!(
            VarInt::decode(&mut packet.as_slice()).unwrap().0,
            ServerboundKnownPacks::ID
        );
    }
}
#[cfg(test)]
mod validation_tests {
    use super::*;
    #[tokio::test]
    async fn rejects_unrepresentable_compression_threshold() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            serve(
                listener,
                ServerConfig {
                    online_mode: false,
                    compression_threshold: i32::MAX as usize + 1,
                    ..Default::default()
                },
            ),
        )
        .await;
        assert!(result.is_ok_and(|r| r.is_err()));
    }
}
#[cfg(test)]
mod game_channel_tests {
    use super::game::{ServerboundPlay, decode_play};
    use ferrite_protocol::{encode_packet, packets::play::MovePosition};
    #[test]
    fn play_input_decodes_to_typed_event() {
        let mut bytes = Vec::new();
        encode_packet(
            &MovePosition {
                x: 5.0,
                y: 65.0,
                z: 0.5,
                flags: 1,
            },
            &mut bytes,
        );
        assert!(
            matches!(decode_play(&bytes).unwrap(),Some(ServerboundPlay::Position(p)) if p.x==5.0)
        );
    }
}
pub mod game;
