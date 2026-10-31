# Ferrite

> **English summary** — Ferrite is a Minecraft Java server written in Rust. It aims for
> Folia-style regionized multithreading and, via an embedded JVM, the ability to run
> Bukkit/Spigot/Paper/Folia plugins, with multiple Minecraft versions supported later.
> Target: Minecraft Java 26.3, protocol 777. Current status: **playable survival basics on
> bot-verified worlds** — login/encryption, signed chat, server-list ping (players, favicon),
> block break/place with drops, inventory/crafting/chests, item entities, survival (health,
> hunger, death, XP), redstone, falling blocks and explosions, passive mobs, random ticks, fluids,
> ~70 vanilla commands (including execute and data), plugin events, multi-version client
> translation, a Folia-style regionizer and Starlight-style lighting (0 mismatches vs vanilla).
> World generation (density functions, noise chunks, aquifers, surface rules, carvers) matches
> the vanilla Java reference on the fixture world; terrain features and structures are still in
> progress. Plugins load on an embedded JVM with a growing Paper/Bukkit `Server` surface, a
> Folia-style scheduler and a minimal `org.bukkit.craftbukkit` shim. **Honest caveat: it has been
> verified with protocol-level bots and Java differential harnesses, not yet with a real 26.3
> client.**
> Build with Rust stable + Java 25, generate vanilla data with `python tools/fetch_vanilla.py`,
> then `cargo test --workspace`. No Mojang code or assets are distributed here; see the legal
> note below. Licensed under GPL-3.0-or-later (see `LICENSE`).

Rust로 작성된 Minecraft Java 서버입니다. Folia 스타일의 리전(region) 기반 멀티스레딩을
목표로 하며, 최종적으로는 내장 JVM을 통해 Bukkit/Spigot/Paper/Folia 플러그인을 그대로
실행하는 것을 목표로 합니다.

- **대상 버전**: Minecraft Java 26.3, 프로토콜 777
- **언어**: Rust (edition 2024)
- **설계 목표**: Folia 스타일 리전 멀티스레딩 + Paper/Folia 플러그인 호환 + 다중 버전 지원

## 현재 상태

**봇(프로토콜 수준 테스트 클라이언트)과 바닐라 Java 대조 테스트로 검증된 생존 기본기가
동작합니다.** 다만 **아직 실제 26.3 클라이언트로 접속해 검증하지는 않았습니다.**
이것이 다음 첫 작업입니다.

구현되어 커밋된 것:

- **접속** — 오프라인/온라인 로그인(RSA/AES-CFB8, AES-NI), 서명 채팅, 레지스트리/태그
  동기화, Velocity 모던 포워딩과 BungeeCord 레거시 포워딩
- **서버 목록** — 실제 접속자 수와 샘플(최대 12명), 64x64 `server-icon.png` favicon, MOTD,
  `enforcesSecureChat`, 핸드셰이크/상태/로그인 읽기 타임아웃
- **리전 멀티스레딩** — Folia 스타일 리전라이저(리전 로컬 세이프 포인트, 리전별 큐),
  패킷 압축·암호화를 틱 밖의 연결별 writer task로 분리
- **월드** — Anvil IO, 청크 패킷 캐시, Starlight 스타일 라이팅(바닐라 대비 **불일치 0**),
  랜덤 틱(잔디 번짐·나뭇잎 소멸), 유체(물/용암)
- **게임플레이** — 블록 캐기/설치와 드롭, 아이템 엔티티와 줍기, 인벤토리 클릭, 2x2/작업대
  크래프팅, 상자·통 메뉴, 게임룰 59종과 저장
- **생존** — 체력·피해(낙하/익사/용암/불/공허), 허기와 포화도, 음식 먹기, 사망·리스폰
  (`keepInventory` 반영), 경험치 오브와 사망 시 경험치 드롭
- **블록 변화** — 이웃 업데이트와 `canSurvive` 규칙, 낙하 블록(모래·자갈·콘크리트 가루·모루),
  바닐라 폭발 알고리즘(TNT, 크리퍼 위력 상수)
- **레드스톤** — 와이어 신호 감쇠, 레버·버튼·압력판·횃불(번아웃)·관찰자·반복기·비교기,
  램프·문·트랩도어·펜스 게이트·노트 블록 반응
- **몹 (진행 중)** — 몹 프레임워크(체력·피해·넉백·전리품·디스폰), 수동 몹(돼지·소·양·닭)의
  배회/주시 목표와 자연 스폰, `/summon`
- **명령(약 70개)** — `gamemode`, `tp`, `time`, `weather`, `difficulty`, `gamerule`,
  `give`, `clear`, `xp`, `effect`, `enchant`, `title`, `scoreboard`, `bossbar`, `team`,
  `worldborder`, `setblock`, `fill`, `clone`, `execute`(as/at/if/unless/store/run 등),
  `data`(get/merge/modify/remove), `op`, `ban`, `whitelist`, `stop` 등.
  선택자(`@a` 등)는 권한 레벨 2 이상만 사용 가능합니다.
- **플러그인** — `plugin.yml`/`paper-plugin.yml` 로딩, 내장 JVM(Java 25) 위에서 플러그인
  `onEnable` 실행, 플레이어 이동·상호작용·블록 파괴/설치·명령 이벤트(취소 가능),
  오류 정책 `plugins.on-event-error = allow | deny`. Bukkit `Server`/`Player`/`World`/
  `Inventory`/`Scheduler`/`ConfigurationSection`의 자주 쓰는 표면과 최소
  `org.bukkit.craftbukkit` shim을 제공합니다(미구현 메서드는 명확한 메시지의
  `UnsupportedOperationException`). Bukkit 스케줄러 작업은 Folia 규칙에 맞춰 리전/글로벌/
  async 스케줄러로 전달됩니다
- **관리** — `/tps`, `/mspt`, `/profile`
- **월드젠 (진행 중)** — 밀도 함수, 노이즈 청크, aquifer, 표면 규칙, 동굴·협곡 카버가
  바닐라 Java 참조 구현과 일치하는 것을 확인했습니다. 서버는 전용 생성 스레드에서 바닐라
  지형을 생성합니다(`generator = vanilla | superflat`). 청크 생성 처리량 개선, 지형 지물
  (광물·나무), 구조물은 진행 중입니다.
- **다중 버전** — `ferrite-translate`가 연결 단위로 구버전 클라이언트 패킷을 변환합니다
  (`supported-versions = all | native`, 네이티브 클라이언트 경로는 할당 없음).

아직 아닌 것(솔직하게):

- **실제 클라이언트로 접속 테스트를 하지 않았습니다.**
- 적대 몹 AI·전투, 화재(진행 중), 피스톤은 개발 중이거나 아직 없습니다.
- 플러그인 API는 Paper `Server`의 일부(약 120/206 메서드)와 최소 CraftBukkit shim만
  구현되어 있습니다. EssentialsX 같은 대형 플러그인의 모든 호출 경로는 아직 동작하지 않고,
  NMS 타입에 의존하는 메서드는 `UnsupportedOperationException`을 던집니다.
- 바닐라와 동일한 구조물 생성은 아직입니다. 다중 버전 변환은 26.2 클라이언트 등 일부
  버전만 확인되어 있습니다.

설정 키와 실행 방법은 [`docs/CONFIG.md`](docs/CONFIG.md)에 정리되어 있습니다. 진행
상황은 아래 로드맵 기준으로만 판단해 주세요.

## 로드맵

| 단계 | 내용 | 상태 |
|---|---|---|
| 1 | **기반** — 워크스페이스, 프로토콜 코덱, NBT, 네트워킹/로그인, 리전화 뼈대 | 완료 |
| 2 | 월드 IO/청크/이동/블록/채팅 — 접속해 플레이 가능한 최소 서버 | 완료 (봇 검증) |
| 3 | 리전라이저 + 벤치마크 — Paper/Folia 대비 성능 측정 | 진행 중 |
| 4 | 플러그인 브리지 v1 — 내장 JVM에서 Bukkit 이벤트/커맨드 실행 | 진행 중 |
| 5 | 게임플레이 — 인벤토리, 엔티티, 전투, 크래프팅 등 | 진행 중 |
| 6 | 바닐라 동일 월드 생성 — 지형/구조물/시드 호환 | 진행 중 |
| 7 | 다중 버전 지원 — 하나의 바이너리로 여러 프로토콜 버전 | 진행 중 |
| 8 | Paper 기능 동등성 — Paper API/설정/최적화 정렬 | 예정 |
| 9 | 호환성 확장 — 플러그인 생태계 커버리지 확대 | 예정 |

## 크레이트 구성

| 크레이트 | 담당 |
|---|---|
| `ferrite-protocol` | `Encode`/`Decode`, `VarInt`, 프레이밍, 패킷 구조체 |
| `ferrite-net` | tokio 연결, 암호화, 로그인/설정/플레이, 서버 목록 상태 |
| `ferrite-region` | Folia 스타일 리전라이저와 리전 스케줄러 |
| `ferrite-world` | 청크, 팔레트 컨테이너, Anvil IO |
| `ferrite-light` | Starlight 스타일 라이팅 엔진 |
| `ferrite-worldgen`, `ferrite-noise` | 밀도 함수, 노이즈, 청크 생성, 표면 규칙 |
| `ferrite-entity`, `ferrite-fluid` | 리전 로컬 엔티티 저장소와 몹, 유체 |
| `ferrite-translate` | 다중 버전 프로토콜 변환 계층 |
| `ferrite-inventory`, `ferrite-item`, `ferrite-recipe`, `ferrite-loot` | 인벤토리 메뉴, 아이템, 레시피, 전리품 |
| `ferrite-command`, `ferrite-admin` | 브리가디어식 명령 디스패처, 게임룰, 권한/관리 |
| `ferrite-plugin`, `ferrite-plugin-meta`, `ferrite-jvm` | 플러그인 로딩, `plugin.yml`, 내장 JVM |
| `ferrite-profiler` | 틱 프로파일러(`/tps`, `/mspt`, `/profile`) |
| `ferrite-nbt`, `ferrite-data`, `ferrite-text`, `ferrite-math`, `ferrite-config`, `ferrite-console` | NBT, 바닐라 데이터 테이블, 텍스트, 수학, 설정, 콘솔 |
| `ferrite-server` | 바이너리: 설정, 콘솔, 배선 |

## 빌드

필요한 것:

- **Rust stable** — `rust-toolchain.toml`이 Windows MSVC 채널을 고정합니다. Linux/macOS에서는
  `RUSTUP_TOOLCHAIN=stable`로 덮어쓰세요(CI가 그렇게 합니다).
- **Java 25** — 바닐라 데이터 생성기와 내장 JVM/플러그인 브리지용 (예: Temurin 25)
- **Python 3** — `tools/fetch_vanilla.py` 실행용

```sh
# Git Bash (Rust가 PATH에 없다면 먼저 실행)
source tools/env.sh
# PowerShell
. .\tools\env.ps1

# 1) 바닐라 데이터 생성 (최초 1회, 네트워크 + Java 25 필요)
python tools/fetch_vanilla.py

# 2) 선택: 픽스처 월드 (fixture-world를 읽는 테스트용, Java 25 필요)
python tools/make_fixture_world.py

# 3) 빌드/검증
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check

# 4) 실행 (오프라인 모드 로컬 서버)
cargo run --release -p ferrite-server
```

`tools/fetch_vanilla.py`는 공식 서버 jar를 내려받아 데이터 제너레이터를 실행하고,
그 결과를 `vanilla/<버전>/generated/`에 남깁니다. 이 디렉터리는 git에 포함되지 않으며,
패킷 ID·블록·레지스트리 같은 수치는 추측하지 말고 이 파일들에서 읽어야 합니다.
`fixture-world`가 없으면 픽스처를 읽는 테스트는 건너뜁니다(CI가 그 상태입니다).
GitHub Actions(`.github/workflows/ci.yml`)는 Linux/Windows에서 같은 스크립트로 데이터를
생성한 뒤 fmt/clippy/test를 돌리고, 바닐라 데이터가 커밋되지 않았는지도 확인합니다.
Linux 잡은 `RUSTUP_TOOLCHAIN=stable`로 MSVC 고정을 덮어씁니다.

## 법적 고지

이 저장소는 **Mojang의 코드나 에셋을 배포하지 않습니다.** 바닐라 데이터(패킷 리포트,
블록/레지스트리 목록, 월드젠 데이터 등)는 사용자의 로컬 환경에서 공식 서버 jar를
내려받아 생성되며 `vanilla/`에 저장되고 git에서 제외됩니다. 저장소에는 그 데이터를
읽는 코드만 포함됩니다. Minecraft는 Mojang Studios의 상표이며, 이 프로젝트는 Mojang
또는 Microsoft와 아무런 제휴 관계가 없는 비공식 프로젝트입니다.

## 라이선스

GPL-3.0-or-later. 전체 내용은 [`LICENSE`](LICENSE)에 있습니다.
Paper, Folia, Lithium 등 GPL/LGPL 프로젝트의 코드를 직접 포팅하기 위해 MIT/Apache에서 GPLv3로 바꿨습니다.
포팅한 코드의 출처는 [`NOTICE.md`](NOTICE.md)와 각 소스 파일 주석에 적습니다.
