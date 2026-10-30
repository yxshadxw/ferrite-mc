//! A real TCP server-list handshake against `ferrite_net::serve`: the client sends the status
//! intention, a `status_request` and a `ping_request`, and decodes every reply with the protocol
//! codecs — the same exchange the vanilla client uses to fill the multiplayer menu.

use ferrite_net::{STATUS_SAMPLE_LIMIT, ServerConfig, StatusState, serve};
use ferrite_protocol::{
    Decode, FrameDecoder, FrameEncoder, Packet, VarInt, encode_packet,
    packets::{
        handshake::{Intent, Intention},
        status::{PingRequest, PongResponse, StatusRequest, StatusResponse},
    },
};
use std::{sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

/// Read one complete frame, failing the test instead of hanging if the server stalls.
async fn read_frame(stream: &mut TcpStream, decoder: &mut FrameDecoder) -> Vec<u8> {
    loop {
        if let Some(frame) = decoder.next_frame().unwrap() {
            return frame;
        }
        let mut buffer = [0u8; 4096];
        let read = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut buffer))
            .await
            .expect("timed out waiting for a server packet")
            .unwrap();
        assert_ne!(read, 0, "server closed the connection");
        decoder.feed(&buffer[..read]);
    }
}

#[tokio::test]
async fn status_and_ping_over_tcp_report_vanilla_fields() {
    let status = Arc::new(StatusState::default());
    for index in 0..13u128 {
        status.join(
            index as u64 + 1,
            format!("Player{index}"),
            uuid::Uuid::from_u128(index + 1),
        );
    }
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(serve(
        listener,
        ServerConfig {
            motd: "Ferrite status test".into(),
            max_players: 100,
            online_mode: false,
            enforce_secure_profile: true,
            favicon: Some("data:image/png;base64,AAAA".into()),
            status: status.clone(),
            ..Default::default()
        },
    ));
    let mut stream = TcpStream::connect(address).await.unwrap();
    let encoder = FrameEncoder::default();
    let mut decoder = FrameDecoder::default();

    let mut wire = Vec::new();
    for body in [
        {
            let mut body = Vec::new();
            encode_packet(
                &Intention {
                    protocol_version: ferrite_protocol::PROTOCOL_VERSION,
                    server_address: "localhost".into(),
                    server_port: address.port(),
                    intent: Intent::Status,
                },
                &mut body,
            );
            body
        },
        {
            let mut body = Vec::new();
            encode_packet(&StatusRequest, &mut body);
            body
        },
    ] {
        encoder.encode(&body, &mut wire);
    }
    stream.write_all(&wire).await.unwrap();

    let frame = read_frame(&mut stream, &mut decoder).await;
    let mut body = frame.as_slice();
    assert_eq!(VarInt::decode(&mut body).unwrap().0, StatusResponse::ID);
    let response = StatusResponse::decode(&mut body).unwrap();
    assert!(body.is_empty(), "status response has trailing bytes");
    let json: serde_json::Value = serde_json::from_str(&response.json).unwrap();
    assert_eq!(json["version"]["name"], ferrite_protocol::GAME_VERSION);
    assert_eq!(
        json["version"]["protocol"],
        ferrite_protocol::PROTOCOL_VERSION
    );
    assert_eq!(json["players"]["max"], 100);
    assert_eq!(json["players"]["online"], 13);
    let sample = json["players"]["sample"].as_array().unwrap();
    assert_eq!(sample.len(), STATUS_SAMPLE_LIMIT);
    assert_eq!(sample[0]["name"], "Player0");
    assert_eq!(sample[0]["id"], uuid::Uuid::from_u128(1).to_string());
    assert_eq!(sample[11]["name"], "Player11");
    assert_eq!(json["description"]["text"], "Ferrite status test");
    assert_eq!(json["favicon"], "data:image/png;base64,AAAA");
    assert_eq!(json["enforcesSecureChat"], false);

    // The ping payload must come back untouched.
    let mut body = Vec::new();
    encode_packet(
        &PingRequest {
            payload: 0x1122_3344_5566_7788,
        },
        &mut body,
    );
    let mut wire = Vec::new();
    encoder.encode(&body, &mut wire);
    stream.write_all(&wire).await.unwrap();
    let frame = read_frame(&mut stream, &mut decoder).await;
    let mut body = frame.as_slice();
    assert_eq!(VarInt::decode(&mut body).unwrap().0, PongResponse::ID);
    assert_eq!(
        PongResponse::decode(&mut body).unwrap().payload,
        0x1122_3344_5566_7788
    );
    assert!(body.is_empty(), "pong response has trailing bytes");
    server.abort();
}
