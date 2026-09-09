# Kobo Glo에서 PocketJS 구동 쿡북

> 대상 실행자: **Claude Code** (에이전트). 이 문서는 읽어서 실행하는 런북이다.
> **1차 목표: PocketJS가 제공하는 데모/기능(`hero` 등)을 Kobo Glo(1024×758 e-ink)에서 그대로 구동한다.**
> 커스텀 앱(날씨 등)은 호스트가 스톡 콘텐츠로 증명된 **뒤**에 착수한다(§8, 지연).

---

## 0. 시작 전에 — 에이전트를 위한 프라임 디렉티브

1. **커스텀 앱을 짜지 마라.** 1차 목표는 호스트를 세우고 **PocketJS 제공 데모**로 검증하는 것이다. 앱 이식은 §8로 미룬다.
2. **이 문서의 API 스케치를 신뢰하지 마라.** 함수 시그니처·op 코드·필드명은 근사치다. 코드를 쓰기 전에 항상 §부록 A의 "그라운드 트루스 파일"을 먼저 `read`하고 실제 시그니처에 맞춘다. PocketJS·inkview-rs·FBInk는 커밋마다 API가 바뀐다.
3. **`STOP` 마커에서 멈춘다.** 사람의 판단·하드웨어·계정이 필요한 지점이다.
4. **검증 게이트(`GATE`)를 통과 못 하면 다음 단계로 가지 않는다.**
5. **레퍼런스를 베끼되, 그게 PocketBook용임을 잊지 마라.** `hosts/pocketbook/`은 참조일 뿐 Kobo에서 안 돈다(§2).

---

## 1. 현실 인식 — 지금 무엇이 있고 무엇이 막혀 있나

### 존재하는 것
- **`pocket-stack/pocketjs` PR #172** (`wonderbeel:feat/pocketbook-inkview-host`, draft, 커밋 10개). PocketBook e-reader용 inkview 호스트. **PocketBook Verse 실기에서 boot/render/scale-to-fit/애니메이션 부분갱신까지 검증됨**(2026-07-24). AI 주도 실험이고 입력·idle 고스팅·컬러 패널은 미검증.
- PR에 딸려온 **공유 변경 2개가 Kobo 이식의 실질 토대**:
  - `pocket-ui-surface` 크레이트 추출 — 전체 `ui` HostOps 표면 + pak 피딩 + DevTools 메일박스를 wgpu에서 분리. **비GPU 호스트가 HostOps 17개를 재구현하지 않아도 됨.**
  - `pocket-mod: Guest::frame_with_touches` — 3-인자 `frame(buttons, analog, touches)` 경로. 터치 기기에 필수.
- 재사용 가능한 불변 층: `pocketjs-core`(DrawList + `raster::render_scaled`), 프레임워크(Solid/Vue Vapor 렌더러, host-agnostic), 폰트 베이커, pak 파이프라인.
- **PocketJS 제공 데모/앱** — `hero`(PR에서 `bun pocket compile --target pocketbook`로 빌드 확인됨) 외에 `demos/`·`apps/`에 여러 개. 실제 목록은 에이전트가 §4에서 열거한다.

### 막혀 있는 것
- **Kobo 호스트(`hosts/kobo/`)는 아직 없다.** 새로 써야 한다.
- 메인테이너(doodlewind)가 #172에 대해 *"호스트를 더 확장 가능하게 만드는 아키텍처 리팩터 후 랜딩하겠다"* 고 명시. → **지금 fork를 그대로 베끼면 리팩터에 갈린다.** 공유 크레이트(`pocket-ui-surface`, `frame_with_touches`)는 durable, 호스트 내부 구조는 churn 예상.
- 미해결 설계 질문(작성자→메인테이너, 답 없음): compat 뷰포트 유지 + 데스크톱식 동적 뷰포트 네이티브 타깃 분리? **이 답이 Kobo에서 패널 전체(1024×758)를 쓸 수 있는지를 결정**(§7).

### 실행 순서 (호스트가 스파인)

| 페이즈 | 내용 | 상태 |
| --- | --- | --- |
| **P1** | 환경 셋업 + **PocketJS 제공 데모가 데스크톱/레퍼런스 호스트에서 빌드·구동** 확인(§3–4) | 지금 |
| **P2** | `hosts/kobo/` 작성 — FBInk/mxcfb present + evdev 입력(§5) | #172 상태 확인 후 |
| **P3** | **스톡 데모를 Kobo 실기에 배포·검증**(§6) | P2 후 |
| **P4** | (지연) 커스텀 앱 + net 서피스 + 한글 폰트(§8) | P3 후 |

---

## 2. 왜 PocketBook 호스트가 Kobo에서 안 도나 (교체 지점 지도)

`hosts/pocketbook/`은 **inkview SDK**(`libinkview.so` dlopen)에 의존. Kobo엔 그런 SDK가 없다. Kobo는 **raw framebuffer(`/dev/fb0`) + i.MX EPDC ioctl(`mxcfb`)** 이고, 이를 감싸는 표준 라이브러리가 **FBInk**(NiLuJe). 따라서 Kobo 호스트 = PocketBook 호스트에서 **present/refresh/input 층만 교체**.

| 파일 | PocketBook | Kobo에서 | 작업량 |
| --- | --- | --- | --- |
| `UiSurface` 배선 + pak 피딩 | `pocket-ui-surface` | **그대로** | 0 |
| 래스터화 | `raster::render_scaled` → RGBA8 | **그대로** | 0 |
| `framebuffer.rs` | RGBA8→RGB24 blit, gray 변환은 inkview에 위임 | **RGBA8→Gray8/RGB565 직접 변환** 후 `/dev/fb0` mmap 쓰기. Kobo엔 위임할 변환이 없음 | **실질 재작성** |
| `refresh.rs` | inkview partial/dynamic/full | **정책은 그대로**, 호출을 mxcfb 파형(A2/DU/GC16)으로 매핑 | 소폭 수정 |
| `input.rs` | inkview 키/터치 콜백 | **evdev `/dev/input/event*`** (Glo = Neonode IR 터치, 절대 X/Y) | 이벤트 소스 교체 |
| `main.rs` | `iv_main` 블로킹 루프 + 채널 + 2스레드 | **자체 단일 루프**, 이벤트 구동 틱 | **오히려 단순화** |
| 크로스 타깃 | `armv7-unknown-linux-gnueabi` (soft-float) | **`armv7-unknown-linux-gnueabihf`** (hard-float) | 트리플 변경 + bindgen 재생성 |

---

## 3. 환경 셋업 (P1, 에이전트 자율 수행)

```bash
mkdir -p ~/kobo-pocketjs && cd ~/kobo-pocketjs

# 1) PocketJS 업스트림 + PR #172 브랜치(레퍼런스)
git clone https://github.com/pocket-stack/pocketjs.git
git -C pocketjs remote add fork https://github.com/wonderbeel/pocketjs.git
git -C pocketjs fetch fork feat/pocketbook-inkview-host   # 레퍼런스로만 사용

# 2) inkview-rs (레퍼런스: refresh/screen 전략을 읽기 위함. Kobo엔 링크 안 함)
git clone https://github.com/simmsb/inkview-rs.git

# 3) FBInk (Kobo present 층의 실제 의존성)
git clone https://github.com/NiLuJe/FBInk.git

# 4) 러스트 툴체인
rustup toolchain install stable
rustup target add armv7-unknown-linux-gnueabihf   # Kobo = hard-float
cargo install cargo-zigbuild
# zig, libclang(bindgen용) 필요 — 아래 GATE에서 확인

# 5) JS 프레임워크 빌드 도구 (bun)
curl -fsSL https://bun.sh/install | bash   # 이미 있으면 생략
```

**`GATE 3`:**
```bash
zig version && cargo zigbuild --version && bun --version && clang --version
cd ~/kobo-pocketjs/pocketjs && cargo check --workspace   # 엔진 워크스페이스 빌드
```
- `zig`/`libclang` 없으면 `STOP` → 사람에게 설치 요청(환경별 상이).
- `cargo check` 실패 시 로그 보고 후 `STOP`.

---

## 4. PocketJS 제공 데모 선정·빌드 (P1)

호스트를 세우기 전에 **어떤 스톡 콘텐츠로 검증할지 확정**한다. 커스텀 앱은 짜지 않는다.

### 4-1. 실제 제공 데모 열거
```bash
cd ~/kobo-pocketjs/pocketjs
ls demos/ apps/ 2>/dev/null           # 실제 목록 확인
grep -rn "pocket compile\|--target" package.json tools/ 2>/dev/null | head
```
`read`로 각 데모의 성격(정적 UI / 애니메이션 / 3D·게임 / 뷰어)을 파악.

### 4-2. e-ink 적합성 기준으로 선별
| 유형 | e-ink 적합 | 용도 |
| --- | --- | --- |
| 정적·이산 UI 데모 (`hero` 류) | **최적** | **1차 검증 대상.** 상태 변화 시에만 present, 정지 시 전력 0 |
| 뷰어 (pocket-figma 류) | 조건부 | 연속 팬/줌은 e-ink 최악. 페이지/스텝 점프로 바꾸면 가능. 후순위 스트레스 테스트 |
| 게임·3D (OpenStrike 류) | 부적합 | 60fps 전제라 e-ink와 싸움. 단 "렌더가 되긴 하나" 스모크 테스트로는 유용 |

**1차 타깃 = `hero`** (PR에서 pocketbook 타깃 빌드 확인됨). 정적 UI라 e-ink 정합이 가장 좋다.

### 4-3. 타깃 프로필로 빌드
뷰포트는 §7 결정에 따르되, **시작은 레퍼런스의 compat 프로필(480×272 @2x = 960×544 렌더)** 을 그대로 쓴다.
```bash
bun pocket compile --target pocketbook   # hero 및 480x272 integer-fit 앱이 빌드됨
# (Kobo 전용 프로필을 아직 안 만들었으므로 pocketbook compat을 재사용)
```

### 4-4. 데스크톱에서 렌더 확인
```bash
# 데스크톱 wgpu 호스트로 선정 데모 실행 (정확한 타깃명은 read로 확인)
cargo run -p <wgpu-uihost-예제> -- --app <선정데모경로>
```
**`GATE 4`:** 선정 데모가 pak으로 빌드되고 데스크톱 창에서 정상 렌더. → 스톡 콘텐츠·JS 파이프라인 확정. 이후 Kobo 호스트가 이걸 그대로 띄우면 성공.

---

## 5. Kobo 호스트 구현 (P2, `hosts/kobo/`)

```
STOP — 착수 판단:
  PR #172 상태 확인(랜딩됐나? doodlewind의 "아키텍처 리팩터"가 머지됐나?).
  - 랜딩+리팩터 완료 → 리팩터된 hosts/pocketbook 구조를 베이스로(권장).
  - 아직 draft → 현재 fork를 베이스로 프로토타이핑하되 "구조는 바뀐다" 전제.
    공유 크레이트(pocket-ui-surface, frame_with_touches)만 안정 축으로 신뢰.
```

### 5-0. 레퍼런스 정독 (코드 전 필수) — §부록 A
특히 `hosts/pocketbook/src/{main,framebuffer,refresh,input}.rs`, `pocketjs-inkview-implementation.md`, `inkview-rs/.../screen.rs` + `inkview-slint`, `FBInk/fbink.h`, `engine/core/src/raster.rs`, `hosts/psp/src/main.rs`.

### 5-1. 크레이트 스캐폴드
`hosts/kobo/`를 `hosts/pocketbook/` 구조로 생성. `Cargo.toml`:
- 유지: `pocketjs-core`(features=["std"]), `pocket-mod`, `pocket-ui-surface`
- 제거: `inkview`
- 추가: FBInk 연동(아래 STOP)
- `[profile.release]`: `opt-level="s"`, `lto=true`, `strip=true`, `panic="abort"`
```
STOP — FBInk 연동 방식(사람):
  (a) libfbink FFI 바인딩(bindgen) — quirk 흡수를 FBInk에 위임(권장)
  (b) raw mxcfb ioctl 직접(nix/ioctl!) — 의존성 최소, Glo 회전/bpp/파형 상수 직접 처리
```

### 5-2. `framebuffer.rs` — 실질 재작성
1. `raster::render_scaled(ui, words, &mut rgba)` → RGBA8 (그대로)
2. fb 포맷 조회 — `fbink_get_state`(또는 `FBIOGET_VSCREENINFO`)로 **bpp·회전·stride 런타임 감지**. Glo는 통상 16bpp RGB565로 알려졌으나 **하드코딩 금지, 런타임 감지**. Kobo 회전/반전 quirk는 FBInk가 안다.
3. RGBA8 → fb 포맷 변환(그레이면 luminance `0.2125R+0.7154G+0.0721B`, RGB565면 packing). 흰 배경 위 알파 컴포짓.
4. **16×16 타일 damage diff** 유지. 단 **변환 후 값으로 diff** — Kobo에선 동일 휘도의 색상 변화는 무변화이므로 damage 감소(PocketBook은 RGBA로 diff해 보수적 과보고).
5. dirty 타일만 mmap된 `/dev/fb0`에 쓰기(또는 `fbink_print_raw_data`).
```
GATE 5-2: 데스크톱에서 raster→gray 변환 결과를 PNG 덤프해 육안 검증 후 실기 진행.
```

### 5-3. `refresh.rs` — 정책 유지, 파형 매핑
레퍼런스 정책(idle=partial, 모션 중=throttled dynamic, 200ms quiet cleanup, 주기적 full로 고스팅 리셋)을 **그대로**, 호출만 Kobo 파형으로:

| 레퍼런스(inkview) | Kobo(mxcfb via FBInk) | 용도 |
| --- | --- | --- |
| PartialUpdate | `WFM_GC16`(부분) 또는 `WFM_DU` | 정착 화면, 고품질 |
| DynamicUpdate | `WFM_A2` | 모션 중 빠름/저품질(잔상 감수) |
| FullUpdate | `WFM_GC16` full + 플래시 | 전환·고스팅 리셋 |
```
STOP — Glo(Pearl 패널) 지원 파형 확인:
  read: FBInk 파형 enum + Kobo Glo 항목. A2/DU/GC16은 대개 가능, REAGL류는 세대별 상이.
```

### 5-4. `input.rs` — evdev로 교체
inkview 콜백 → **`/dev/input/event*` evdev**. Glo = Neonode IR 터치(단일 접점, 절대 X/Y). `input_event` → `frame_with_touches` packed 포맷.
- **터치 와이어 = `(id<<18)|(y<<9)|x` → x,y 각 9비트 = 최대 511px.** §7 뷰포트 제약의 근원.
- 물리버튼 최소이므로 터치 위주. BTN 비트마스크는 `read`로 확인.
```
STOP — 실기 확인: Glo 터치 event 노드 번호·좌표계(회전/반전)는 기기에서 evtest로 확인. 5-4는 §6 배포 후 실기 튜닝.
```

### 5-5. `main.rs` — 자체 루프
`iv_main` 채널/2스레드 **제거**. 단일 루프가 이벤트 소유:
```
loop:
  ev = poll(evdev, timeout)                # 블로킹 + 타임아웃
  tick 조건: (1) 터치 입력  (2) 타이머  (3) 첫 진입   # net은 P4에서 추가
  if tick:
    input.drain(); guest.frame_with_touches(...); guest.drain_jobs()
    ui.tick(dt); words = ui.draw()
    pipeline.rasterize(ui, words); dirty = pipeline.diff()
    if dirty: refresh.present(dirty)        # 파형 선택 포함
  else:
    # 무변화 → present 안 함. e-ink가 전력 0으로 화면 유지
```
고정 60Hz 불필요. 이벤트 구동이 배터리·고스팅에 유리.

### 5-6. 크로스컴파일
```bash
cd ~/kobo-pocketjs/pocketjs/hosts/kobo
cargo zigbuild --release --target armv7-unknown-linux-gnueabihf.2.15   # 버전은 GATE에서 확정
```
레퍼런스에서 **그대로 가져올 두 픽스**:
- **glibc 수학 심**: LLVM 19+가 `f32::max/min`을 C23 심볼(`fmaximum_numf`, `fminimum_num`)로 낮춤 → 구형 glibc에 없어 링크 실패. `build.rs`가 `src/compat.c`로 심볼 공급(크로스빌드 전용). **Kobo도 동일 → 재사용.**
- **rquickjs bindgen**: ARM 타깃 pre-gen 바인딩 없음 → `bindgen` feature를 arm에만 활성(libclang 필요).
```
STOP — Kobo glibc 버전 확인(실기): strings /lib/libc.so.6 | grep -i "glibc 2" → zigbuild .2.x 핀 확정.
GATE 5-6: stripped ARM ELF 산출 + `file`로 EABI5/hard-float 확인.
```

---

## 6. 스톡 데모 실기 배포·검증 (P3)

```
STOP — Kobo 셸 접근(사람, 1회 셋업):
  개발 루프엔 SSH 최선: KOReader 내장 SSH 서버 또는 KTerm.
  "앱처럼 설치"는 후순위: KFMon(NiLuJe)이 라이브러리 항목 감지해 스크립트 실행.
```
```bash
# §4에서 빌드한 스톡 데모(hero) pak + 호스트 바이너리를 복사
scp target/armv7-unknown-linux-gnueabihf/release/kobo-host  kobo:/mnt/onboard/.apps/hero/
scp -r <hero-pak>/*                                          kobo:/mnt/onboard/.apps/hero/
ssh kobo 'cd /mnt/onboard/.apps/hero && ./kobo-host > pocketjs.log 2>&1'
# 레퍼런스 deploy.sh처럼 stdout/stderr를 pocketjs.log로 리다이렉트해 포스트모템 확보
```
**`GATE 6` — 실기 검증 순서(레퍼런스 체크리스트 준용):**
1. **boot/render** — `hero`가 고스팅 없이 뜨나 (← 최소 성공 기준)
2. **입력** — 터치 좌표 정확도 (evtest로 노드/축 확인 후 튜닝)
3. **refresh** — 갱신 시 파형·잔상 관찰, `full_update_interval` 튜닝
4. **idle 고스팅** — 장시간 후 리셋 동작

여기까지 통과하면 **"PocketJS 제공 데모가 Kobo에서 구동"이라는 1차 목표 달성.**

---

## 7. 뷰포트 결정 (가로지르는 이슈)

터치 9비트(511px) 제약 → 논리 뷰포트가 축당 511px 초과 불가. 레퍼런스는 **480×272 @2x(960×544 렌더)를 굽고 패널에 nearest-neighbor scale-to-fit 센터링**으로 회피(Verse 758×1024 → 758×429).

Kobo Glo(1024×758)도 동일. 선택:
- **안전(권장 시작):** compat(480×272 @2x) 재사용 → 즉시 동작, 패널 일부만 사용(대략 758×429).
- **패널 풀 사용:** `contracts/spec/platforms.ts`에 `kobo` 프로필 신설 + 터치 좌표 제약 우회 필요. **#172의 미해결 질문**(takeover 동적 뷰포트)에 물려 있음.
```
STOP — 뷰포트 전략: 1차 목표는 "동작"이므로 compat으로 시작. 풀패널은 #172 동적 뷰포트 결론 후 별도 과제.
```

---

## 8. (지연) 커스텀 앱 — 날씨 (P4, P3 통과 후)

호스트가 스톡 데모로 증명된 뒤에만 착수. 여기서부터가 커스텀 영역이다.

- **앱 이식:** 기존 날씨 웹앱 → Solid(또는 Vue Vapor). React면 기계적 포팅. 동적 클래스 금지(전체 리터럴 삼항만), Tailwind 서브셋, 애니메이션 배제.
- **한글 폰트:** 폰트 베이커는 쓰는 코드포인트만 굽고 vendored Inter엔 한글 없음. → 한글 폰트 vendor(Pretendard/Noto Sans KR) + **쓸 글자 빌드타임 열거.** API 응답을 직접 렌더하지 말고 `코드→구운 문자열 테이블`(도시·상태·숫자·요일 전부 유한)로 매핑.
- **net 서피스(신규):** RUNTIMES.md §5 절차로 `net` 서피스 선언. Kobo는 PocketJS 타깃 중 최초로 실제 네트워크가 있는 기기. ops=`net.request/abort`, events=`net.response/error`(Law 2: 의도는 op, 사실은 event). 호스트가 별도 스레드로 실제 HTTPS → 다음 틱 이벤트로. 수신은 `@pocketjs/framework/effects` shell이 프레임 경계에 양자화. 15분 타이머 재요청, 그 외 전력 0.
```
STOP — P4 착수 시 사람 확인: 기존 앱 스택/API/UI 언어(한글 여부 → 폰트 작업 규모 결정).
```

---

## 부록 A — 그라운드 트루스 파일 (코드 전 read)

| 파일 | 무엇을 알려주나 |
| --- | --- |
| `docs/RUNTIMES.md` | 3법칙, Runtime=⟨Cores, Surfaces, Guest⟩, 서피스 신설 절차(§5) |
| `contracts/spec/spec.ts` | op 코드, prop ID, DrawList 포맷, BTN 비트마스크, 터치 패킹 |
| `contracts/spec/platforms.ts` | 타깃 프로필(`pocketbook` 참조, `kobo` 추가 위치) |
| `engine/core/src/raster.rs` | `render` / `render_scaled` 시그니처 |
| `engine/core/src/draw.rs` | DrawList 8 opcode |
| `engine/crates/pocket-ui-surface/` | `UiSurface`, pak walker (Kobo 재사용) |
| `hosts/pocketbook/src/*.rs` | 교체 대상 원본(fork 브랜치) |
| `pocketjs-inkview-implementation.md` | 실기 대응 수정 이력(뷰포트·터치·glibc 심) |
| `hosts/psp/src/main.rs` | 프레임 루프 순서 레퍼런스 |
| `inkview-rs/.../screen.rs`, `inkview-slint` | refresh 전략 계보 |
| `FBInk/fbink.h`, `FBInk/README.md` | Kobo present 층 실제 API |

## 부록 B — STOP/사람 결정 지점
1. #172 랜딩/리팩터 상태 → 베이스 선택(§5-0)
2. FBInk FFI vs raw ioctl(§5-1)
3. Glo 지원 파형(§5-3)
4. Glo 터치 노드/좌표계 — evtest(§5-4)
5. Kobo glibc 버전 핀(§5-6)
6. 셸 접근 — SSH/KFMon(§6)
7. 뷰포트 전략 — compat vs 풀패널(§7)
8. (P4) 커스텀 앱 스택·API·언어(§8)

## 부록 C — 검증 게이트
- `GATE 3` 환경 빌드 · `GATE 4` 스톡 데모 데스크톱 렌더 · `GATE 5-2` 그레이 변환 PNG · `GATE 5-6` ARM ELF · `GATE 6` 실기 4단계(1차 목표 달성점)

---

### 전략 요약
- **1차 목표는 커스텀 앱이 아니라 "PocketJS 제공 데모(`hero`)가 Kobo에서 뜨는 것".** 성공 기준이 명확하고 스톡 콘텐츠라 변수 없이 호스트만 검증한다.
- **어려운 판단(파형 정책·damage·quiet cleanup·고스팅 리셋)은 이미 레퍼런스에 풀려 있다.** 새로 쓰는 실질은 `framebuffer.rs`의 gray 경로와 present를 mxcfb로 바꾸는 한 층.
- **#172 리팩터 관찰이 리스크 관리의 핵심.** 공유 크레이트는 신뢰, 호스트 내부 구조는 확정 후 착수.
- 커스텀 앱·한글·net은 P3 통과 후(§8).
