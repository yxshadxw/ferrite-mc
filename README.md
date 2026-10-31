# Ferrite

> **English summary** — Ferrite is a Minecraft Java server written in Rust: Folia-style regionized
> multithreading, and — via an embedded JVM — the ability to run Bukkit/Paper plugins. It targets
> **Minecraft Java 26.3, protocol 777**.
>
> What works today: the full login/configuration/play handshake (offline and online mode, Velocity
> modern and BungeeCord legacy forwarding), registry and known-pack sync, signed 26.3 chat, chunk
> streaming from superflat or real Anvil worlds, Starlight-style lighting (**0 mismatches vs
> vanilla** on the 256-chunk fixture), block breaking and placing, item entities (physics, merging,
> pickup, loot-table drops), fluids, random ticks, gamerules, inventory clicks with crafting and
> chests, an embedded JVM that loads Paper `plugin.yml` plugins and fires their player, chat and
> block events, and a Brigadier-compatible command tree generated from the 26.3 reports.
>
> Honest caveats: there are **no mobs or AI** yet, no redstone, no world generation beyond superflat
> and Anvil worlds, most commands are still stubs, the plugin API covers a small part of Bukkit, and
> no release has been tagged. `docs/BACKLOG.md` lists the remaining work by family.
>
> No Mojang code or assets are distributed here; see the legal note below. Licensed under
> GPL-3.0-or-later (see `LICENSE`).

Rust로 작성된 Minecraft Java 서버입니다. Folia 스타일의 리전(region) 기반 멀티스레딩과, 내장
JVM을 통한 Bukkit/Paper 플러그인 실행을 목표로 합니다.

- **대상 버전**: Minecraft Java 26.3, 프로토콜 777
- **언어**: Rust (edition 2024)
- **설계 목표**: Folia 스타일 리전 멀티스레딩 + Paper/Folia 플러그인 호환 + 다중 버전 지원

## 현재 상태

실제 26.3 클라이언트로 접속해 이동, 블록 파괴/설치, 채팅, 인벤토리를 확인했습니다.

동작하는 것:

- **접속/암호화** — 오프라인·온라인 로그인(RSA/AES-CFB8, AES-NI, Mojang `hasJoined`), Velocity
  modern / BungeeCord legacy 포워딩, 상태 핑(MOTD·파비콘)
- **레지스트리 동기화** — known-pack 레지스트리와 태그
- **채팅** — 26.3 서명 채팅(시그니처 검증, last-seen/ack), 채팅 명령
- **월드** — 슈퍼플랫과 Anvil `.mca` 로드/저장, 청크 스트리밍(배치 흐름 제어), 청크 패킷 캐시,
  높이맵, 블록 엔티티(상자)
- **라이팅** — Starlight 스타일, 픽스처 256개 청크에서 바닐라와 **불일치 0**
- **게임플레이** — 블록 파괴/설치(도구별 시간, 도달 거리), 아이템 엔티티(낙하·병합·줍기·루트
  테이블 드롭), 유체(경사 확산, 용암/물 반응), 랜덤 틱(잔디/잎), 게임룰(파일 저장), 인벤토리
  클릭·제작·상자
- **엔티티** — 리전 로컬 엔티티 저장소, 바닐라 트래커(spawn/move/teleport/remove, 메타데이터),
  아이템 엔티티
- **명령어** — 26.3 `commands.json`에서 생성한 Brigadier 호환 트리. gamemode, tp/teleport, msg/tell/w,
  kill, op/deop, whitelist, ban/pardon/kick, stop, save-*, gamerule, time, weather, difficulty,
  seed, list, say, me, clear, give, xp, tps/mspt/profile
- **플러그인** — 내장 JVM(Java 25), Paper 전용 `plugin.yml` 로딩, onEnable/onDisable, 플레이어/채팅/
  블록 이벤트, 플러그인 명령어, 이벤트 오류 정책
- **리전라이저** — Folia 스타일 데드라인 스케줄러, 리전별 큐, 리전 병합/분할
- **월드젠 데이터 모델** — 밀도 함수 평가기, 비트 단위로 일치하는 노이즈, multi-noise 바이옴 소스
  (지형 생성 자체는 아직 없음 — `docs/BACKLOG.md`)

아직 아닌 것(솔직하게): 몹/AI 없음, 레드스톤 없음, 슈퍼플랫·Anvil 외 지형 생성 없음, 대부분의
명령어가 스텁, Bukkit API는 일부만, 릴리스 태그 없음. 남은 작업은
[`docs/BACKLOG.md`](docs/BACKLOG.md)에 계열별로 정리되어 있습니다.

설정 키와 실행 방법은 [`docs/CONFIG.md`](docs/CONFIG.md)에 있습니다.

## 로드맵

| 단계 | 내용 | 상태 |
|---|---|---|
| 1 | **기반** — 워크스페이스, 프로토콜 코덱, NBT, 네트워킹/로그인, 리전화 | 완료 |
| 2 | 월드 IO/청크/이동/블록/채팅 — 실제로 플레이 가능한 최소 서버 | 진행 중 |
| 3 | 리전라이저 + 벤치마크 — Paper/Folia 대비 성능 측정 | 진행 중 |
| 4 | 플러그인 브리지 — 내장 JVM에서 Bukkit 이벤트/커맨드 실행 | 진행 중 |
| 5 | 게임플레이 — 인벤토리, 엔티티, 전투, 크래프팅 | 진행 중 |
| 6 | 바닐라 동일 월드 생성 — 시드 호환 지형/구조물 | 데이터 모델만 |
| 7 | 다중 버전 지원 — 하나의 바이너리로 여러 프로토콜 | 미시작 |
| 8 | Paper 기능 동등성 — Paper API/설정/최적화 | 미시작 |
| 9 | 호환성 확장 — 플러그인 생태계 커버리지 | 미시작 |

## 크레이트 구성

| 크레이트 | 담당 |
|---|---|
| `ferrite-nbt` | NBT 모델, 바이너리 IO, SNBT |
| `ferrite-protocol` | `Encode`/`Decode`, `VarInt`, 프레이밍, 패킷 구조체 |
| `ferrite-net` | tokio 연결, 암호화/압축, 로그인/설정/플레이 부트스트랩, 연결별 writer 태스크 |
| `ferrite-region` | Folia 스타일 리전라이저와 리전 스케줄러 |
| `ferrite-world` | 청크, 팔레트 컨테이너, Anvil IO, 라이팅, 높이맵 |
| `ferrite-light` | Starlight 스타일 라이팅 엔진 |
| `ferrite-entity` | 엔티티 저장소, 바닐라 트래커, 아이템 엔티티 |
| `ferrite-inventory`, `ferrite-item` | 인벤토리/메뉴, 아이템 컴포넌트와 와이어 코덱 |
| `ferrite-command` | Brigadier 호환 디스패처, 26.3 명령어 트리, 게임룰 |
| `ferrite-data` | 바닐라 데이터 추출 및 생성 테이블 |
| `ferrite-worldgen`, `ferrite-noise` | 월드젠 데이터 모델, 밀도 함수, 노이즈 |
| `ferrite-fluid`, `ferrite-loot`, `ferrite-recipe` | 유체, 루트 테이블, 제작법 |
| `ferrite-plugin*`, `ferrite-jvm` | 플러그인 메타데이터, 내장 JVM 브리지 |
| `ferrite-server` | 바이너리: 설정, 콘솔, 게임 루프, 배선 |

## 빌드

필요한 것:

- **Rust stable** — `rust-toolchain.toml`이 Windows MSVC 채널을 고정합니다. Linux/macOS에서는
  `RUSTUP_TOOLCHAIN=stable`로 덮어쓰세요(CI가 그렇게 합니다).
- **Java 25** — 바닐라 데이터 생성기와 내장 JVM용 (예: Temurin 25)
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

`tools/fetch_vanilla.py`는 공식 서버 jar를 내려받아 데이터 제너레이터를 실행하고, 그 결과를
`vanilla/<버전>/generated/`에 남깁니다. 이 디렉터리는 git에 포함되지 않으며, 패킷 ID·블록·
레지스트리 같은 수치는 추측하지 말고 이 파일들에서 읽어야 합니다. `fixture-world`가 없으면
픽스처를 읽는 테스트는 건너뜁니다(CI가 그 상태입니다).

GitHub Actions(`.github/workflows/ci.yml`)는 Linux와 Windows에서 같은 스크립트로 데이터를
생성한 뒤 fmt/clippy/test를 돌리고, 바닐라 데이터 디렉터리가 커밋되지 않았는지도 확인합니다.
Linux 잡은 `RUSTUP_TOOLCHAIN=stable`로 MSVC 고정을 덮어씁니다.

## 법적 고지

이 저장소는 **Mojang의 코드나 에셋을 배포하지 않습니다.** 바닐라 데이터(패킷 리포트,
블록/레지스트리 목록, 월드젠 데이터 등)는 사용자의 로컬 환경에서 공식 서버 jar를 내려받아
생성되며 `vanilla/`에 저장되고 git에서 제외됩니다. 저장소에는 그 데이터를 읽는 코드만
포함됩니다. Minecraft는 Mojang Studios의 상표이며, 이 프로젝트는 Mojang 또는 Microsoft와
아무런 제휴 관계가 없는 비공식 프로젝트입니다.

## 라이선스

GPL-3.0-or-later. 전체 내용은 [`LICENSE`](LICENSE)에 있습니다.
Paper, Folia, Lithium 등 GPL/LGPL 프로젝트의 코드를 직접 포팅하기 위해 MIT/Apache에서
GPLv3로 바꿨습니다. 포팅한 코드의 출처는 [`NOTICE.md`](NOTICE.md)와 각 소스 파일 주석에 적습니다.
