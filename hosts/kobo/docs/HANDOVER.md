# 인수인계 — 다른 머신에서 이어받기

이 문서만 따라 하면 새 머신에서 지금과 동일한 상태에 도달한다.
아래 §1–§3의 절차는 실제로 순정 트리에 적용해 **트리 해시 동일 + 테스트 31/31 통과**를
확인한 것이다(추정이 아님).

배경과 결정 근거는 [PROGRESS.md](PROGRESS.md), 원래 계획은
[cookbook.md](cookbook.md)를 본다.

---

## 0. 지금 상태 한 줄

`hosts/kobo`(Kobo Glo e-ink 호스트)는 **2026-09-05 실기에서 `GATE 6`를 통과했다.**
Kobo Glo(N613, FW 3.19.5761)에서 `paper-ink`가 758×1024 패널에 렌더되고, 터치가
누른 자리에 정확히 꽂히고, 잔상은 e-ink 기준 허용 범위였다.

**2026-09-07, nickel 없는 부팅에서 원격 셸이 열렸다.** 이제 배포는
`hosts/kobo/tools/kobo-push.py`로 네트워크를 탄다 — 카드를 뽑을 일이 없다. §4-1-c.

**2026-09-08, P4가 끝났다.** `net.http`가 실기에서 돈다 — HTTP와 HTTPS 둘 다
200 / 0.1초. TLS는 이 바이너리에 정적 링크했다(기기 OpenSSL이 2009년판이라
빌려 쓸 것이 없었다). 시계도 실제 시각을 표시한다: RTC가 출처, NTP가 보정.

STOP 5는 닫혔지만 **질문이 틀린 채로** 닫혔다 — §5를 본다. 남은 미해결은
suspend와, 고부하 세션이 12분 만에 죽던 건이다(로그를 잃어 원인 미상, 관찰
중단 — PROGRESS.md 2026-09-09). **RTC는 전원을 버티지 못한다**: 네트워크 없는
콜드 부팅은 2012년을 보여준다.

이 저장소(`pocket-kobo`)에는 호스트 코드가 없다. 코드는
**`github.com/cbcruk/pocketjs`, 브랜치 `feat/kobo-glo-host`**에 있다 —
upstream `pocket-stack/pocketjs`의 포크이지만 **기여하지 않는 독립 포크**다(§7).
`cbcruk/pocket-kobo`는 이 작업을 가리키는 저장소로만 남아 있다.

이 브랜치가 upstream에 더하는 것:

| 경로 | 무엇 |
| --- | --- |
| `hosts/kobo/` | 호스트 본체, 디바이스 스크립트, 문서, 도구 |
| `apps/paper-ink`, `apps/ink-clock` | 데모와 상태 화면 |
| `apps/hangul-probe`, `apps/ghost-probe`, `apps/net-probe` | 각각 한글·잔상·네트워크 판정용 |
| `contracts/spec/platforms.ts` | `kobo-glo` 프로파일 (`input.touch`, `text.glyphs.baked`, `net.http`) |
| `engine/core/src/raster.rs`, `tools/pocket.ts` | Gray8 경로와 타깃 등록에 필요한 최소 변경 |

커밋 단위의 이력은 `git log --oneline --reverse 2d20dda..HEAD`가 준다.

기기 시각이 2012년이면 `clock.sh sync`를 한 번 돌린다 — RTC에 써 두면 이후 부팅은 네트워크 없이도 맞다. 시간대는 `.apps/pocketjs/timezone` (서울은 `KST-9`).
| `PROGRESS.md` | 결정 근거, 게이트 결과, 남은 STOP |
| `cookbook.md` | 원래 런북 (일부 전제는 낡음 — PROGRESS.md의 대조표를 먼저 볼 것) |

---

## 1. 코드 가져오기

브랜치가 원격에 있다. 복원할 것이 없다.

```sh
mkdir -p ~/kobo-pocketjs && cd ~/kobo-pocketjs
git clone https://github.com/cbcruk/pocketjs.git
cd pocketjs
git checkout feat/kobo-glo-host
git remote add upstream https://github.com/pocket-stack/pocketjs.git
```

베이스는 upstream `2d20dda`다. `origin`이 이 작업의 저장소이고 upstream은
참조용이다 — §7을 본다.

무엇이 왜 바뀌었는지는 커밋 메시지가 갖고 있다. 이 브랜치는 upstream
`2d20dda` 위의 커밋 29개이고, 하나하나가 하나의 발견이다:

```sh
git log --oneline --reverse 2d20dda..HEAD
```

한때 이 자리에 `.patch` 파일 29개로 트리를 재구성하는 절차가 있었다. 브랜치가
이 맥에만 있던 시절의 운반 수단이었고, 원격이 생긴 뒤로는 낡을 수 있는 두 번째
복원 경로일 뿐이라 지웠다.

## 2. 환경 셋업

이 작업은 Ubuntu 24.04 / **aarch64** / 헤드리스에서 했다. x86_64에서도 절차는 같고
zig 타르볼 URL만 아키텍처에 맞게 바뀐다(아래 명령이 자동 처리).

```sh
# 1) 시스템 패키지 — libclang은 rquickjs bindgen에 필수
sudo apt-get update
sudo apt-get install -y clang libclang-dev build-essential pkg-config file binutils

# 2) Rust + 타깃
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y \
  --default-toolchain stable --profile minimal
. "$HOME/.cargo/env"
rustup target add armv7-unknown-linux-musleabihf

# 3) zig (cargo-zigbuild 링커)
ZIG=0.15.2
ARCH=$(uname -m)   # aarch64 | x86_64
curl -sSL -o /tmp/zig.tar.xz \
  "https://ziglang.org/download/$ZIG/zig-$ARCH-linux-$ZIG.tar.xz"
mkdir -p ~/.local/zig && tar -xJf /tmp/zig.tar.xz -C ~/.local/zig --strip-components=1
~/.local/zig/zig version    # 0.15.2

# 4) cargo-zigbuild
cargo install cargo-zigbuild

# 5) bun
curl -fsSL https://bun.sh/install | bash
```

확인된 버전 조합 (다른 조합은 검증 안 됨):

```
rustc 1.98.1 / cargo 1.98.1     cargo-zigbuild 0.23.4
zig 0.15.2                       clang 18.1.3 (libclang: /usr/lib/llvm-18/lib)
bun 1.4.0                        node v24.19.0
```

> zig 0.16이 이미 나와 있지만 cargo-zigbuild 0.23.4와의 조합은 확인하지 않았다.
> 0.15.2로 고정할 것.

**macOS(arm64)에서도 전부 재현된다** — 게이트 (a)–(f) 동일. 바뀌는 것만:

```sh
rustup target add armv7-unknown-linux-musleabihf
cargo install cargo-zigbuild
curl -sSL -o /tmp/zig.tar.xz https://ziglang.org/download/0.15.2/zig-aarch64-macos-0.15.2.tar.xz
mkdir -p ~/.local/zig && tar -xJf /tmp/zig.tar.xz -C ~/.local/zig --strip-components=1

# libclang은 Command Line Tools 것을 쓴다
export LIBCLANG_PATH=/Library/Developer/CommandLineTools/usr/lib
export CLANG_PATH="$(xcrun --find clang)"
```

rustc는 1.95.0에서도 통과했다. 즉 핀이 필요한 것은 **zig 0.15.2뿐**이다.

---

## 3. 검증 — 여기까지는 하드웨어 없이 재현된다

순서대로 돌리고 기대값과 대조한다. 하나라도 어긋나면 §5를 보기 전에 여기서 멈춘다.

```sh
cd ~/kobo-pocketjs/pocketjs
bun install
```

**(a) 코어 래스터라이저** — gray8 4건 포함

```sh
(cd engine/core && cargo test)
# 기대: 133 passed; 0 failed
```

**(b) 호스트 단위 테스트** — Kobo 치수 geometry, non-MT 싱글터치 포함

```sh
cargo test --manifest-path hosts/kobo/Cargo.toml
# 기대: 31 passed; 0 failed
```

**(c) 플랫폼 계약**

```sh
bun test tests/platform-contracts.test.ts tests/symbian-runtime.test.ts
# 기대: 0 fail
```

> 전체 `bun test`는 **39건이 실패하는데 전부 순정 main에서도 실패하는 기존 건**이다
> (대부분 Vue Vapor oracle). 순정 main은 728 pass / 40 fail, 이 브랜치는
> 730 pass / 39 fail(신규 테스트 +1). 놀라지 말 것.

**(d) 앱 빌드**

```sh
bun tools/pocket.ts compile --target kobo-glo \
  --manifest apps/paper-ink/pocket.json --project-root .
# 기대: "✓ kobo-glo satisfies pocket.json capabilities", raster=2x
#       dist/paper-ink-main.{js,pak} 생성
```

**(e) 렌더 육안 확인 — 하드웨어 없이 가능한 마지막 게이트**

```sh
cargo run --manifest-path hosts/kobo/Cargo.toml --example render_gray -- \
  dist/paper-ink-main.js dist/paper-ink-main.pak /tmp/paper-ink
# 기대: "/tmp/paper-ink.pgm — 758x1024 Gray8, 6924 dark px (0.9%), 92 distinct levels"
```

`/tmp/paper-ink.pgm`을 열어 제목 "PAPER / INK", 괘선, 하단 "AUTO PARTIAL / SAMPLES 0"
상태바가 보이면 파이프라인이 정상이다. 프레임이 단색이면 예제가 nonzero로 종료한다.

**터치도 같이 돌린다.** 유휴 첫 프레임은 앱의 극히 일부만 실행한다 — `paper-ink`는
접촉이 생겨야 잉크 노드를 만들고, 그 노드가 쓰는 스타일 prop도 그때 처음 검증된다.
이걸 빼먹은 탓에 실기에서 첫 터치에 죽는 번들이 이 게이트를 통과했었다:

```sh
cargo run --manifest-path hosts/kobo/Cargo.toml --example render_gray -- \
  dist/paper-ink-main.js dist/paper-ink-main.pak /tmp/paper-ink-touch --touch 180,300
# 기대: 758x1024 Gray8, 7374 dark px (1.0%), 117 distinct levels
```

**(e2) 한글 경로** — P4를 이어받는다면 여기까지 확인한다

기본 Inter에는 한글 cmap이 없고 **매핑 없는 코드포인트는 조용히 두부가 된다.**
빌드는 성공하므로 반드시 눈으로 본다. 절차와 비용은 PROGRESS.md의
"한글 폰트" 절에 있다. 요약하면 폰트를 받아서 `tools/build.ts`를 직접 부른다:

```sh
curl -sSLO https://github.com/google/fonts/raw/main/ofl/nanumgothic/NanumGothic-Regular.ttf
curl -sSLO https://github.com/google/fonts/raw/main/ofl/nanumgothic/NanumGothic-Bold.ttf
bun tools/pocket.ts compile --target kobo-glo \
  --manifest apps/hangul-probe/pocket.json --project-root .
bun tools/build.ts --plan=.pocket/kobo-glo/plan.json --project-root=. \
  --outdir=dist --hz=60 \
  --font-regular=$PWD/NanumGothic-Regular.ttf --font-bold=$PWD/NanumGothic-Bold.ttf
# 기대: font: slot 2 (16px) 169 glyphs ... / pak: 6 entries, 881872 bytes
cargo run --manifest-path hosts/kobo/Cargo.toml --example render_gray -- \
  dist/hangul-probe-main.js dist/hangul-probe-main.pak /tmp/hangul-probe
# 기대: 758x1024 Gray8, 18671 dark px (2.4%), 124 distinct levels
```

`/tmp/hangul-probe.pgm`에 "서울 날씨"와 3일 예보 표가 **두부 없이** 보여야 한다.

**(f) ARM 크로스컴파일**

```sh
CARGO_ZIGBUILD_ZIG_PATH="$HOME/.local/zig/zig" \
CLANG_PATH=/usr/bin/clang \
LIBCLANG_PATH=/usr/lib/llvm-18/lib \
  cargo zigbuild --manifest-path hosts/kobo/Cargo.toml \
  --release --target armv7-unknown-linux-musleabihf

file hosts/kobo/target/armv7-unknown-linux-musleabihf/release/pocketjs-kobo
# 기대: ELF 32-bit LSB executable, ARM, EABI5 ... statically linked, stripped
readelf -h hosts/kobo/target/armv7-unknown-linux-musleabihf/release/pocketjs-kobo | grep Flags
# 기대: Flags: 0x5000400, Version5 EABI, hard-float ABI
```

macOS에는 `readelf`가 없고 `file`도 float ABI를 안 찍는다. ELF 헤더에서 직접 읽는다:

```sh
python3 -c "
import struct
d = open('hosts/kobo/target/armv7-unknown-linux-musleabihf/release/pocketjs-kobo','rb').read(64)
print(hex(struct.unpack_from('<I', d, 0x24)[0]))"
# 기대: 0x5000400
```

`hard-float`과 `statically linked` 두 가지를 반드시 눈으로 확인한다. soft-float
바이너리는 Glo에서 조용히 오작동하고, 동적 링크는 펌웨어 glibc에 묶인다.

**(g) 디바이스 스크립트** — nickel 복구 계약을 하드웨어 없이 검증한다

```sh
hosts/kobo/tests/device-scripts.sh
# 기대: device scripts: all checks passed  (19개 체크)
```

스테이지 루트(`NICKEL_ROOT`)와 스텁 `pidof`/`killall`/`usleep`을 깔고 **실제
`device/*.sh`를 그대로 돌린다.** 정상 종료·호스트 실패·중복 실행·죽은 락·FBInk
부재 다섯 경로에서 UI가 돌아오는지(또는 애초에 안 멈추는지)를 본다.

---

## 4. 실기 작업 — 여기서부터가 진짜 남은 일

### 4-1. 준비물

| 항목 | 비고 |
| --- | --- |
| Kobo Glo (Kraken, N613) | 다른 모델은 호스트가 geometry를 거부한다. §6 참조 |
| 기기 셸 | 아래 4-1-a |
| FBInk 바이너리 (Kobo용 ARM) | 아래 4-1-b |

#### 4-1-a. 셸 확보 — 실제로 통한 경로

**USB가 아예 안 됐다.** macOS(Apple Silicon)에서도 Windows에서도 Glo가 USB 장치로
열거되지 않았다. 기기에는 연결 확인 화면이 뜨는데(Kobo는 VBUS만 감지돼도 띄운다)
호스트의 USB 트리에는 나타나지 않는다. 케이블·허브·OS를 다 바꿔도 같았다.

**해결: microSD를 빼서 카드 리더로 직접 쓴다.** Glo의 저장소는 뒷커버를 벗기면 나오는
microSD 카드 그 자체이고, 그 **3번 파티션이 FAT32 `KOBOeReader` = `/mnt/onboard`** 다.
USB로 하려던 두 가지(conf 편집, 파일 배포)가 전부 그 파티션 위에 있다.

```
/dev/diskN  4.0GB, FDisk_partition_scheme
  1: Linux        268.4 MB      rootfs
  2: Linux        268.4 MB      recovery
  3: DOS_FAT_32   KOBOeReader   ← 이게 /mnt/onboard
```

카드를 꽂았을 때 macOS가 **"디스크를 읽을 수 없습니다 — 초기화"** 를 띄우면 반드시
**"무시"**. ext4 두 개를 못 읽어서 나오는 정상 반응이고, 초기화를 누르면 OS가 날아간다.
디스크 유틸리티의 "복구"/"지우기"도 금지. 편집 전 conf 원본을 따로 백업하고,
가능하면 `sudo dd if=/dev/rdiskN of=card.img bs=4m`로 전체 이미지를 떠 둔다
(카드가 기기의 유일한 OS다).

`.sh` 파일은 텍스트 편집기로 열지 말 것 — CRLF가 되면 실행되지 않는다. 복사만 한다.
macOS는 `cp -X`로 확장속성을 빼면 `._` 잔재가 안 생긴다.

셸 자체는 두 갈래. **바꾸는 게 적은 순서**로 적는다.

1. **펌웨어 디버그 서비스.** USB로 붙여 `.kobo/Kobo/Kobo eReader.conf` 끝에 붙인다:

   ```ini
   [DeveloperSettings]
   EnableDebugServices=true
   ForceWifiOn=true
   ```

   되꽂고 리부트 → Wi-Fi 연결 → *설정 → 기기 정보*에서 IP 확인 →
   `telnet <ip>`, 계정 `root`, **비밀번호 없음**. 두 줄을 지우면 원상복구다.
   (`ForceWifiOn`은 유휴 시 Wi-Fi가 꺼져 세션이 끊기는 걸 막는다.)

   **FW 3.19.5761에서 이 방법이 실제로 통했다.** 리부트 후 21·23 포트가 열린다
   (ICMP는 막혀 있으니 `ping`으로 판단하지 말고 포트를 본다).

   macOS에는 telnet도 ftp 클라이언트도 없다. 이 저장소의 `hosts/kobo/tools/kobo-sh.py`가
   busybox telnetd의 옵션 협상까지 처리하고 명령 하나를 실행해 준다:

   ```sh
   hosts/kobo/tools/kobo-sh.py 192.168.50.x "cd /mnt/onboard/.apps/pocketjs && ./diagnose.sh"
   ```

   `nc`로는 안 된다 — 협상을 안 하므로 로그인 프롬프트조차 못 받는다.
2. **KOReader.** *Tools → SSH*가 진짜 SSH 데몬이다. 이쪽이 **FBInk까지 같이
   해결**하므로 결국 어차피 깔게 될 가능성이 높다.

telnet 경로가 안 먹으면(펌웨어 버전에 따라 다르다) MobileRead 위키의
`inetd.conf` + `inittab` 을 넣는 `KoboRoot.tgz` 방식이 문서화된 대안이다.

#### 4-1-c. nickel 없는 부팅에서 셸 열기 — 세 가지가 다 nickel의 일이었다

`REMOTE` 파일만 있으면 런처가 알아서 한다. 왜 세 번 실패했는지는 남겨 둔다.
이 펌웨어(2.1.5)를 다시 만나면 같은 순서로 막힌다.

1. **Wi-Fi 모듈 경로.** `wifi.sh`가 `PLATFORM`으로 경로를 조립했는데 2.1.5는
   그 변수를 export하지 않는다. 결과는 `/drivers//wifi/.ko`. rcS가 이미
   `WIFI_MODULE_PATH`를 완성해 export하므로 그걸 쓴다 (0015).
2. **`telnetd` 파일이 없다.** `/bin`·`/sbin`·`/usr/bin`·`/usr/sbin` 어디에도
   심볼릭 링크가 없다. 애플릿은 busybox 바이너리 안에만 있다 —
   `strings busybox | grep -x telnetd`로 확인된다. `/bin/busybox telnetd`로
   부른다 (0016).
3. **pty가 없다.** 2.1.5의 rcS는 devpts를 마운트하지 않는다 (`grep devpts rcS` →
   없음). nickel이 하던 일이다. `/dev`는 매 부팅 새 tmpfs라 `/dev/ptmx`도
   없을 수 있다. 커널에는 둘 다 들어 있으니 (`strings vmlinux | grep devpts`)
   마운트하고 필요하면 `mknod`한다 (0016).

로그인은 펌웨어 기본값 그대로다: `root`, 비밀번호 없음, `/bin/login`.

**진단이 안 되던 진짜 이유는 따로 있었다.** 런처가 `[ -t 1 ]`로 "사람이 보고
있는가"를 판단했는데, init이 rcS에 넘기는 **콘솔도 터미널이다**. 그래서 아무도
읽지 않는 부팅에서 정확히 로그를 안 남겼다. pty만 사람이 있다는 뜻이다 (0017).

```sh
hosts/kobo/tools/kobo-push.py <IP> /mnt/onboard/.apps/pocketjs \
    ~/kobo-pocketjs/pocketjs/hosts/kobo/device/pocketjs.sh
hosts/kobo/tools/kobo-sh.py <IP> "cat /mnt/onboard/.apps/pocketjs/launcher.log"
```

기기 IP는 UI가 없어서 화면으로 알 수 없다. 라우터에서 보거나 `arp -an`으로 찾는다.

#### 4-1-b. FBInk

FBInk는 GitHub Releases에 **소스 tarball만** 올린다(프리빌트 Kobo 바이너리 없음).
문서화된 빌드 경로는 [koxtoolchain](https://github.com/koreader/koxtoolchain)으로
크로스 툴체인을 만든 뒤 `make kobo`다.

**지름길: KOReader 릴리스 zip에서 `fbink` 하나만 꺼내면 된다.** KOReader를 설치할
필요도 없다 — 40MB zip에서 파일 하나를 뽑아 올리면 끝이다. 실제로 쓴 방법:

```sh
curl -sSL -o koreader-kobo.zip \
  https://github.com/koreader/koreader/releases/download/v2026.07.1/koreader-kobo-v2026.07.1.zip
unzip -o -j koreader-kobo.zip koreader/fbink -d .
file fbink   # ELF 32-bit LSB, ARM EABI5, dynamically linked, for GNU/Linux 2.6.33
strings fbink | grep -oE 'GLIBC_2\.[0-9]+' | sort -uV   # GLIBC_2.4 뿐
```

**GLIBC_2.4만 요구**하므로 2016년 펌웨어에서 그대로 돈다. 기기의
`.apps/pocketjs/bin/fbink`에 두면 `device/pocketjs.sh`가 찾는다.
KOReader를 실제로 설치했다면 `/mnt/onboard/.adds/koreader/fbink`도 자동 탐색한다.

동작 확인은 `fbink -e` 한 방이면 된다 — 기기 신원과 터치 퀵을 한 줄로 뱉는다:

```
deviceName='Glo' deviceCodename='Kraken' devicePlatform='Mark 4'
viewWidth=758 viewHeight=1024 BPP=16 lineLength=1536 FBID='mxc_epdc_fb'
isKoboNonMT=1 touchSwapAxes=1 touchMirrorX=1 touchMirrorY=0 pixelFormat='BGR565'
```

### 4-2. `make devcap` — 막혔을 때의 예비 진단

§4-4의 `diagnose.sh` + `--probe-touch`가 §5의 항목 2·3을 덮으므로 보통은 필요 없다.
그래도 결과가 앞뒤가 안 맞으면 FBInk의 진단 번들이 독립적인 2차 의견이 된다:

```sh
cd ~/kobo-pocketjs/FBInk && make devcap    # devcap_test.sh + 바이너리 tarball
```

Glo에서 돌리면 프레임버퍼 방향, bpp, 터치 축 정보가 FBInk의 기기표 관점으로 나온다.

### 4-3. 배포 — USB로 끌어다 놓는 게 제일 빠르다

`/mnt/onboard`가 **USB로 붙였을 때 보이는 그 드라이브 자체**다. 즉 scp도 ftp도 필요 없다.
셸을 여는 데 어차피 USB를 한 번 쓰므로 그때 같이 넣는다.

먼저 여섯 개를 한 폴더에 모은다:

```sh
mkdir -p ~/kobo-pocketjs/deploy/pocketjs && cd ~/kobo-pocketjs/pocketjs
cp hosts/kobo/target/armv7-unknown-linux-musleabihf/release/pocketjs-kobo \
   ~/kobo-pocketjs/deploy/pocketjs/
cp dist/paper-ink-main.js  ~/kobo-pocketjs/deploy/pocketjs/app.js
cp dist/paper-ink-main.pak ~/kobo-pocketjs/deploy/pocketjs/app.pak
cp hosts/kobo/device/{pocketjs,nickel,diagnose}.sh ~/kobo-pocketjs/deploy/pocketjs/
```

Kobo를 USB로 붙이고(macOS 기준 `/Volumes/KOBOeReader`) 통째로 복사한다.
Finder는 점으로 시작하는 폴더를 숨기므로 터미널에서 하는 편이 낫다:

```sh
mkdir -p /Volumes/KOBOeReader/.apps
cp -R ~/kobo-pocketjs/deploy/pocketjs /Volumes/KOBOeReader/.apps/
dot_clean /Volumes/KOBOeReader   # macOS가 뿌린 ._ 파일 정리 (선택)
diskutil eject /Volumes/KOBOeReader
```

`scp`가 되는 환경(KOReader SSH)이라면 같은 여섯 개를
`kobo:/mnt/onboard/.apps/pocketjs/`로 보내면 된다. 결과는 동일하다.

#### macOS에서 Glo가 안 붙으면 (실제로 겪음)

2026-09-05, Apple Silicon / macOS 26.5.1에서 **Glo가 USB 장치로 열거되지 않았다.**
기기에는 연결 확인 화면이 뜨는데(Kobo는 VBUS만 감지돼도 띄운다) 호스트 쪽
`ioreg -p IOUSB`에 나타나지 않는다. 맥의 USB 스택 자체는 정상이었다 — 같은 트리에
모니터 허브와 카드 리더가 잡혀 있었다. 이 세대 Kobo와 최신 macOS 조합의 알려진
문제로 보인다.

**해결: 파일 투입만 Windows에서 한다.** 명령은 아래와 같고, 탐색기는 점으로 시작하는
폴더를 못 만들므로 `cmd`를 쓴다(`E:`는 실제 드라이브 문자로 바꾼다):

```bat
set K=E:
mkdir %K%\.apps
xcopy /E /I pocketjs %K%\.apps\pocketjs

copy "%K%\.kobo\Kobo\Kobo eReader.conf" "%K%\.kobo\Kobo\Kobo eReader.conf.bak"
>> "%K%\.kobo\Kobo\Kobo eReader.conf" echo.
>> "%K%\.kobo\Kobo\Kobo eReader.conf" echo [DeveloperSettings]
>> "%K%\.kobo\Kobo\Kobo eReader.conf" echo EnableDebugServices=true
>> "%K%\.kobo\Kobo\Kobo eReader.conf" echo ForceWifiOn=true
```

**`.sh` 파일을 메모장으로 열어 저장하지 말 것.** CRLF가 되면 기기에서 실행되지 않는다.
복사만 한다.

#### USB가 필요한 건 이 한 번뿐이다

개발 서비스는 telnet(23)과 함께 **FTP(21)도 연다.** 그 뒤로는 빌드 머신에서 바로 민다:

```sh
curl -T dist/paper-ink-main.js  ftp://<IP>/mnt/onboard/.apps/pocketjs/app.js
curl -T dist/paper-ink-main.pak ftp://<IP>/mnt/onboard/.apps/pocketjs/app.pak
curl -T hosts/kobo/target/armv7-unknown-linux-musleabihf/release/pocketjs-kobo \
     ftp://<IP>/mnt/onboard/.apps/pocketjs/
```

셸도 네트워크 너머다. 즉 **빌드와 개발 루프는 계속 원래 머신에서 하고**,
Windows는 최초 1회 파일 투입에만 쓴다.
macOS에는 telnet도 ftp 클라이언트도 없다 — 접속은 `nc <IP> 23`
(또는 `brew install telnet`), 업로드는 위처럼 `curl -T`를 쓴다.

#### 실행 비트

`/mnt/onboard`는 FAT32라 퍼미션을 저장하지 않는다. 실행 비트는 마운트 옵션이 정한다.
**스크립트는 이 문제를 우회할 수 있다** — 아래 예시가 전부 `sh ./...`로 부르는 이유다
(런처는 자기를 `/tmp`로 복사한 뒤 거기서 `chmod +x` 하므로 그다음은 문제없다).

바이너리는 우회가 안 된다. `./pocketjs-kobo --probe`가 `Permission denied`면 rootfs로 옮긴다:

```sh
mkdir -p /usr/local/pocketjs
cp /mnt/onboard/.apps/pocketjs/pocketjs-kobo /usr/local/pocketjs/
chmod +x /usr/local/pocketjs/pocketjs-kobo
# 이후 POCKETJS_BIN=/usr/local/pocketjs/pocketjs-kobo 로 런처에 알려 준다
```

### 4-4. 첫 접촉 — 읽기 전용부터

```sh
cd /mnt/onboard/.apps/pocketjs && sh ./diagnose.sh
```

nickel을 멈추지도, `/dev/fb0`에 쓰지도 않는다. 펌웨어 버전, 프레임버퍼 실측치,
`fbink -e` 출력, `/proc/bus/input/devices`, 그리고 호스트 `--probe`까지 한 번에 나온다.

`--probe`가 758×1024(또는 회전형 1024×758)를 보고해야 한다.
**여기서 값이 다르면 실행하지 말고 §6을 본다.**

이어서 터치 축을 확정한다. 이것도 화면에 안 쓴다:

```sh
/mnt/onboard/.apps/pocketjs/pocketjs-kobo --probe-touch
```

네 모서리를 누르고 `logical=` 값을 기대치와 비교한다. 화면에 뜨는 안내대로
**swap을 먼저 확정하고 다시 돌린 뒤 flip을 정한다** — 런타임이 그 순서로 적용한다.

**Glo에서 측정된 값은 이미 나와 있다** (`device/pocketjs.sh`의 기본값):

```sh
POCKETJS_TOUCH_SWAP_XY=1 POCKETJS_TOUCH_FLIP_X=1 POCKETJS_TOUCH_FLIP_Y=0
POCKETJS_TOUCH_X_MAX=1023 POCKETJS_TOUCH_Y_MAX=757
```

뒤의 두 개가 중요하다. **zForce는 `EVIOCGABS`로 `0..1200 × 0..1600`을 선언해 놓고
실제로는 패널 픽셀을 보고한다.** 선언값을 믿으면 터치가 화면 좌상단 절반에만 닿는다.
`--probe-touch`가 찍는 `raw axes` 줄이 override 적용 후 값이므로 거기서 확인한다.

프로브는 원격에서 이렇게 돌린다 (터치스크린을 배타적으로 잡으므로 탭이 nickel로
새지 않는다):

```sh
hosts/kobo/tools/kobo-sh.py <IP> "cd /mnt/onboard/.apps/pocketjs && \
  (setsid ./pocketjs-kobo --probe-touch > /tmp/touch.log 2>&1 &); sleep 2; head -20 /tmp/touch.log"
# 화면을 누른 뒤 — 탭마다 첫 샘플만 뽑는다
hosts/kobo/tools/kobo-sh.py <IP> "awk '/^slot=/{if(!s){print;s=1}} /^release/{s=0}' /tmp/touch.log"
```

### 4-5. 실행

```sh
cd /mnt/onboard/.apps/pocketjs
POCKETJS_TOUCH_SWAP_XY=1 POCKETJS_TOUCH_FLIP_X=1 sh ./pocketjs.sh
```

(위 두 변수는 §4-4에서 **확정한 값**을 넣는다. 예시일 뿐이다.)

`pocketjs.sh`가 nickel을 멈추고, `POCKETJS_GUI_PAUSED=1`을 세우고, FBInk 경로를
찾아 호스트를 띄우고, **어떤 경로로 끝나든 `EXIT` 트랩에서 nickel을 되살린다.**
로그는 `pocketjs.log`. 남는 인자는 호스트로 그대로 넘어가므로
`sh ./pocketjs.sh --motion-waveform A2 --ghost-budget 40`처럼 세션마다 튜닝한다.
번들만 갈아끼우려면 `killall -HUP pocketjs-kobo`.

UI가 안 돌아오면 `sh ./nickel.sh start`, 그래도 안 되면 리부트.

실기에서 확인된 것: `/mnt/onboard`가 **0755로 마운트되므로 실행 비트는 문제가 없었다**
(`sh ./` 우회도, rootfs 이동도 불필요했다). 그리고 앱이 크래시했을 때 `EXIT` 트랩이
**실제로 nickel을 되살렸고**, Wi-Fi를 안 끈 덕분에 telnet 세션도 끊기지 않았다.

### 4-5-1. 세션 중에는 큰 전송을 하지 않는다

**PocketJS가 도는 동안 대용량 FTP 전송을 하다 기기 전체가 얼어붙은 적이 있다.**
강제 전원 차단이 필요했다. 원인은 미확정이지만(PROGRESS.md 참조), 가장 잘 맞는 설명은
nickel을 죽인 채로 Wi-Fi(SDIO)에 부하를 준 것이다 — 그 모듈을 관리하는 게 nickel이다.

지켜야 할 원칙 하나: **큰 전송은 nickel이 살아 있을 때만.**

1. nickel이 떠 있는 동안 파일을 올린다
2. PocketJS를 띄운다
3. 도는 동안에는 몇 바이트짜리 명령만 보낸다 (`uptime`, `pidof`)
4. 캡처는 기기 안에서 압축만 하고, 종료한 뒤에 받는다

이 프로토콜로는 재현되지 않았다.

### 4-5-2. 화면 캡처 — 사진 말고 프레임버퍼

```sh
# 도는 동안: 네트워크를 안 쓴다. 1.5MB -> 18.9KB
hosts/kobo/tools/kobo-sh.py <IP> "dd if=/dev/fb0 bs=1536 count=1024 2>/dev/null | \
  gzip -9 > /mnt/onboard/.kobo/fb.gz"
hosts/kobo/tools/kobo-sh.py <IP> "killall -TERM pocketjs-kobo"   # 트랩이 nickel을 되살린다
curl -u root: -o fb.gz ftp://<IP>/fb.gz               # FTP 루트가 .kobo다
gunzip -c fb.gz > fb.raw
```

758×1024, stride 1536, RGB565로 디코드하면 PNG가 된다. 실기 계조는 **29단계**로,
오프라인 `render_gray`의 92~117단계보다 훨씬 적다 — 패널이 16bpp라 그렇다.

### 4-6. `GATE 6` — 1차 목표 달성 판정

1. **boot/render** — paper-ink가 고스팅 없이 뜨나 (← 최소 성공 기준)
2. **입력** — 터치 좌표 정확도. §4-4의 `--probe-touch`로 확정한 값 적용
3. **refresh** — 갱신 시 파형·잔상 관찰, `--motion-waveform` / `--ghost-budget` 튜닝
4. **idle 고스팅** — 장시간 후 정리 동작

---

## 5. 남은 STOP (전부 실기 필요)

1. ~~**셸 접근**~~ — **해소.** §4-1-a.
2. ~~**프레임버퍼 실측**~~ — **해소:** `mxc_epdc_fb 758×1024 Rgb565, virtual 768×2048,
   stride 1536, rotate=3`. 드라이버가 보고한 `rotate=3`은 예상대로 무시되고 R0이
   가시 래스터에서 유도됐다. 아래는 다른 기기에서 이어받을 때를 위한 원문이다.
   Kobo `mxc_epdc`는 nickel이 마지막에 설정한 방향을 보고하므로 `geometry.rs`는
   `var.rotate`를 참고값으로만 쓰고 실제 가시 래스터에서 방향을 유도한다.
   방향이 틀리면 `--rotation 0|90|180|270`으로 강제할 수 있다.
3. ~~**터치 축**~~ — **해소.** §4-4의 측정값을 쓴다. 아래는 원문이다.
   `--probe-touch`로 확정한다(§4-4). FBInk 기기표는 Glo를
   `touchSwapAxes=true`, `touchMirrorX=true`로 기록하므로 아래가 필요할 것으로
   **예상**된다:
   ```sh
   POCKETJS_TOUCH_SWAP_XY=1 POCKETJS_TOUCH_FLIP_X=1
   ```
   (`POCKETJS_TOUCH_FLIP_Y`도 있다.) **가정 금지 — 눌러 보고 정한다.**
   런타임은 swap 먼저, flip 나중이다. 순서를 뒤집어 생각하면 그럴듯하지만
   틀린 캘리브레이션이 나온다.
4. **FBInk `-s` 좌표계** — FBInk 문서상 **Kobo에서는 `-s` 사각형이 회전·뷰포트 퀵 없이
   ioctl로 그대로 전달된다.** 호스트는 `/dev/fb0`에 쓸 때와 같은 좌표계로 계산하므로
   일치해야 하지만, **갱신 영역이 엉뚱한 곳에 뜨면 여기부터 의심한다.**
5. ~~**파형 튜닝**~~ — **해소, 다만 질문이 틀렸다.** 답은 파형이 아니라 페이싱이었다.
   패널을 실측 속도로 페이싱하면(0024) 전폭 갱신이 573ms인데 `MOTION_WINDOW`는
   120ms라, **큰 영역을 그리는 화면은 모션 경로에 들어갈 수 없고** DU/A2가 아예
   선택되지 않는다(`motion 0, static 17`). 전면 정리도 0회라 `--ghost-budget`과
   `--ghost-area` 둘 다 도달하지 않는다. 잔상 예산은 과잉 제출 시절의 증상
   관리였다. 모션 파형은 작고 국소적인 갱신에서만 의미가 있으니, 그런 앱이
   나오면 그때 `ghost-probe`로 다시 잰다. 아래는 원문이다.
   Glo(Pearl)에서 A2/DU/GC16 모두 사용 가능.
   `--motion-waveform DU|A2`, `--ghost-budget N`을 실제 잔상 기준으로 조정.
   **프레임버퍼로는 판정할 수 없다** — 거기엔 우리가 요청한 것이 들어 있지,
   잉크가 실제로 한 일이 들어 있지 않다. 사람이 패널을 봐야 한다.

   `apps/ghost-probe`가 그 판정을 위한 화면이다. 띠 세 개가 각각 다른 실패를
   유도한다: **SWEEP**(흰 바탕을 가로지르는 검은 막대 — 꼬리가 남는가),
   **FLIP**(제자리 반전 — 회색 잔여물이 쌓이는가), **REFERENCE**(첫 프레임 이후
   다시 그리지 않음 — 여기가 더러워지면 이웃이 번진 것). 화면을 누르면 전부
   멈춘다. **판정은 멈춘 상태에서 한다** — 움직이는 동안은 다음 갱신이 잔상을
   덮는다.

   호스트가 `__inkPolicy`(파형·budget·present Hz)를 공표하고 화면이 스스로
   어떤 설정인지 적는다. 라벨 없는 두 판은 비교가 아니다.

   한 판에 5초다. 재부팅하지 않는다:

   ```sh
   D=/mnt/onboard/.apps/pocketjs
   hosts/kobo/tools/kobo-sh.py <IP> "echo '--js $D/ghost-probe.js --pak $D/ghost-probe.pak \
       --motion-waveform A2 --ghost-budget 240' > $D/RESTART.args \
       && touch $D/RESTART && killall pocketjs-kobo"
   ```

   빌드는 한글 폰트가 필요하다(§4-2 (e2)와 같은 절차). 안 그러면 라벨이 두부가 된다.

---

## 5-1. 펌웨어를 되돌리는 법 (알아낸 것)

업그레이드 스크립트(`/etc/init.d/upgrade-generic.sh`)가 카드의 **raw 오프셋**에 쓴다:

```
u-boot    dd of=/dev/mmcblk0 bs=1K  seek=1
waveform  dd of=/dev/mmcblk0 bs=512 seek=14336
kernel    dd of=/dev/mmcblk0 bs=512 seek=2048
```

즉 기기를 켜지 않고 **카드 리더에서 직접** 커널을 바꿔 넣을 수 있다.
uImage 헤더(magic `27051956`, offset 12에 크기, 32~64에 빌드 이름)로 어느 버전이
올라가 있는지 확인할 수 있다 — 이름의 날짜만 보면 연도를 착각하기 쉬우니
타임스탬프(offset 8)를 디코드할 것.

과거 펌웨어는 pgaskin의 미러에 전부 있다 (Glo = `kobo4`):

```
https://kfw.storage.pgaskin.net/firmwares/kobo4/january2016/kobo-update-3.19.5761.zip
https://kfw.storage.pgaskin.net/MD5SUMS
```

zip 내용을 `.kobo/`에 풀면 다음 부팅에서 설치된다. 단 **`mv`로 디렉터리를 옮기지 말 것** —
macOS의 FAT 드라이버가 `..` 항목을 갱신하지 않아 `fsck`가 깨진다. 복사하고 지운다.

## 6. 지뢰

- **모델 고정.** 호스트는 758×1024(또는 회전형)가 아닌 프레임버퍼를 **추측하지 않고
  거부한다.** Glo HD(Alyssum, 1072×1448)나 다른 Kobo는 그냥 안 된다.
  다른 모델을 붙이려면 `contracts/spec/platforms.ts`에 프로필을 새로 만들고
  `hosts/kobo/src/main.rs`의 `LOGICAL_W/H`, `DENSITY`를 바꿔야 한다.
  그때 **논리 좌표 최대값이 축당 511을 넘는지** 반드시 확인한다(아래).
- **9비트 터치 상한은 좌표 기준이다.** 패킹은 `(id << 18) | (y << 9) | x`,
  즉 최대 좌표 511. 379×512는 최대 좌표가 (378, 511)이라 **정확히 맞는다** —
  512는 extent이지 좌표가 아니다. 넘어가면 `framework/src/touch.ts`의
  wide form(bit31=1, 축당 10비트)이 필요한데 **Rust 쪽엔 아직 프로듀서가 없다.**
  이 불변식은 `tests/platform-contracts.test.ts`의
  `every touch target's logical viewport fits the legacy touch packing`이 잠그고 있다.
- **`hero`는 이 타깃으로 안 빌드된다.** `input.buttons`를 요구하는데 Glo의 물리 키
  (전원·프론트라이트)는 펌웨어가 점유하므로 프로필이 터치만 광고한다. 이건 버그가 아니다.
  버튼 데모가 필요하면 앱을 터치 기반으로 고치거나 별도 매니페스트를 만든다
  (kindle 브랜치의 `apps/hero/pocket.kindle.json`이 그 예).
- **FBInk는 링크 대상이 아니라 런타임 의존성이다.** 호스트는 픽셀만 직접 쓰고
  패널 갱신은 설치된 FBInk CLI로 넘긴다. 기기에 FBInk가 없으면 호스트는 부팅하지 못한다.
- **musl 정적이라 glibc 핀이 필요 없다.** 쿡북 §5-6의 "Kobo glibc 버전 확인 후
  zigbuild `.2.x` 핀" 절차는 **하지 말 것** — 해당 없음.
- **헤드리스에서는 gpui 데스크톱 호스트를 못 쓴다.** 쿡북 §4-4의 "데스크톱 창 렌더 확인"
  대신 §3(e)의 `render_gray`를 쓴다. e-ink 경로를 직접 검증하므로 오히려 낫다.
- **`/mnt/onboard`는 USB를 꽂는 순간 사라진다.** FAT32라 nickel/PC가 배타적으로
  가져간다. 실행 중이던 스크립트의 본문까지 같이 사라지므로, `pocketjs.sh`는
  자기 자신과 `nickel.sh`를 `/tmp`로 복사해 거기서 `exec` 한 뒤에야 nickel을 멈춘다.
  **런처를 고칠 때 이 재실행 단계를 지우지 말 것** — 지우면 USB를 꽂았을 때
  UI가 영영 안 돌아온다.
- **Wi-Fi는 일부러 안 끈다.** KOReader는 nickel 재시작 전에 인터페이스를 내리지만,
  개발 루프가 그 인터페이스 위에서 돈다. 대신 nickel이 자기가 안 올린 Wi-Fi를
  이상하게 볼 수 있다 — 그때는 리부트가 정답이다.
- **`rustfmt`는 버전에 따라 `examples/render_gray.rs`에서 차이를 낸다.**
  원 작업 머신(1.98.1)의 포매팅이므로 **다른 rustc에서 `cargo fmt`로 밀지 말 것.**
  무의미한 diff만 생긴다.
- **PocketJS가 도는 동안 큰 네트워크 전송을 하지 말 것.** 기기 전체가 얼어붙어 강제
  전원 차단이 필요했다. nickel이 Wi-Fi(SDIO) 모듈을 관리하는데 그 nickel을 죽여 놓았기
  때문으로 보인다. §4-5-1의 순서를 지키면 재현되지 않는다.
- **이 커널의 `load average`를 믿지 말 것.** 16ms마다 깨어나는 태스크를 자주
  runnable로 잡아서 0.99가 나오는데, 실제 프로세스 CPU는 14.5%였다. `/proc/<pid>/stat`의
  `utime`을 직접 재라. (해소됨: `--sim-hz` 기본값 30에서 7%.)
- **패널보다 빨리 시뮬레이션하지 말 것.** 패널은 잘해야 30Hz로 present하고 DU 파형은
  한 프레임보다 오래 걸린다. `--sim-hz`로 낮추면 CPU가 정비례로 준다 —
  60Hz 14.5% / 30Hz 7% / 10Hz 2%. 사람의 시간 단위로 바뀌는 화면은 10Hz면 충분하다.
- **nickel이 도는 동안 `event0`를 읽으면 아무것도 안 나온다.** nickel이 `EVIOCGRAB`으로
  독점하기 때문이지 드라이버가 안 쏘는 게 아니다. 입력 노드를 진단할 때는 **nickel을
  먼저 멈춘다** — 안 그러면 없는 하드웨어 문제를 쫓게 된다.
- **`cat`으로 입력 노드를 캡처하지 말 것.** stdout이 4KB 블록 버퍼링이라 수십 바이트짜리
  이벤트는 kill될 때 통째로 사라진다. `dd bs=16`은 블록마다 즉시 쓴다.
- **실패한 ioctl과 할 일이 없던 ioctl은 구별되지 않는다.**
  `MXCFB_WAIT_FOR_UPDATE_COMPLETE`에 마커를 값으로 넘기면 커널이 EINVAL을 주고
  **즉시** 돌아온다 — 이미 끝난 패널을 기다린 것과 똑같이 보인다. 그래서 호스트가
  역압 없이 도는 걸 아무도 몰랐다. **커널은 포인터를 원한다.**
  ioctl 인코딩은 커널마다 다르니 추측하지 말고 `--probe-epdc`로 묻는다:
  ```sh
  hosts/kobo/tools/kobo-sh.py <IP> "/mnt/onboard/.apps/pocketjs/pocketjs-kobo --probe-epdc"
  ```
  이 기기(2.1.5)의 답: `_IOW('F',0x2F,u32)` **포인터**, 1030ms. 나머지는 ENOTTY.
- **`--present-hz`는 패널이 할 수 있는 속도가 아니다.** 루프가 시도해도 되는
  상한일 뿐이다. 실측: 전폭 갱신 **573ms**, 전면 플래시 **1030ms**. 30Hz를 믿고
  제출하면 패널이 소화하는 양의 열 배를 밀어넣게 되고, EPDC가 재초기화되면서
  `MXCFB_SEND_UPDATE`가 EPERM을 돌려준다. 커널 문자열로 확인된 조건은
  `"Display HW not properly initialized. Aborting update."`
- **패널 오류를 치명적으로 다루지 말 것.** 그렇게 두면 호스트가 죽고, 런처가
  약속대로 nickel을 되살리고, nickel이 패널과 Wi-Fi를 가져가 **기기가 죽은 것처럼
  보인다.** "기기가 멈췄다"를 네 번 겪고서야 그게 전부 호스트 크래시였음을 알았다.
  `pocketjs.log.1`을 먼저 읽는다 — 마지막 줄에 이유가 있다.
- **`pidof <applet>`는 busybox 애플릿을 못 찾는다.** `/bin/busybox telnetd`로 띄우면
  프로세스 이름은 `busybox`다. telnet으로 접속한 채로 `pidof telnetd`가 실패하는 걸
  실제로 봤다. 데몬이 사는지 물을 땐 이름 말고 **포트**를 본다:
  ```sh
  awk '$2 ~ /:0017$/ && $4 == "0A"' /proc/net/tcp   # 0017 = 23, 0A = LISTEN
  ```
- **nickel을 없애면 nickel이 읽던 파이프도 없어진다.** rcS가 만드는
  `/tmp/nickel-hardware-status`에 udhcpc와 udev 훅이 이벤트마다 한 줄씩 쓴다.
  독자가 없으면 쓰는 쪽이 커널에서 영구히 막힌다 — 한 번 부팅에 `pipe_wait`로
  주저앉은 셸 3개를 실제로 봤다. 다행히 펌웨어 스크립트가 그 쓰기를 `&`로
  던지고 마지막에 하므로 ifconfig/route/resolv.conf는 이미 끝난 뒤다. 즉
  **네트워크는 멀쩡하고, 새는 건 프로세스뿐**이다. 런처가 독자 역할을 넘겨받되
  nickel을 되살리기 전에 놓는다 (0018). 찾는 법:
  ```sh
  hosts/kobo/tools/kobo-sh.py <IP> "for p in /proc/[0-9]*; do \
      [ \"\$(cat \$p/wchan 2>/dev/null)\" = pipe_wait ] && echo \$p; done"
  ```
- **이 모델의 suspend는 미해결이다.** nickel의 짧은 누르기는 화면만 끄는 상태이고
  (자는 동안 telnet 응답, uptime 연속), `power.sh`의 `echo mem`은 그보다 깊다.
  한 번 시도했고 돌아오지 않았다. rtc에 `wakealarm`이 없어 타이머 복귀도 없다.
- **`e2cp`는 파일 속성을 호스트 것으로 바꾼다.** ext4 이미지에 쓸 때 `-P 0755 -O 0 -G 0`을
  빠뜨리면 `rcS`가 실행 비트를 잃고, init이 못 돌려 **기기가 아예 안 켜진다.** 쓴 뒤에
  `e2ls -l`로 원본과 대조할 것.
- **부팅 애니메이션은 직접 죽여야 한다.** `on-animator.sh`는 250ms마다 전체 화면을
  덮는 무한 루프이고, 평소엔 nickel이 죽인다. 안 죽이면 우리 화면을 초당 4번 지운다.
- **회전은 가시 래스터에서 유도할 수 없다.** 뒤집힌 래스터는 축 교환만 증명한다 —
  R90과 R270 둘 다 들어맞고, 어느 쪽이 똑바른지는 패널의 스캔 방향이다.
  이 기기에서는 **R270**. 다만 런처에 박지 말 것: 프레임버퍼가 세로로 올라오는
  펌웨어에서는 R270을 강제하면 호스트가 기동을 거부한다.
- **동적 의존은 펌웨어에 묶인다.** FBInk(hard-float)가 2.1.5의 soft-float 유저스페이스에서
  실행되지 않아 호스트 전체가 멈췄다 — musl 정적인 호스트 자신은 멀쩡했는데도.
  패널 갱신을 ioctl로 직접 하도록 옮긴 이유다.
- **`ForceWifiOn=true`를 오래된 기기에 넣지 말 것. 이게 기기를 잃는 방법이다.**
  개발 세션에서 Wi-Fi가 끊기는 걸 막으려고 넣었는데, 그 설정이 기기를 밤새 온라인에
  붙들어 두었고 **FW 3.19(2016) → 4.38(2026) 자동 업데이트**를 불렀다. 결과:
  화면이 깨지고 터치가 죽었으며, 기기가 스스로 공장 복원(2.1.5)까지 갔고 사용자
  데이터가 초기화됐다. `EnableDebugServices`만 넣고 Wi-Fi는 필요할 때 수동으로 켠다.
- **활성화가 벽이 될 수 있다.** 공장 초기화된 구형 Kobo는 2012년 API로 계정 활성화를
  시도하는데 그 엔드포인트는 이제 없다. Wi-Fi가 붙어도 설정이 안 끝난다. USB가 죽은
  기기라면 Kobo Desktop 우회도 못 쓴다 — **nickel을 아예 안 띄우는 편이 빠르다.**
- **`nickel.sh`의 kill 목록에 DHCP 클라이언트를 넣지 말 것.** `dhcpcd`는 SIGTERM에
  인터페이스 설정을 해제하고 리스를 반납한다 — nickel을 멈추면서 세션이 타고 있던
  주소를 스스로 버리게 된다. KOReader는 Wi-Fi를 자기가 올리므로 죽여도 되지만 우리는
  아니다. 재시작 시점에만 nickel에게 돌려준다.
- **펌웨어에는 Wi-Fi를 올리는 스크립트가 없다.** rcS는 모듈 위치만 export하고 실제
  기동은 nickel이 한다. nickel 없이 네트워크가 필요하면 `device/wifi.sh up`을 쓴다
  (단계마다 멱등이라 반쯤 무너진 인터페이스의 복구 경로이기도 하다).
- **`ifconfig`는 내려간 인터페이스에도 주소를 계속 보고한다.** "주소가 있으니 올라와
  있다"는 판정은 틀린다 — `UP` 플래그를 같이 봐야 한다. 이걸로 `wifi.sh`가
  `wpa_supplicant`가 멈춘 채 성공을 반환한 적이 있다.
- **`nickel.sh`가 넘기는 환경을 줄이지 말 것.** rcS가 export하는 `WIFI_MODULE_PATH`가
  비면 되살아난 nickel이 무선 모듈을 못 올리고, 개발 세션이 자기 네트워크를 잃는다.
  telnet 셸은 이 값들을 하나도 상속하지 않는다.
- **한글 앱에서 글자 크기를 하나 더 쓰는 비용은 그 크기 × 전체 음절 수다.** 아틀라스는
  사용 중인 모든 슬롯에 charset 전체를 굽는다. 54px 슬롯 하나가 숫자 다섯 개를 그리려고
  2MB를 먹은 적이 있다 — 나머지 pak 전부보다 컸다.
- **드라이버가 선언한 evdev 축 범위를 믿지 말 것.** Glo의 zForce는 `EVIOCGABS`로
  `0..1200 × 0..1600`을 선언하고 **패널 픽셀(`0..1023 × 0..757`)을 보고한다.**
  선언값으로 정규화하면 터치가 화면 좌상단 절반에만 닿는다. 다른 기기를 붙일 때도
  **반드시 `--probe-touch`로 실제 도달 범위를 재고** `POCKETJS_TOUCH_{X,Y}_{MIN,MAX}`를
  설정한다. 가장자리를 훑었을 때 양 끝이 40~50 단위 안쪽인 건 정상이다 —
  손가락 중심이 물리적 가장자리에 닿을 수 없어서다(212dpi에서 40px ≈ 5mm).
- **유휴 첫 프레임만 렌더해 놓고 검증했다고 하지 말 것.** `paper-ink`는 접촉이 생겨야
  잉크 노드를 만들고, 그 노드의 스타일 prop도 그때 처음 검증된다. `borderRadius`
  (없는 `PROP`. 올바른 이름은 `radius`)로 첫 터치에 죽는 번들이 그렇게 게이트를
  통과했었다. `render_gray --touch X,Y`를 반드시 같이 돌린다.
  **`origin/agent/kindle-hero`의 `paper-ink`에도 같은 줄이 있다** — 그쪽도 미검증이다.
- **최신 macOS에 Glo가 안 붙을 수 있다.** 기기는 연결 화면을 띄우는데 호스트는
  USB 장치조차 못 본다. 케이블·허브를 다 배제해도 그렇다면 §4-3의 Windows 경로를 쓴다.
  **Windows에서도 안 붙었다.** 그때는 §4-1-a의 microSD 직접 편집이 답이다.
  어느 쪽이든 최초 1회만 필요하고, 그 뒤는 telnet/FTP로 네트워크에서 다 된다.
- **한글은 빌드가 아니라 화면에서 실패한다.** 폰트에 없는 코드포인트는 에러가 아니라
  gid 0(두부)로 조용히 떨어진다. 한글이 들어간 앱은 `render_gray` 육안 확인이 필수다.
- **`pocket.ts`는 폰트 플래그를 안 넘긴다.** `--font-regular` / `--font-bold` /
  `--extra-chars`는 `tools/build.ts`에만 있고 `pocket.config.ts`에도 항목이 없다.
  그래서 한글 빌드는 plan 생성 → 컴파일러 직접 호출의 2단계다. 1단계 산출물은 두부다.
- **한글 폰트는 저장소에 커밋하지 않는다.** 2MB짜리 TTF 2개는 패치를 5MB 넘게 부풀린다.
  툴체인과 같은 취급 — 받아서 쓴다.
- **`hosts/kobo`는 독립 크레이트다**(자체 `[workspace]` + `Cargo.lock`).
  `engine/Cargo.toml` 워크스페이스에 들어 있지 않으므로 워크스페이스 명령이
  이 크레이트를 건드리지 않는다. 항상 `--manifest-path hosts/kobo/Cargo.toml`을 쓴다.

---

## 7. 이 포크는 독립이다

**upstream에 기여하지 않는다.** `pocket-stack/pocketjs`는 참조 대상이고, 이
작업은 그 위에서 독립적으로 간다. 커밋을 정리해 PR로 만들 계획은 없다 —
지금의 히스토리는 "어떻게 알아냈는가"를 기록하고 있고, 같은 기기를 다시 만질
사람에게는 깔끔하게 묶인 커밋보다 그쪽이 쓸모 있다.

```
origin    github.com/cbcruk/pocketjs        # 이 작업. push는 여기로
upstream  github.com/pocket-stack/pocketjs  # 참조. fetch만
```

브랜치는 `feat/kobo-glo-host`, 베이스는 upstream `2d20dda`.

**upstream을 따라가려면** 필요할 때만 rebase한다. 급할 것 없다 — 이 호스트가
쓰는 것은 `engine/core`, `engine/crates/pocket-mod`, `pocket-ui-surface`,
`pocket-net`이고, upstream의 활동은 주로 다른 타깃과 문서다:

```sh
git fetch upstream main
git log --oneline HEAD..upstream/main          # 무엇이 들어왔는지 먼저 본다
git rebase upstream/main                        # 사본에서 먼저 시험할 것
```

`engine/` 아래가 바뀐 커밋만 실제 위험이다. `pocket-net`의 `HttpTransport`는
이 호스트가 처음 구현했으므로, 그 계약이 바뀌면 `hosts/kobo/src/net.rs`가
가장 먼저 깨진다.

**문서와 도구도 이 트리 안에 있다** (`hosts/kobo/docs`, `hosts/kobo/tools`).
upstream은 `hosts/kobo/` 아래를 건드리지 않으므로 리베이스가 이것들 때문에
깨질 일은 없고, 클론 하나에 코드·문서·도구가 다 들어온다. 동기화할 두 번째
장소가 없다는 것이 요점이다.

발견한 것 중 upstream에도 해당하는 것 두 가지는 기록만 남긴다:

- **`paper-ink`의 `borderRadius`는 `agent/kindle-hero`에도 있는 버그다.** 실제
  PROP 이름은 `radius`이고, 이 줄은 첫 터치에 앱을 죽인다. 그쪽 데모도 실기
  터치를 안 거쳤다는 뜻이다.
- `render_gray --touch`가 그 부류를 잡는다 — 유휴 첫 프레임은 앱의 극히 일부만
  실행하므로, 터치 없는 렌더 게이트는 이 버그를 통과시킨다.
