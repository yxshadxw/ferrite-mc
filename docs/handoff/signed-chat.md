# Signed chat (secure profiles) — 26.3

Status: implemented on `yxshadxw/fb4` for offline and online mode. Packet IDs and field order come
from `vanilla/26.3/generated/reports/packets.json`; wire semantics were checked against the
decompiled 26.3 server (`javap` dumps in `target/`).

## Packets

| State | Bound | Packet | ID |
|---|---|---|---|
| play | serverbound | `chat` | 9 |
| play | serverbound | `chat_ack` | 6 |
| play | serverbound | `chat_command` | 7 |
| play | serverbound | `chat_command_signed` | 8 |
| play | serverbound | `chat_session_update` | 10 |
| play | clientbound | `player_chat` | 66 |
| play | clientbound | `disguised_chat` | 33 |
| play | clientbound | `player_info_update` (`INITIALIZE_CHAT`) | 71 |
| play | clientbound | `system_chat` | 124 |

Packet structs live in `ferrite-protocol/src/packets/play.rs`; routing in `ferrite-net/src/game.rs`;
sessions/signatures/tracker in `ferrite-server/src/game/chat.rs`; broadcast in
`ferrite-server/src/game/global.rs`.

## Behaviour (vanilla parity)

- `chat_session_update` registers the client's `RemoteChatSession.Data` (session UUID + 2048-bit
  RSA profile key). The key is parsed as X.509 (`RsaPublicKey::from_public_key_der`) and rejected
  with `multiplayer.disconnect.invalid_public_key_signature` if malformed. A re-sent identical
  `ProfileKeyData` is ignored; a key expiring earlier than the current key disconnects with
  `multiplayer.disconnect.expired_public_key`.
- After registration the server broadcasts `player_info_update` with the `INITIALIZE_CHAT` action
  to every client (and includes known sessions in the initial `player_info_update` on join), so
  clients can verify later signatures.
- Every `chat` message is verified with `SHA256withRSA` over
  `Int(1) | sender UUID | session UUID | chain index | salt(long) | timestamp epoch seconds(long) |
  content length(int) | content UTF-8 | last-seen count(int) | each 256-byte signature`.
  This is `PlayerChatMessage.updateSignature` / `SignedMessageLink` / `SignedMessageBody` in vanilla.
  A bad signature, a lower timestamp than the previous message, or a missing signature while a
  session is registered breaks the chain (`chat.disabled.messages_*` red message); the chain stays
  broken for the rest of the session.
- `chat_ack` offsets and the `last_seen` update carried by `chat`/`chat_command_signed` feed
  `LastSeenMessagesValidator`: 20 slots, pending/acknowledged entries, offsets validated against
  `trackedMessages.size() - 20`, the 20-bit bitset, and the JVM `Arrays.hashCode`-style checksum
  (checksum `0` means "ignore checksum"). Validated acknowledgements are returned in slot order and
  are what the client signs into its next message.
- Broadcasts are `player_chat` with a per-connection `global_index`, the sender's chain index,
  the signature (or absent for unsigned), the packed signed body, `filterMask` 0 and the
  `minecraft:chat` chat type bound to the sender's name. Only recipients whose client chat
  visibility is `Shown` receive player chat (vanilla `ServerPlayer.acceptsChatMessages`), and skipped
  recipients do not advance their acknowledgement index.
- Offline mode / `enforce-secure-profile = false`: no profile key is required. If a client sends no
  session (cracked client), `chat` is accepted unsigned and broadcast with the current timestamp,
  salt 0 and no last-seen entries, exactly like `PlayerChatMessage.unsigned` +
  `ClientboundPlayerChatPacket` in vanilla. A client that does register a session is validated
  normally even when enforcement is off.

## Deliberate gaps / next steps

- **No Mojang services-key validation.** Vanilla fetches `ServicesKeySet` (PROFILE_KEY) and verifies
  `ProfileKeyData.key_signature`; if the key set is unavailable it *ignores* the session. Ferrite
  has no services-key discovery yet, so it trusts the key after DER parsing/expiry checks. This is
  documented in `chat.rs`. Consequence: `enforce-secure-profile = true` proves that the client owns
  the key it registered, not that Mojang issued it. Do not advertise signed chat as authenticated
  until the services public keys are fetched and cached (or a pinned key is configured).
- **Signed command arguments are not verified.** `chat_command_signed` applies its `last_seen`
  update and then executes the command text as if it were `chat_command`; the `argument_signatures`
  are decoded but ignored. Vanilla's `ServerGamePacketListenerImpl.performSignedChatCommand` collects
  and verifies signatures for signable arguments via `SignableCommand` (Brigadier parse tree), and
  unsigned commands whose parsed command `hasSignableArguments()` are rejected with
  `INVALID_COMMAND_SIGNATURE` when `enforceSecureProfile` is true. Ferrite's `ferrite-command`
  dispatcher has no signable-argument metadata yet, so implementing this needs that metadata first.
  Until then, commands must be treated as unsigned/unverified regardless of the packet used.
- No chat filtering/reporting: `filter_mask` is always 0 and no `unsigned_content` is emitted.
- Only `minecraft:chat` is used for player chat. `/say` still sends `system_chat`; `disguised_chat`
  is implemented but not wired to `/say` (`say_command` chat type), `/msg` (`msg_command_*`) or team
  chat (`team_msg_command_*`).
- No chat spam throttling (`disconnect.spam`) and only a warning log for messages older than vanilla's
  5-minute server expiry.
- `client_information` is only read during configuration; changing chat visibility during play
  (play-state `client_information`, ID 14) is ignored until that packet is routed.

## Verification

- Unit tests (`crates/ferrite-server/src/game/chat.rs`): signed chain advance/tamper/broken-chain,
  out-of-order timestamps, missing profile key in enforced mode, expired key, malformed key,
  signature payload includes the acknowledged last-seen list, last-seen checksum algorithm,
  pending/acknowledged/offset/bitset validation, and session-update round trip.
- Packet round trips: `crates/ferrite-protocol/src/packets/play.rs` (`chat_tests`), plus the
  serverbound routing test in `ferrite-net/src/game.rs`.
- Integration: `offline_chat_broadcasts_unsigned_player_chat_to_others` and the existing movement /
  duplicate-login / region-boundary tests assert unsigned `player_chat` reaches other players with
  no signature and the expected chat type.
- `python tools/wirecheck/run.py` (server running, offline mode) sends an unsigned `chat` and
  decodes the resulting `player_chat` (and all other clientbound packets) with the real 26.3 codecs.
