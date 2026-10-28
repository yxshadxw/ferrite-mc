# 내장 최적화 (Built-in optimizations)

Ferrite는 Fabric/Paper 생태계의 대표적인 최적화 모드가 하는 일을 **기본 탑재**합니다.
모드 코드를 가져오지 않습니다(대부분 LGPL 등으로 라이선스가 다르고, Java 코드라 의미도
없음). **기법을 Rust로 새로 구현**하고, 바닐라와 동작이 같은지 테스트로 확인합니다.
최적화가 바닐라 동작을 바꾸는 경우(예: 일부 Lithium 옵션)는 설정으로 끌 수 있게 합니다.

상태: ✅ 구현됨 · 🔧 진행 중 · 📋 예정

## 네트워크 (Krypton, Velocity native, Paper 계열)

| 기법 | 원조 | Ferrite 위치 | 상태 |
|---|---|---|---|
| 틱당 한 번 flush, 패킷 묶어서 write (flush consolidation) | Krypton | ferrite-net 연결 출력 큐 | ✅ (A3) |
| TCP_NODELAY | Paper/Krypton | ferrite-net | ✅ |
| libdeflate 기반 압축/해제 (zlib보다 빠름) | Krypton, Velocity | ferrite-protocol frame | 📋 |
| 압축을 틱 스레드 밖(연결별/풀)에서 수행 | Krypton | ferrite-net | 📋 |
| VarInt 인코딩/길이 계산 fast path (분기 없는 bit trick) | Krypton | ferrite-protocol codec | 📋 |
| AES-CFB8 하드웨어 가속(AES-NI) 경로 | Velocity native | ferrite-net wire | 📋 |
| 청크 패킷 인코딩 결과 캐시(같은 청크를 여러 명에게 보낼 때 재사용) | Paper/Krypton | ferrite-server ChunkSource | 📋 |
| 엔티티 트래커 최적화(거리 버킷, 변경분만 전송) | Krypton/Paper | ferrite-server game | 📋 |
| 패킷 할당 줄이기(버퍼 풀, zero-copy) | Krypton | ferrite-protocol/net | 📋 |
| Velocity modern forwarding / 프록시 친화 | Paper | ferrite-net | 📋 |

## 게임 로직 (Lithium)

5단계(게임플레이) 구현 시 **처음부터 이 방식으로** 작성합니다.

| 기법 | Ferrite 위치 | 상태 |
|---|---|---|
| 엔티티 충돌 검사: 청크 섹션 단위 엔티티 목록 + 블록 충돌 shape 캐시 | ferrite-entity / ferrite-math | 📋 (ferrite-math 🔧) |
| 블록 상태별 속성 미리 계산 테이블(투명도, 충돌, 유체 등) | ferrite-data block_props | 🔧 (T1) |
| 호퍼/컨테이너: 인벤토리 변경 추적, 빈 슬롯 스킵, 아이템 엔티티 흡수 캐시 | ferrite-game | 📋 |
| AI: 목표(goal) 선택 캐시, 센서 주기 조정, 경로 탐색 노드 캐시 | ferrite-entity | 📋 |
| POI(마을 침대·작업대 등) 검색을 섹션 인덱스로 | ferrite-world | 📋 |
| 랜덤 틱: 랜덤 틱 대상 블록이 없는 섹션 스킵 | ferrite-world | 📋 |
| 폭발: 광선 계산 캐시, 블록 저항 테이블 | ferrite-game | 📋 |
| 레드스톤: 와이어 업데이트 순서 최적화(Alternate Current 방식, 옵션) | ferrite-game | 📋 |
| 수학: sin/cos 테이블, 방향 벡터 상수 | ferrite-math | 🔧 |

## 청크·메모리 (Starlight, C2ME, FerriteCore, ModernFix 계열)

| 기법 | 원조 | Ferrite 위치 | 상태 |
|---|---|---|---|
| Starlight 방식 조명 엔진 | Starlight | ferrite-light | ✅ (바닐라와 불일치 0) |
| 청크 로드/생성/저장 병렬화 | C2ME | ferrite-server IO 풀, ferrite-worldgen | 🔧 (T2) / 📋 |
| 블록 상태를 u16 ID로, 속성 테이블 공유(중복 제거) | FerriteCore | ferrite-data | ✅ |
| 팔레트 컨테이너 단일값/간접 압축, 빈 조명 섹션 null 표현 | FerriteCore | ferrite-world, ferrite-light | ✅ |
| 월드젠 밀도 함수 컴파일(벡터화) | C2ME | ferrite-worldgen | 📋 |

## 검증 원칙

- **버킷(Bukkit/Paper) 서버 호환이 최우선입니다.** 어떤 최적화도 Bukkit/Paper API가 보장하는
  동작(이벤트 발생 순서와 취소 가능성, 스케줄러 의미, 인벤토리/엔티티 API 결과)을 바꾸면
  안 됩니다. 이벤트를 건너뛰는 최적화(예: 호퍼 이동 스킵)는 해당 이벤트에 등록된 리스너가
  없을 때만 적용합니다.

- 각 최적화는 바닐라 대조 테스트(같은 입력 → 같은 결과)를 통과해야 머지합니다.
- 성능 수치는 `ferrite-server` 예제의 봇 부하 테스트와 각 크레이트 벤치마크로 기록합니다.
