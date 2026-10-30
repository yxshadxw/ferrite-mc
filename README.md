# Ferrite

> **English summary** — Ferrite is a Minecraft Java server written in Rust. It aims for
> Folia-style regionized multithreading, and — via an embedded JVM — the ability to run
> Bukkit/Spigot/Paper/Folia plugins, with multiple Minecraft versions supported later.
> Current status: **phase 1 (foundation) nearly complete, with much of phase 2 working on a
> superflat world** — login and encryption, registry sync, spawn, movement, chunk streaming,
> multiplayer visibility and chat, on top of a Folia-style regionizer, Starlight-style lighting
> (**0 mismatches vs vanilla** on 256 chunks), Anvil world IO, `plugin.yml` parsing and an
> embedded JVM bootstrap; measured idle RSS is **16.7 MB**. **Honest caveats: it has not been
> tested with a real client yet, there is no block breaking/placing, no plugin actually runs,
> and no world generation beyond superflat.**
> Target: Minecraft Java 26.3, protocol 777. Build with Rust stable + Java 25, generate
> vanilla data with `python tools/fetch_vanilla.py`, then `cargo test --workspace`.
> No Mojang code or assets are distributed here; see the legal note below.
> Licensed under GPL-3.0-or-later (see `LICENSE`).

Rust로 작성된 Minecraft Java 서버입니다. Folia 스타일의 리전(region) 기반 멀티스레딩을
목표로 하며, 최종적으로는 내장 JVM을 통해 Bukkit/Spigot/Paper/Folia 플러그인을 그대로
실행하는 것을 목표로 합니다.

- **대상 버전**: Minecraft Java 26.3, 프로토콜 777
- **언어**: Rust (edition 2024)
- **설계 목표**: Folia 스타일 리전 멀티스레딩 + Paper/Folia 플러그인 호환 + 다중 버전 지원

## 현재 상태

**1단계(기반)는 거의 끝났고, 2단계의 핵심 기능도 슈퍼플랫 월드 위에서 구현되어
있습니다.** 로그인/암호화, 레지스트리 동기화, 스폰, 이동, 청크 스트리밍, 멀티플레이
가시성, 채팅까지 동작하며, 그 아래에 Folia 스타일 리전라이저와 Starlight 스타일
라이팅이 받쳐 줍니다. 다만 **아직 실제 26.3 클라이언트로 접속해 검증하지는
않았습니다.**

구현되어 커밋된 것:

- **로그인/암호화** — 오프라인 로그인과 온라인 로그인(RSA/AES-CFB8, Mojang `hasJoined`
  검증)
- **레지스트리 동기화** — known-pack 레지스트리/태그 동기화
- **스폰, 이동, 청크 스트리밍, 멀티플레이 가시성, 채팅** — 슈퍼플랫 월드 기준
- **Folia 스타일 리전라이저** — 리전 로컬 세이프 포인트, 리전별 큐, 힙 데드라인
- **Starlight 스타일 라이팅** — 256개 청크에서 바닐라와 대조해 **불일치 0**
- **Anvil 월드 IO** — `.mca`/`.mcc`, 26.3 청크 NBT, 네트워크 인코딩
- **플러그인 기반** — `plugin.yml`/`paper-plugin.yml` 파싱, 내장 JVM(Java 25)
  부트스트랩
- **측정값** — 릴리스 서버 유휴 RSS **16.7 MB**

아직 아닌 것(솔직하게):

- **실제 클라이언트로 접속 테스트를 하지 않았습니다.** 다음 첫 작업입니다.
- 블록 캐기/설치 없음
- 플러그인이 실제로 로드되어 `onEnable`을 실행하지는 않음
- 슈퍼플랫 외 월드 생성 없음

설정 키와 실행 방법은 [`docs/CONFIG.md`](docs/CONFIG.md)에 정리되어 있습니다. 진행
상황은 아래 로드맵 기준으로만 판단해 주세요.

## 로드맵

| 단계 | 내용 |
|---|---|
| 1 | **기반** — 워크스페이스 구조, 프로토콜 코덱, NBT, 네트워킹/로그인 파이프라인, 리전화 뼈대 (진행 중) |
| 2 | 월드 IO/청크/이동/블록/채팅 — 실제로 접속해 플레이 가능한 최소 서버 |
| 3 | 리전라이저 + 벤치마크 — Paper/Folia 대비 성능 측정 |
| 4 | 플러그인 브리지 v1 — 내장 JVM에서 Bukkit 이벤트/커맨드 실행 |
| 5 | 게임플레이 — 인벤토리, 엔티티, 전투, 크래프팅 등 |
| 6 | 바닐라 동일 월드 생성 — 바닐라와 동일한 지형/구조물/시드 호환 |
| 7 | 다중 버전 지원 — 하나의 서버 바이너리로 여러 프로토콜 버전 대응 |
| 8 | Paper 기능 동등성 — Paper API/설정/최적화 기능 정렬 |
| 9 | 호환성 확장 — 플러그인 생태계 커버리지 확대 |

## 크레이트 구성

| 크레이트 | 담당 |
|---|---|
| `ferrite-nbt` | NBT 모델, 바이너리 IO, SNBT |
| `ferrite-protocol` | `Encode`/`Decode`, `VarInt`, 프레이밍, 패킷 구조체 |
| `ferrite-net` | tokio 연결, 암호화, 로그인/설정/플레이 부트스트랩 |
| `ferrite-region` | Folia 스타일 리전라이저와 리전 스케줄러 |
| `ferrite-world` | 청크, 팔레트 컨테이너, Anvil IO, 라이팅 |
| `ferrite-data` | 바닐라 데이터 추출 및 생성 테이블 |
| `ferrite-server` | 바이너리: 설정, 콘솔, 배선 |

## 빌드

필요한 것:

- **Rust stable** (`rust-toolchain.toml`에 채널이 고정되어 있음)
- **Java 25** — 바닐라 데이터 생성기 실행용 (예: Temurin 25)
- **Python 3** — `tools/fetch_vanilla.py` 실행용

```sh
# Git Bash (Rust가 PATH에 없다면 먼저 실행)
source tools/env.sh
# PowerShell
. .\tools\env.ps1

# 1) 바닐라 데이터 생성 (최초 1회, 네트워크 필요)
python tools/fetch_vanilla.py

# 2) 빌드/검증
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

`tools/fetch_vanilla.py`는 공식 서버 jar를 내려받아 데이터 제너레이터를 실행하고,
그 결과를 `vanilla/<버전>/generated/`에 남깁니다. 이 디렉터리는 git에 포함되지 않으며,
패킷 ID·블록·레지스트리 같은 수치는 추측하지 말고 이 파일들에서 읽어야 합니다.
CI(GitHub Actions)에서도 같은 스크립트로 데이터를 생성한 뒤 검사/테스트를 돌립니다
(`.github/workflows/ci.yml`, Linux/Windows).

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
