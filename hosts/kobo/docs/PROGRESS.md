# Kobo Glo × PocketJS — 진행 기록

**다른 머신에서 이어받는다면 [HANDOVER.md](HANDOVER.md)부터 읽는다.**

쿡북: [cookbook.md](cookbook.md)
작업 트리: `~/kobo-pocketjs/pocketjs`, 브랜치 `feat/kobo-glo-host`
원격: `origin` = [cbcruk/pocketjs](https://github.com/cbcruk/pocketjs) (이 작업),
`upstream` = pocket-stack/pocketjs (참조만, 기여 없음)
베이스 커밋: `2d20dda` (pocket-stack/pocketjs, main, 2026-09-03)

이 브랜치는 upstream `2d20dda` 위의 커밋 29개다. 무엇이 왜 바뀌었는지는
커밋 메시지가 갖고 있다:

```sh
git log --oneline --reverse 2d20dda..HEAD
git show <hash>
```

한때 이 자리에 패치 파일 29개와 적용 후 트리 해시 표가 있었다. 브랜치가
`cbcruk/pocketjs`에 올라간 뒤로는 브랜치가 곧 운반 수단이고, 낡을 수 있는
복원 경로를 둘로 유지할 이유가 없어 지웠다.

## 2026-09-08 — P4 통과: `net.http`가 이 기기에서 돈다

`apps/net-probe`가 실기에서 **HTTP와 HTTPS 둘 다 200 / 559바이트 / 0.1초**를
받았다. 게스트에서, 기기에서.

**이 호스트가 `pocket-net`의 `HttpTransport`를 구현한 첫 호스트다.** 코어는 핸들·
한계·틱 경계 배칭을 갖고 DNS·소켓·TLS·스레드는 일부러 갖지 않는데, 그 목록이
정확히 `hosts/kobo/src/net.rs`가 채우는 것이다.

**TLS는 이 바이너리에 정적 링크했다. 기기에서 빌려 쓸 것이 없었기 때문이다:**

| | |
| --- | --- |
| 기기 OpenSSL | 0.9.8l (2009) — TLS 1.2 없음, SNI 없음 |
| busybox wget | `https://` URL을 아예 거부 (`not an http or ftp url`) |
| rustls 비용 | 바이너리 2.03MB → 3.10MB (+1.07MB) |
| 실측 핸드셰이크 | **0.36초** (armv7 1GHz, 2012년 커널) |

코드를 쓰기 전에 이 빌드가 armv7 musl로 되는지부터 확인했다. 안 됐으면 설계가
통째로 달라졌을 것이다.

워커 스레드는 하나다. 패널이 초당 두 번 present하므로, 한 번에 여섯 개를 요청하는
화면도 그것들을 한 번의 리페인트를 위해 원하는 것이고 직렬화로 잃는 게 없다.
완료는 `begin_tick`에서만 게스트로 넘어간다 — 프레임 앞이라, 직전 프레임 중에
도착한 응답이 한 프레임 늦지 않고 이번 프레임에 보인다.

오프라인 렌더도 같은 값을 한다: 모듈을 마운트하지 않는 `render_gray`에서
`unavailable`을 보고한다. 에러 경로가 스스로를 증명한 것이다.

## 2026-09-08 — 시계가 시계가 되었다

앱은 `2012-05-01 UTC`를 표시하고 있었다. RTC 하드웨어는 멀쩡하고 rcS가 부팅 때
이미 읽는데, **값이 한 번도 설정된 적이 없었다.** 게다가 이 펌웨어에는 zoneinfo
데이터베이스가 없어서 시각이 맞아도 시간대가 틀린다.

**RTC가 출처, NTP가 보정이다** (0028). 한 번 맞춰 두면 이후 부팅은 네트워크 없이도
정확하다. 동기화는 백그라운드로 돌리고 — 부팅이 네임서버를 기다릴 이유가 없다 —
답이 오면 호스트에 SIGHUP을 보낸다. `publish_boot_clock`이 리로드에서 다시
실행되므로, 그게 정확히 그 리로드의 용도다.

시간대는 카드의 파일에 든 POSIX TZ 문자열이다. musl이 문자열을 그대로 읽으므로
zoneinfo 파일이 필요 없고, 카드 리더만 있으면 바꿀 수 있다. 파일이 없으면 UTC —
기기가 어디 있는지 추측하지 않는다. 서울은 `KST-9`(POSIX는 동쪽을 음수로 센다).

**그리고 이 부팅이 0026을 증명했다:**

```
wifi: associated after 16s
```

결합에만 16초가 걸렸다. 예전 코드는 DHCP 전체에 15초를 줬으므로 이 부팅은
확실히 실패했을 것이다. 앞선 검증 때 "수정이 없었어도 됐을 것"이라고 적었는데,
이번엔 수정이 실제로 구했다.

## 2026-09-08 — Wi-Fi가 안 붙던 이유, 그리고 잃어버린 로그

부팅 두 번이 시계는 떴는데 네트워크가 없었다. 카드에서 로그를 읽고서야 원인이
나왔다 — **다른 네트워크에 붙은 게 아니었다.** 실패한 부팅과 성공한 부팅이 같은
줄을 갖고 있다:

```
Sending discover... ×3
No lease, forking to background
```

차이는 다음 줄뿐이다. 성공한 쪽은 백그라운드 재시도가 우리의 15초 안에
들어왔고, 실패한 쪽은 안 들어왔다. **같은 동작, 다른 운.**

원인은 `wpa_supplicant`를 띄우자마자 DHCP를 시작한 것이다. 결합(스캔 +
핸드셰이크)에 몇 초가 걸리는데 그 사이에 쏜 discover는 갈 곳이 없다.
`wpa_state=COMPLETED`를 먼저 기다리고, 결합에 걸린 시간을 로그에 남기고,
리스에도 재시도할 여유를 준다 (0026). `wifi.sh status`가 supplicant 상태와
사용 중인 conf 파일을 보고하므로, 다음 실패는 **라디오 탓인지 DHCP 탓인지**
구별된다.

**12분 만에 죽은 세션의 로그는 잃었다.** 재시작 간 덮어쓰기는 이미 고쳤지만
부팅 간에는 여전히 한 세대뿐이었고, 네트워크를 확인하려고 두 번 껐다 켠 것이
정확히 그 로그를 덮어썼다. 이제 네 세대를 유지한다. 크래시 뒤의 재부팅이
사람이 로그를 보기 전 마지막 재부팅인 경우는 드물다.

## 2026-09-09 — 이 기기의 배터리는 부풀었다. 꽂아두고 쓴다

절전이 없다는 것이 확정된 뒤, 한 번 충전으로 얼마나 가는지를 재려고 5분마다
`battery.log`에 게이지를 기록하기 시작했다. 케이블을 뽑는 순간 **기기가 곧바로
네트워크에서 사라졌다** — 게이지가 100%를 가리키던 상태에서.

살펴보니 **셀이 부풀어 있었다.** 2012년 기기이므로 14년 된 셀이고, 이 시점의
수명 종료는 예상 범위다. 부풂은 내부 가스 발생이고 되돌아가지 않는다.

그러면 100%에서 부하를 못 버틴 것이 설명된다. 그리고 **측정하려던 것 자체가
성립하지 않는다**: "한 번 충전으로 얼마나 가나"의 답이 "이 셀로는 못 간다"이므로,
잴 대상이 없다.

### 운영 결정

**배터리를 교체할 때까지 전원을 꽂아둔 채로 쓴다.** 이 기기는 상시 표시 화면이고,
절전 상태가 없으므로(위 항목) 어차피 계속 깨어 있다.

부푼 셀을 다루는 원칙은 그대로 지킨다 — 자리를 비우는 동안 충전하지 않고, 누르거나
구부리지 않고, 트인 곳에 둔다.

### `battery.log`를 읽을 때

기록은 계속 쌓아 둔다(비용이 없다). 다만 **교체 이전의 값은 곡선이 아니다.**
꽂힌 채로 100%에 고정된 숫자이거나, 뽑는 순간 끊긴 기록이다. 방전 곡선은 새 셀을
넣은 뒤의 구간부터 의미가 있다. `status` 열이 `Discharging`으로 바뀌는 지점이
그 시작이다.

Kobo Glo의 배터리는 교체 가능하다 — 뒷판을 열고 커넥터를 뽑는 방식이고 호환
셀(1000mAh 정도)이 유통된다. 교체하면 원래의 질문이 그때 답을 가진다.

## 2026-09-09 — suspend: 두 가지 이유로 불가능하다

두 가지를 각각 확인했고, 둘 중 하나만으로도 suspend가 성립하지 않는다.

### 1. 커널이 드라이버 서스펜드에서 멈춘다

커널 자체의 `pm_test`로 단계를 나눠 재현했다. `pm_test`는 같은 경로를 걷고 5초 뒤
스스로 깨어나므로, 전원을 끊지 않고 어디서 깨지는지 볼 수 있다.

| 단계 | 결과 |
| --- | --- |
| `freezer` | 통과 — 0.67초에 정지, 5초 뒤 정상 복귀 |
| `devices` | **정지** — 로그가 `entering mem`에서 끊기고 자동 복귀에도 도달 못 함 |

두 번째 시도는 호스트를 SIGSTOP으로 재우고, Wi-Fi를 내리고, `state-extended=1`까지
세웠다. `power.sh`가 쓰는 순서에 안전망만 얹은 것이고, 그래도 같았다.
**우리 코드 탓이 아니다.**

### 2. 깨울 방법이 없다 — 전원 버튼은 소프트웨어에 보이지 않는다

`/proc/interrupts`가 확정한다:

```
 60:          0    MXC_TZIC  mxckpd      ← event0의 컨트롤러. 부팅 이래 0
234:          1           -  power_key   ← 별도 IRQ. 부팅 때의 1회뿐
```

동작 중에 버튼을 눌러도 **양쪽 다 증가하지 않는다.** PMIC가 하드웨어에서 처리하고,
커널은 서스펜드용 wakeup 소스로만 알고 있다. `/proc/bus/input/devices`에는
`mxckpd`와 `zForce` 둘뿐이며 `power_key` 뒤에는 입력 장치가 없다.

`discover_power_device()`가 event0을 고르는 이유는 `mxckpd`가 KEY_POWER를
**선언**하기 때문이다. 선언과 실제가 다른 것은 zForce 축 범위와 같은 함정이다
(§6 지뢰).

**따라서 0009의 전원 키 처리는 이 기기에서 한 번도 동작한 적이 없다.** 짧게
누르기도, 길게 눌러 Kobo UI로 돌아가는 탈출구도 마찬가지다. 사용자가 관찰한
"길게 누르면 꺼진다"는 하드웨어 전원 차단이다.

### CPU 절전도 여지가 없었다

cpufreq가 `userspace` 거버너 800MHz를 보고해서 고정된 줄 알았는데, 그 밑에서
**DVFS가 이미 스케일링**하고 있다. 초당 10회 샘플링: 호스트가 도는 중 평균
176MHz, 정지 시 160MHz(바닥). 아낄 것이 없었다.

### 남은 것

`--power-press`가 `none|doze|suspend`를 받고 **기본은 `none`이다.** doze는 라디오를
내리므로 깨어날 유일한 수단이 전원 키인데 그 키가 없다 — `--doze-max-secs`(기본
600) 데드라인이 그래서 있다. 키가 없는 기기에서 doze는 "N분간 접근 불가"일 뿐이니
실사용 가치는 없고, 키가 있는 다른 Kobo를 위해 남겨 둔다.

**결론: 이 기기에 사용자가 부를 수 있는 절전 상태는 없다.**

## 2026-09-09 — 12분 문제: 관찰을 중단한다

밤샘 실험은 돌지 않았다. 사용자가 기기를 껐다 — 그리고 그게 이 질문의 답을
바꾼다. **기기는 밤에 꺼진다. 실사용에서 연속 가동이 하루를 넘길 일이 없다.**
20시간 뒤에 나타날지 모를 고장은 그 사용 패턴에서 도달하지 않는 지점이다.

남은 증거를 정리하면 쫓을 것이 있는지도 불확실하다:

- 증명된 메커니즘(대기 ioctl 실패 → 패널 과부하 → EPDC의 EPERM)은 0024가 고쳤다.
- 그 뒤 죽은 것은 프로브를 12분 돌렸을 때 한 번뿐이고, **그 세션 로그를 잃어서**
  같은 원인인지 다른 원인인지 알 수 없다.
- **시계로는 한 번도 죽지 않았다.** 16분·15분 세션과 여러 번의 재시작 전부 무사.
- fd가 8개에서 그대로다. 누수라면 가장 먼저 보일 곳이다.

메모리 비교는 무효다: 기준점(RSS 7384kB)을 잡은 뒤 net.http가 들어가 바이너리에
TLS가 붙었다(RSS 8000kB, 스레드 1→2). 둘 다 예상된 증가이고, 누수의 증거가 아니다.

**그래서 추가 실험을 만들지 않는다.** 쓰는 대로 쓰고, 죽으면 그때 로그가 말한다 —
로그 4세대, 20초 flush, 카드에서 읽는 도구까지 전부 갖춰져 있다. 어제까지 없어서
원인을 못 찾았던 것이 지금은 있다.

### RTC는 전원을 버티지 못한다

어제 "한 번 맞춰 두면 이후 부팅은 네트워크 없이도 정확하다"고 적었는데 **틀렸다.**
오늘 콜드 부팅이 다시 2012-05-01에서 시작했고 NTP가 453,072,915초(14.4년)를
보정했다. 진짜 전원 차단이었으므로 RTC가 값을 잃은 것은 확정이다.

남은 가능성은 하나뿐이다 — rcS가 `hwclock -s -u`를 백그라운드 서브셸에서 돌리므로
런처가 그보다 먼저 시작했을 수 있다. `clock.sh sync`가 이제 **보정 전 RTC 값을
로그에 남기므로** 다음 콜드 부팅이 공짜로 가른다. 2012가 찍히면 백업 셀이 죽은
것이고, 올바른 값이 찍히면 경쟁이다.

실용적으로는 어느 쪽이든 같다: 네트워크가 없는 콜드 부팅은 2012년을 보여준다.

### 아직 모르는 것 — 그리고 재현에 프로브가 필요 없는 이유

프로브(고부하)를 돌린 세션이 12분 만에 죽은 이유. 로그를 잃었으므로 추측하지
않는다. 다만 **작업량에 비례하는 고장으로 보인다**:

| | 패널 작업량 | 사망까지 |
| --- | --- | --- |
| 프로브 (페이싱 전) | 초당 13회 | 2~5분 |
| 프로브 (페이싱 후) | 초당 1.7회 | 12분 |
| 시계 | 분당 1회 | 미확인 |

작업량을 약 5배 줄이니 수명이 약 4배 늘었다. 비례한다면 시계는 프로브보다 100배
적게 그리므로 **20시간쯤**이 된다. 그게 정확히 이 기기의 용도이므로, 이 질문은
고부하 테스트가 아니라 **장시간 방치**로 답해야 한다. e-ink에서 프로브 수준의
부하는 실제 앱이 만들 수 없고, 페이싱이 들어간 뒤로는 만들 수도 없다.

측정 기준점 (시계, uptime 429s, 패치 0027 빌드):

```
VmRSS 7384 kB   VmSize 8512 kB   Threads 1   fds 8
MemFree 198488 kB   cpu 2399 ticks (~5.8% of one core)
```

RSS나 fd가 자라면 누수고, 그대로인데 죽으면 EPDC 쪽이다. 죽으면
`pocketjs.log`(이제 4세대)의 마지막 줄이 말해 준다.

## 2026-09-07 — STOP 5: 답이 파형이 아니었다

넉 대의 "멈춤"을 쫓다가 호스트의 근본 결함을 찾았다. **기기는 멈춘 적이 없다.
호스트가 매번 죽었고, 런처가 약속대로 nickel을 되살렸다.** nickel이 패널과 Wi-Fi
모듈을 가져가니 화면이 멎고 네트워크가 끊겼다 — 그게 "멈춤"의 정체였다.

### 사슬

1. **대기 ioctl이 한 번도 성공한 적이 없다.** `MXCFB_WAIT_FOR_UPDATE_COMPLETE`에
   마커를 **값으로** 넘기고 있었다. 커널은 포인터를 원하고, EINVAL을 돌려주고,
   즉시 반환한다 — **아무것도 기다릴 게 없는 패널과 구별되지 않는다.** 그래서
   실패가 6개월간 보이지 않았다.
2. **역압이 없으니 과부하.** 이 패널은 전폭 갱신에 573ms, 전면 플래시에 1030ms가
   걸린다(실측). 호스트는 초당 13회를 밀어넣고 있었다 — 패널 용량의 열 배.
3. **EPDC가 재초기화되고 EPERM.** 커널 문자열이 정확히 말해 준다:
   `"Display HW not properly initialized. Aborting update."`
4. **EPERM이 치명적이었다.** 호스트 종료 → 런처가 nickel 복원 → 기기 사망.

### 고친 것 (0024)

- **대기에 포인터를 넘긴다.** 추측이 아니라 `--probe-epdc`로 확정했다: 실제
  진행 중인 갱신에 대해 알려진 인코딩 네 개를 시험하고 errno와 소요 시간을 찍는다.
  이 커널에서 셋은 ENOTTY, 넷째가 1030ms 걸리며 성공한다.
- **거부된 갱신은 치명적이지 않다.** 다음 갱신에서 컨트롤러가 살아나므로 전면
  다시 그리기로 표시하고 계속 간다. 연속 30회여야 포기한다.
- **파형별 정착 시간을 첫 사용 때 실측하고, 그만큼 패널을 건드리지 않는다.**
  블로킹은 정직하지만 1초는 논리 틱 30개다 — 잉크 값을 게스트 시계가 치를
  이유가 없다.

### 페이싱 전후

| | 전 | 후 |
| --- | --- | --- |
| 논리 틱 / 10초 | 300 (버스트로 133ms씩 정지) | 300 (정지 없음) |
| 패널 갱신 / 10초 | 130 (요청) | 17 (패널이 실제로 소화) |
| 전면 정리 / 10초 | 7~8 | **0** |
| 거부된 대기 | 100% | 0 |
| 프로브 지속 | 2~5분 뒤 사망 | 10분+ 무사고 |

### 그래서 STOP 5의 답

**파형 선택 문제가 아니었다.** 정직하게 페이싱하면 전폭 갱신 하나가 573ms인데
`MOTION_WINDOW`는 120ms다. 즉 **큰 영역을 그리는 화면에서는 모션 경로에 들어갈
수가 없고**, DU/A2는 아예 선택되지 않는다(`motion 0, static 17`). 모션 파형은
작고 국소적인 갱신에서만 의미가 있다.

그리고 페이싱이 붙자 **`--ghost-budget`도 `--ghost-area`도 도달하지 않는다** —
전면 정리가 0회다. 잔상 예산은 과잉 제출을 하던 시절의 증상 관리였다.

앞선 세 판의 육안 판정(DU/80, A2/240, A2/무제한 — 전부 "깨끗함")은 **모션 파형을
한 번도 쓰지 않은 판이었다.** AUTO가 깨끗하다는 사실만 말해 준다. 그것도 결과이긴
하다: 이 패널에서 사람 속도로 바뀌는 화면은 AUTO로 충분하다.

## 2026-09-07 — STOP 5 파형 측정 (방법론 오류, 위 항목이 정정한다)

`apps/ghost-probe`로 실기에서 눈으로 판정했다. 세 개의 띠가 각각 다른 실패를
유도한다: SWEEP(흰 바탕 위를 지나가는 검은 막대 — 꼬리), FLIP(제자리 반전 — 회색
잔여물), REFERENCE(첫 프레임 이후 안 그림 — 이웃이 번지는지). 화면을 누르면 전부
멈춘다. 움직이는 동안은 다음 갱신이 잔상을 덮으므로, **판정은 멈춘 상태에서** 한다.

호스트가 `__inkPolicy`(파형·budget·present Hz)를 `__simHz`와 같은 슬롯에 공표하고
화면이 스스로 어떤 설정인지 적는다. 라벨 없는 두 판은 비교가 아니다.

| 파형 / budget | SWEEP 꼬리 | FLIP 회색 | REFERENCE | 전면 번쩍임 |
| --- | --- | --- | --- | --- |
| DU / 80 (기존 기본값) | 없음 | 없음 | 깨끗 | — |
| A2 / 240 | 없음 | 없음 | 깨끗 | 거의 없음 |

DU/80은 안전한 쪽이었다. A2는 DU보다 빠르고, budget 240은 전면 갱신 간격을 3배로
늘리는데도 잔상이 안 나온다. 절벽이 어디인지 보려고 budget을 사실상 끄고
(100000) 재확인 중.

**측정 비용이 이제 명령 한 줄이다.** 런처에 `RESTART`/`RESTART.args`가 생겨서
파형을 바꾸는 데 재부팅이 필요 없다 — 5초.

## 2026-09-07 — 카드 왕복이 끝났다

nickel 없는 부팅에서 원격 셸이 열렸다. 막고 있던 건 셋 다 nickel이 대신 해 주던
일이었다: Wi-Fi 모듈 경로(rcS의 `WIFI_MODULE_PATH`), `telnetd` 심볼릭 링크(없음 —
busybox 애플릿으로), devpts 마운트(2.1.5 rcS는 안 한다). 자세한 건 HANDOVER §4-1-c.

그걸 세 번이나 눈감고 고친 이유가 따로 있었다. 런처가 `[ -t 1 ]`로 로그 여부를
정했는데 **init이 rcS에 주는 콘솔도 터미널이라**, 아무도 읽지 않는 부팅에서 정확히
로그를 안 남겼다. pty만 사람이 있다는 뜻으로 바꿨다 (0017).

셸이 열리자마자 부산물이 하나 나왔다: `pipe_wait`에 주저앉은 셸 3개. nickel이
읽던 `/tmp/nickel-hardware-status`에 독자가 없어서다. 네트워크 이벤트마다 하나씩
샌다 (0018).

부산물이 하나 더 있었다. `pidof telnetd`는 busybox 애플릿을 못 본다 — 프로세스
이름이 `busybox`다. 그래서 **telnet으로 접속한 채로 "telnetd: not running"을
읽었다.** 런처의 중복 실행 방지 분기도 같은 이유로 죽은 코드였다. `/proc/net/tcp`에
포트를 묻는 걸로 바꿨다 (0019). `diagnose.sh`에 남아 있던 FBInk 탐색도 걷어냈다 —
패널이 ioctl로 간 뒤로 의미가 없었다.

이제 배포는 `hosts/kobo/tools/kobo-push.py`가 네트워크로 한다. 양쪽 md5를 맞춰본 뒤에야
제자리로 옮기는데, 첫 실행에서 **자기 자신의 버그(404)를 잡아냈다** — 그 검사가
있는 이유다.

측정: 재부팅 후 호스트 CPU **10초에 57틱 = 코어의 5.7%**, 메모리 55MB/254MB,
`pipe_wait` 누수 0. `load average`는 여전히 1.69를 보고한다 — 예전과 같은 거짓말이다.

## 상태: `GATE 6` 통과 — 실기에서 돈다

**2026-09-05, Kobo Glo(N613, FW 3.19.5761)에서 `paper-ink`가 렌더되고 터치가 정확히 동작했다.**

| 페이즈 | 내용 | 상태 |
| --- | --- | --- |
| P1 | 환경 셋업 + 스톡 데모 빌드 | **완료** |
| P2 | `hosts/kobo/` 작성 | **완료** — 빌드·테스트·크로스컴파일 통과 |
| P2.5 | 디바이스 브링업 (nickel 래퍼, 터치 프로브) | **완료** — 하드웨어 없이 스모크 테스트 통과 |
| P3 | 실기 배포·검증 | **완료** — `GATE 6` 4단계 전부 통과 |
| P4 | 커스텀 앱 + 한글 폰트 | **완료** — 한글 검증, `net.http` 구현·실기 통과 |

### `GATE 6` 결과

| 단계 | 결과 |
| --- | --- |
| 1. boot/render | 758×1024 패널에 `paper-ink` 렌더 |
| 2. 입력 | 점이 손가락을 따라오고 **누른 자리에 정확히** 찍힘. `SAMPLES` 증가 확인 |
| 3. refresh | DU 모션 파형, 30Hz present. 이상 없음 |
| 4. idle 고스팅 | 잔상 있으나 **e-ink 기준 거슬리지 않는 수준** (사용자 판정) |

프레임버퍼를 그대로 떠서 확인했다 (사진 아님):

```sh
dd if=/dev/fb0 of=/mnt/onboard/.kobo/fb.raw bs=1536 count=1024   # 기기
curl -u root: -o fb.raw ftp://<IP>/fb.raw                        # 빌드 머신
# 758×1024, stride 1536, RGB565 → PNG
```

실기 프레임버퍼의 계조는 **29단계**다. 오프라인 `render_gray`는 92~117단계를 내지만
패널이 16bpp라 여기가 상한이다. 결함이 아니라 하드웨어 한계다.

## 첫 자작 앱: `ink-clock`

이식이 아니라 **이 기기를 위해 쓴 첫 앱**이다. 실기에서 확인됐다 — 시각 정확, 분 전환
정확, 탭 토글 동작, 7분 연속 안정.

e-ink가 형태를 정한다. 패널은 정지 화면을 공짜로 들고 있고 바뀔 때만 비용을 내므로,
이 앱은 **1분에 한 번 바뀌고 절대 애니메이션하지 않는다.** 탭하면 시계와 실행 정보가
바뀌는 것이 상호작용의 전부다.

### 시간을 어떻게 넣었나

PocketJS 시간은 **의도적으로 프레임 카운터**다(docs/DETERMINISM.md). 어떤 호스트도
게스트에 살아 있는 시계를 주지 않는다. 그런데 달력은 어딘가에서 출발해야 한다.

`publish_boot_clock`이 로컬 시각을 **딱 한 번**, 런타임이 이미 `__simHz`와 `__pak`에
쓰는 **같은 계약 슬롯**으로, 번들 eval 전에 써넣는다. 그 뒤는 전부 `virtualNow()`다.
같은 부팅값으로 재생하면 같은 궤적이 나오므로 fold의 순수성이 유지된다.
SIGHUP 리로드가 이걸 다시 공표하는데, 장시간 세션의 드리프트를 맞추는 수단이기도 하다.

`render_gray`는 고정 부팅 시각을 공표한다 — 달력 앱의 스냅샷도 바이트 단위로 재현된다.

### 폰트 크기는 타이포그래피가 아니라 예산 결정이다

시계는 원래 54px로 만들었다가 36px로 내렸다. **아틀라스는 사용 중인 모든 슬롯에
charset 전체를 굽는다.** charset에 한글이 들어간 순간, 54px 슬롯 하나가 **숫자 5개를
그리려고 2MB**를 먹는다 — 나머지 pak 전부를 합친 것보다 크다.

| 시계 크기 | 해당 슬롯 | pak 전체 |
| --- | --- | --- |
| 54px (`text-5xl`) | 2.00 MB | 2.93 MB |
| 36px (`text-4xl`) | 0.87 MB | 1.79 MB |

36px는 실물 72px다. 이 패널에서 충분히 크다. **한글 앱에서 글자 크기를 하나 더 쓰는
비용은 그 크기 × 전체 음절 수다.**

## nickel을 대체했다 (사다리 5번) — 그리고 그 대가

**2026-09-06, 기기가 nickel 없이 부팅해 PocketJS를 띄웠다.** 계정도, Wi-Fi도,
활성화도 없이. 화면 정상, 터치 정상.

### 어떻게

`/etc/init.d/rcS`의 nickel 두 줄을 조건부로 바꿨다. rcS는 그 지점 이전에
`PLATFORM`·`INTERFACE`·`WIFI_MODULE`·`NICKEL_HOME`을 전부 export하므로 **런처가
nickel이 받았을 환경을 그대로 물려받는다** — telnet 셸에서 겪었던 문제가 여기선 없다.

```sh
POCKETJS_DIR=/mnt/onboard/.apps/pocketjs
if [ -f $POCKETJS_DIR/pocketjs.sh ] && [ ! -e $POCKETJS_DIR/DISABLED ]; then
	killall on-animator.sh pickel 2>/dev/null
	sh $POCKETJS_DIR/pocketjs.sh &
else
	/usr/local/Kobo/hindenburg &
	/usr/local/Kobo/nickel -qws &
fi
```

**모든 실패 경로가 nickel로 떨어진다** — 런처 없음, 바이너리 없음, `DISABLED` 파일.
부팅 애니메이션은 `on-animator.sh`가 250ms마다 전체 화면을 덮는 무한 루프라
직접 죽여야 한다(평소엔 nickel이 죽인다).

ext4는 macOS가 마운트를 못 하므로 `e2tools`로 이미지 위에서 편집한다. **`e2cp`는
파일 속성을 macOS 것으로 바꾼다** — `-P 0755 -O 0 -G 0`을 반드시 붙여야 한다.
실행 비트가 없으면 init이 rcS를 못 돌려 기기가 아예 안 켜진다.

### 왜 FBInk를 버려야 했나

첫 시도는 이 로그로 끝났다:

```
Error: starting FBInk /mnt/onboard/.apps/pocketjs/bin/fbink
Caused by: No such file or directory (os error 2)
```

파일은 있는데 "없다"는 건 **ELF 인터프리터가 없다**는 뜻이다:

```
공장 펌웨어 2.1.5 :  ld-linux.so.3         glibc 2.11.1   ← soft-float
KOReader의 fbink  :  ld-linux-armhf.so.3                  ← hard-float
```

호스트는 **musl 정적이라 멀쩡히 떴다.** 유일한 동적 의존이 정확히 그 지점에서 걸렸다.
그래서 패널 갱신을 ioctl로 직접 한다 — 기기가 한 종뿐이고 인터페이스는 고정이다
(Mark 4 i.MX50, NTX 2.6.35, `mxcfb_update_data_v1_ntx`). 이제 **바이너리의 동적 의존이
0이고 펌웨어 버전과 무관하다.**

손으로 옮긴 숫자는 두 가지가 지킨다: 테스트가 `_IOW` 인코딩을 다시 계산해 대조하고,
`const` 단언이 구조체를 68바이트로 못박는다. **후자가 실제로 버그를 잡았다** —
`virt_addr`를 포인터로 뒀더니 빌드 머신(64비트)에서 8바이트가 되어 있었다.

### 회전은 유도할 수 없다

nickel이 없으면 프레임버퍼가 가로(1024×758)로 올라온다. 호스트는 설계대로 가시
래스터에서 방향을 유도했지만 **둘 중 틀린 쪽을 골랐다.** 뒤집힌 래스터는 축이
바뀌었다는 것만 증명하고 방향은 말하지 않는다 — R90과 R270 둘 다 들어맞는다.
어느 쪽이 똑바른지는 패널의 스캔 방향이고, 실기에서 **R270**로 측정됐다.

런처에 `--rotation 270`을 박으면 안 된다. 3.19에서는 프레임버퍼가 세로로 올라와서
R270을 강제하면 호스트가 기동을 거부한다.

### 아직 안 된 것

원격 접속(`REMOTE` 파일로 opt-in, Wi-Fi + telnetd)은 **첫 시도에서 네트워크에
닿지 못했다.** 이유를 담은 로그는 카드에 있다. 동작한다고 적지 않는다.

## 쿡북 전제가 바뀐 지점

쿡북은 2026-07 시점 정보로 쓰였고, 그 사이 업스트림이 120커밋 진행했다.

| 쿡북 전제 | 실제 (2026-09-04 확인) |
| --- | --- |
| PR #172는 draft, 리팩터 대기 | **랜딩됨** (`557c939`). 리팩터도 완료 (`34b24bc refactor(hosts): align ports with runtime ownership`). fork 불필요 |
| Kobo가 최초의 raw-framebuffer e-ink 호스트 | **`origin/agent/kindle-hero` 브랜치 존재** — Paperwhite 5용 완성 호스트 (17.9k 라인) |
| §5-1 STOP: FBInk FFI(bindgen) vs raw ioctl | 둘 다 아님. **하이브리드**: 픽셀은 raw `FBIOGET_{F,V}SCREENINFO` + `memmap2`, 패널 갱신은 **FBInk를 CLI로 shell-out**. FBInk를 링크하지 않으므로 bindgen도 라이선스 문제도 없음 |
| §5-6 STOP: Kobo glibc 버전 확인 후 zigbuild 핀 | **불필요**. musl 정적 링크(`armv7-unknown-linux-musleabihf`)라 glibc 의존이 0 |
| §5-2 `framebuffer.rs` gray 변환은 "실질 재작성" | **이미 존재**. kindle 브랜치의 `render_scaled_gray8` / `_regions` / `_incremental` (Rec.709 정수 루마)를 main의 새 `RenderResources` 시그니처로 리베이스만 하면 됨 |
| §5-4 evdev 이벤트 소스 교체 작업 | kindle `input.rs`가 **MT/non-MT 양쪽 처리**. Glo는 FBInk 기준 `isKoboNonMT`라 기존 non-MT 분기가 그대로 커버 |
| §8 net 서피스는 신규 작업 | **이미 존재** — `docs/NET.md`, `engine/crates/pocket-net`, capability `net.http` |
| §7 터치 9비트 때문에 축당 511px 초과 불가 | 제약은 **좌표**에 걸림(`≤ 511`), 폭이 아님. 게다가 `framework/src/touch.ts`에 **wide form**(bit31=1, 축당 10비트)이 이미 있음 |

즉 쿡북이 "실질"이라고 본 작업 대부분은 이미 풀려 있었고, 실제로 한 일은
**Kindle e-ink 호스트를 Kobo 기기 계약으로 이식**하는 것이다.

## 확정된 결정

### 베이스: Kindle 로직 + main 리베이스
`hosts/kindle`의 `framebuffer/refresh/input/damage/geometry`를 현재 main 위로 이식.
공유 크레이트(`pocket-ui-surface`, `pocket-mod`)가 안정 축이라는 쿡북의 예측이 맞았다 —
120커밋 간극에도 **API 드리프트 오류 0개**로 컴파일됐다.

### 뷰포트: `kobo-glo` = 379×512 @2x
```
패널   758 × 1024   (Glo 세로 네이티브)
렌더   758 × 1024   density 2 — 758 = 2×379, 1024 = 2×512
논리   379 ×  512   최대 좌표 (378, 511)
```
레터박스 없음, 분수 스케일 없음, 풀패널.

9비트 터치 패킹 `(id << 18) | (y << 9) | x`의 상한은 **좌표** 511이지 폭이 아니다.
512는 extent이고 그 안의 최대 좌표는 511이므로 legacy 패킹에 정확히 들어맞는다.
따라서 wide form도 불필요. 이 불변식은 테스트로 잠갔다
(`every touch target's logical viewport fits the legacy touch packing`).

### 첫 검증 앱: `paper-ink`
`hero`는 `input.buttons`를 요구하는데 Glo의 물리 키(전원·프론트라이트)는 펌웨어가
점유하므로 프로필이 터치만 광고한다 → hero는 이 타깃으로 빌드되지 않는다.
kindle 브랜치가 같은 이유로 만든 터치 전용 e-ink 데모 `paper-ink`를 379×512로 이식했다.

## 통과한 게이트

| 게이트 | 결과 |
| --- | --- |
| `GATE 3` 환경 빌드 | `bun install` OK, `cargo check --workspace` (engine) 클린 |
| `GATE 4` 스톡 데모 빌드 | `hero` → pocketbook OK; `paper-ink` → **kobo-glo** OK (pak 474KB + JS 127KB) |
| `GATE 5-2` gray 변환 검증 | 758×1024 Gray8 렌더, 92 그레이 레벨, PNG 육안 확인 완료 |
| `GATE 5-6` ARM ELF | `ELF 32-bit LSB, ARM, EABI5, hard-float ABI, statically linked, stripped`, 2.0MB |
| 회귀 | 순정 main 728 pass/40 fail → 브랜치 730 pass/39 fail (+1 신규 테스트), **회귀 0** |
| 호스트 단위 테스트 | 30/30 통과 (geometry, non-MT 싱글터치, 캘리브레이션 순서, 축 override) |
| 코어 단위 테스트 | 133/133 통과 (gray8 4건 포함) |
| 디바이스 스크립트 | `hosts/kobo/tests/device-scripts.sh` 19/19 통과 (스텁 펌웨어 + 스테이지 루트) |
| 한글 렌더 | `hangul-probe` → Nanum Gothic 169 glyph, 758×1024 Gray8 육안 확인 |
| 2차 머신 재현 | macOS 26.5.1 / arm64에서 위 게이트 전부 재현 (rustc 1.95.0, zig 0.15.2) |
| **`GATE 6` 실기** | **Kobo Glo N613 / FW 3.19.5761에서 렌더·터치·refresh·고스팅 전부 통과** |

## 실기가 알려준 것 — 오프라인 게이트가 못 잡은 것들

이게 `GATE 6`의 진짜 수확이다. 셋 다 로컬에서는 전부 초록불이었다.

### 1. zForce가 선언한 축 범위는 거짓이다

드라이버가 `EVIOCGABS`로 **`x=0..1200, y=0..1600`을 선언해 놓고 실제로는 패널 픽셀을
보고한다.** 선언값으로 정규화하면 터치가 화면 좌상단 절반에만 닿는다. 가장자리를
훑어 측정한 실제 도달 범위:

```
raw x  37 .. 981     raw y  54 .. 719
```

양 끝 모두 약 40~50 단위 안쪽인데, 212dpi에서 40px ≈ 5mm로 손가락 중심이 물리적
가장자리에 닿을 수 없는 거리와 일치한다. 즉 **실제 좌표계는 `0..1023 × 0..757`,
패널 픽셀 그 자체**다.

`POCKETJS_TOUCH_{X,Y}_{MIN,MAX}`를 추가해 선언값을 대체한다. 확정된 캘리브레이션:

```sh
POCKETJS_TOUCH_SWAP_XY=1 POCKETJS_TOUCH_FLIP_X=1 POCKETJS_TOUCH_FLIP_Y=0
POCKETJS_TOUCH_X_MAX=1023 POCKETJS_TOUCH_Y_MAX=757
```

`device/pocketjs.sh`가 이 값을 기본값으로 들고 있고, 환경변수로 덮어쓸 수 있다.
`fbink -e`가 `touchSwapAxes=1; touchMirrorX=1; touchMirrorY=0`을 보고해
**독립적으로 같은 결론**을 준다.

### 2. `paper-ink`는 첫 터치에 죽었다

잉크 점을 `borderRadius`로 스타일링하는데 그런 `PROP`이 없다 — 런타임 이름은
`radius`다. 초기 프레임에는 잉크 점이 없으니 화면은 멀쩡히 뜨고 **손가락이 닿는
순간** 던진다. `origin/agent/kindle-hero`에도 같은 줄이 있다. 즉 **그쪽 데모도 실기
터치를 한 번도 안 거쳤다.**

### 3. `render_gray`가 그걸 통과시켰다

첫 프레임만 그렸기 때문이다. 유휴 프레임은 앱의 극히 일부만 실행한다.
`--touch X,Y`로 합성 접촉을 끌도록 고쳤고, 되돌려 확인하니 **기기에서 난 것과
동일한 에러를 오프라인에서 재현**한다.

```sh
cargo run --manifest-path hosts/kobo/Cargo.toml --example render_gray -- \
  dist/paper-ink-main.js dist/paper-ink-main.pak /tmp/paper-ink --touch 180,300
```

한글 앱이든 뭐든, **터치로만 생기는 노드를 가진 앱은 이 옵션 없이 검증했다고 볼 수 없다.**

## 장시간 운영에서 드러난 것

### 기기가 한 번 완전히 멈췄다 — 원인 미확정

`ink-clock`(당시 pak 2.9MB)을 돌리는 중, **`/dev/fb0` 1.5MB를 FTP로 끌어오다** 기기
전체가 얼었다. 강제 전원 차단이 필요했다. `dd` 자체는 성공했고(직후 `date`가 응답)
전송이 372KB에서 멈추면서 죽었다.

수치로 배제된 것:

```
MemTotal 254 MB   MemFree 104 MB (nickel 실행 중) / 120 MB (nickel 정지)
battery  capacity=100, ntx_get_battery_vol: full !! 4200000
```

**메모리도 배터리도 아니다.** pak 크기가 원인이라는 근거도 없다.

**이후에 밝혀진 것:** 네트워크가 사라진 것은 프리즈와 별개로 설명된다 — 우리가
`dhcpcd`를 죽이고 있었다(아래). 화면이 정지한 시계였다는 점까지 감안하면, "멈춤"의
일부는 **외관상 그렇게 보인 것**일 수 있다. 그래도 강제 전원 차단이 필요했던 건 사실이므로
원인 미확정으로 남긴다.

가장 잘 맞는 가설은 **nickel을 죽인 채로 한 대용량 네트워크 전송**이다. Kobo에서
Wi-Fi(SDIO) 모듈의 전원·상태를 관리하는 건 nickel의 스크립트이고, 2.6.35 커널에서
SDIO 버스가 물리면 커널째 멈춘다 — 유저 프로세스가 기기 전체를 얼릴 수 있는 몇 안 되는
경로다. 시간 순서도 맞는다: 성공한 캡처 2번은 **nickel이 방금 재시작된 직후**였고,
프리즈 직전에는 이미 Wi-Fi가 먼저 사라졌었다.

**가설이지 증명이 아니다.** 다만 이 경로를 피하면 재현되지 않았다.

### nickel을 잘못 되살리고 있었다

`nickel.sh`가 `LD_LIBRARY_PATH` 하나만 export하고 있었다. `/etc/init.d/rcS`는
**`PLATFORM` `PRODUCT` `INTERFACE` `WIFI_MODULE` `WIFI_MODULE_PATH` `NICKEL_HOME`
`LANG`** 을 export하고, telnet으로 들어온 셸은 이 중 아무것도 상속하지 않는다.

문제가 되는 건 `WIFI_MODULE_PATH`다. 비어 있으면 `/drivers//wifi/.ko`가 되고,
되살아난 UI가 **무선 모듈을 다시 올리지 못한다** — 개발 세션이 자기가 타고 있던
네트워크를 조용히 잃는 경로가 이것이다. 기기 로그가 내내 말하고 있었다:

```
insmod: can't read '/drivers//wifi/sdio_wifi_pwr.ko': No such file or directory
```

이제 죽이기 전에 `/proc/<pid>/environ`에서 스냅샷을 뜨고(재구성 불가능한 dbus 세션
주소까지 보존된다), 빠진 값은 rcS와 같은 방식으로 채운다. 실기에서 확인:

```
PLATFORM=mx50-ntx  PRODUCT=kraken  INTERFACE=eth0  WIFI_MODULE=dhd
WIFI_MODULE_PATH=/drivers/mx50-ntx/wifi/dhd.ko
insmod: can't insert '...dhd.ko': File exists      ← 경로 정상, 이미 로드됨
```

### 네트워크가 사라진 진짜 이유: 우리가 버렸다

`nickel.sh`의 kill 목록이 KOReader에서 그대로 베껴온 것이었고, 거기엔 **DHCP 클라이언트가
들어 있었다.** KOReader는 Wi-Fi를 자기가 올리므로 그래도 되지만 우리는 아니다.

**`dhcpcd`는 SIGTERM을 받으면 인터페이스 설정을 해제하고 리스를 반납한다.** 즉 nickel을
멈출 때마다 **세션이 타고 있던 주소를 우리 손으로 버리고 있었다.**

실기 확인 — 클라이언트를 살려두자 주소가 살아남는다:

```
[before]              eth0 192.168.50.196
nickel: stopped
[nickel stopped 25s]  eth0 192.168.50.196
[nickel back]         eth0 192.168.50.196
```

리스는 86400초(24시간)였다. **만료가 아니었다.**

### `wifi.sh` — nickel 없이 네트워크를 올린다

펌웨어에는 Wi-Fi를 올리는 스크립트가 없다. rcS는 모듈 위치만 export하고, 실제로
모듈을 넣고 `wpa_supplicant`와 DHCP를 돌리는 건 nickel 본체다. 돌고 있는 기기에서
읽어낸 시퀀스(KOReader `enable-wifi.sh`와 대조):

```
sdio_wifi_pwr.ko → dhd.ko → ifconfig eth0 up → wlarm_le -i eth0 up
→ wpa_supplicant -D wext -c /etc/wpa_supplicant/wpa_supplicant.conf
→ dhcpcd -d -t 30 -w eth0
```

**자격증명은 이 스크립트의 일이 아니다.** nickel이 이미 joined 네트워크를
`/etc/wpa_supplicant/wpa_supplicant.conf`에 써 뒀고, 우리는 그걸 읽는 데몬만 띄운다.
Kobo UI에서 한 번도 네트워크에 붙은 적 없는 기기에는 올릴 것이 없다.

완전히 내린 상태에서 복구되는 것까지 확인했다:

```
[after down]   eth0 (no address), supplicant stopped, dhcp stopped
[bringing up]  carrier acquired → leased 192.168.50.196 for 86400 seconds
[after up]     eth0 192.168.50.196, supplicant running, dhcp running
```

첫 버전은 이 테스트를 통과하지 못했다 — `ifconfig`가 **내려간 인터페이스에도 주소를
계속 보고**하는 탓에 "이미 올라와 있음"으로 조기 반환하고 `wpa_supplicant`가 멈춘 채
성공을 반환했다. 지금은 단계마다 따로 멱등하고, 그래서 bring-up이 곧 복구 경로다.

### 안전한 전송 프로토콜 — 이걸로는 재현되지 않았다

**큰 전송은 nickel이 살아 있을 때만 한다.**

1. nickel이 떠 있는 동안 파일을 올린다
2. PocketJS를 띄운다
3. 도는 동안에는 **몇 바이트짜리 명령만** 보낸다
4. 캡처는 기기 안에서 압축만 하고, **종료해서 nickel을 되살린 뒤** 받는다

```sh
# 도는 동안: 네트워크를 안 쓴다. 1.5MB -> 18.9KB
dd if=/dev/fb0 bs=1536 count=1024 2>/dev/null | gzip -9 > /mnt/onboard/.kobo/fb.gz
killall -TERM pocketjs-kobo            # 트랩이 nickel을 되살린다
# 그 다음에 받는다
curl -u root: -o fb.gz ftp://<IP>/fb.gz
```

이 프로토콜로 7분 연속 무사고. pid 고정, MemFree 120,976 → 120,864 kB(사실상 평평).

### 유휴 CPU — `load average`는 거짓말이었다

처음에 `load average 0.99`를 보고 "코어 하나를 태운다"고 적었다. **틀렸다.**
`/proc/<pid>/stat`을 직접 재니 이렇다:

```
utime 985 -> 1275 jiffies (20초, USER_HZ=100)  =  2.90s / 20s  =  14.5%
/proc/stat idle 395451 / 총 414k               →  시스템은 약 95% 유휴
```

이 커널의 loadavg는 16ms마다 깨어나는 태스크를 자주 runnable로 잡는다.
**프로세스 CPU를 직접 재라.** 루프는 `next_tick`까지 정상적으로 sleep한다.

그래도 14.5%는 줄일 값어치가 있었고, 어디에 쓰는지는 계측이 답했다
(`POCKETJS_PROFILE_SECS=10`):

```
601 ticks in 10.0s — guest 3.2% (0.53ms/tick), raster 10.5% (1.75ms/tick), present 0.6%
```

두 번째 추측도 틀렸다. 래스터는 이미 증분(`render_scaled_gray8_incremental`)이라
픽셀 작업이 아니라 **틱 수가 문제였다.**

### 고친 방법: 패널보다 빨리 시뮬레이션하지 않는다

패널은 잘해야 30Hz로 present하고 DU 파형은 한 프레임보다 오래 걸린다. 60Hz로 도는
프레임 대부분은 **유리에 닿을 수 없다.**

`--sim-hz`가 이 레이트를 런타임이 이미 그렇다고 말하는 **호스트 정책**으로 만든다
(docs/DETERMINISM.md). `__pak`과 같은 계약 슬롯에 `__simHz`로 공표하고, 루프도 같은
숫자로 pace한다 — 그래서 게스트의 시계가 벽시계와 어긋날 수 없다.

| `--sim-hz` | CPU (코어 1개 대비) | 틱당 | 검증 |
| --- | --- | --- | --- |
| 60 (기존) | **14.5%** | 2.2ms | |
| 30 (기본값) | **7%** | 2.2ms | `301 ticks in 10.0s` |
| 10 | **2%** | 2.3ms | `101 ticks in 10.1s` |

틱 수가 선언한 레이트와 일치한다는 것이 **`virtualNow()`가 벽시계를 따라간다는 증명**이다.
시계가 느려지지 않는다.

기본값 30은 동작 변화가 없는 안전한 값이다. 사람의 시간 단위로 바뀌는 화면
(시계, 상태판)은 더 낮춰야 한다 — 터치는 어느 쪽이든 한 프레임 안에 잡히고,
패널이 그보다 느리다.

## 전원 키와 유휴 CPU (사다리 4번)

### 전원 키는 evdev로 온다 — nickel이 잡고 있지 않을 때만

`mxckpd`(event0)의 capability 비트맵에 `KEY_POWER`(116)가 있다. 그런데 nickel이 도는
동안 그 노드를 `dd`로 캡처하면 **0바이트**다. 하마터면 "이 기기는 전원 키 이벤트를
안 쏜다"고 결론 낼 뻔했는데, **nickel이 `EVIOCGRAB`으로 독점**하고 있어서였다
(우리 호스트가 터치스크린에 하는 것과 같은 동작).

nickel을 멈추고 재니 그대로 나온다:

```
type=0x0001 (EV_KEY)  code=0x0074 (116 = KEY_POWER)  value=1/0
```

우리 호스트는 애초에 nickel이 없을 때만 도니 조건이 맞는다.

| 누름 | 동작 | 실기 |
| --- | --- | --- |
| 짧게 | `power.sh suspend` → 성공 시 리로드 | 미검증 (아래) |
| **1.5초 이상** | 종료 → 런처 트랩이 nickel 복구 | **확인됨** |

긴 누르기가 **없던 탈출구**다. "멈춘 것처럼 보이는 기기"에서 강제 차단 말고 빠져나올
방법이 이제 있다. 판정은 뗄 때 한다 — 누르는 중에 "무엇을 하려는지" 알려줄 방법이
e-ink에는 없으므로 애매한 중간 상태를 만들지 않는다.

### suspend는 이 모델에서 미해결

- **nickel의 "잠자기"는 suspend-to-RAM이 아니다.** 자는 동안 telnet이 응답했고
  `/proc/uptime`이 끊기지 않았다 — 화면만 끄는 상태다.
- `power.sh`는 `echo mem`으로 **nickel이 이 기기에서 하지 않는 더 깊은 잠**을 요구한다.
  한 번 시도했고 **돌아오지 않았다** (재부팅으로 끝남).
- 이 rtc에는 `wakealarm`이 없다. 타이머로 깨울 수단이 없으므로 시험에는 사람이 필요하다.

문서에 미해결로 남긴다. `--power-helper`를 다른 것으로 돌리거나 `--no-power-key`를 쓴다.

### 유휴 CPU: 안 변한 프레임은 안 그린다

`DrawList`는 `Vec<u32>` 하나라 "같은 프레임인가"가 **memcmp**다. 같으면 다시 그릴 것이
없고, 유지 중인 래스터와 대기 중인 damage가 모두 그대로 유효하다.

**프레임은 계속 스케줄대로 돌린다.** 가상 시간이 프레임 카운터라 하나라도 빠뜨리면
게스트의 시계가 멈춘다 — 절약은 "덜 돌린다"가 아니라 "유휴 틱을 싸게 만든다"에서 나와야 한다.

| | CPU (코어 1개 대비) |
| --- | --- |
| 60Hz, 매 틱 리페인트 | 14.5% |
| 30Hz (`--sim-hz`) | 7% |
| 30Hz + 미변경 프레임 스킵 | **4%** |

시계 두 분 사이에 `0 repainted / 301 ticks`였다.

남은 것은 `ui.draw()`가 매 틱 리스트를 다시 만드는 비용(약 1.1ms)이고, **엔진 쪽이라
모든 호스트가 공유한다.** 유휴 틱에서 `words` 복사를 없애는 것도 시도해 실기에서
**측정했으나 아무 차이가 없었다**(1.12ms 그대로) — 할당은 원인이 아니었고, 미리 물어보는
방식은 리페인트하는 틱에서 `draw()`를 한 번 더 부르는 대가만 생긴다. 그래서 넣지 않았다.

## 설계 판단이 하드웨어에서 검증된 것

| 판단 | 실기 결과 |
| --- | --- |
| `var.rotate`는 참고값, 방향은 가시 래스터에서 유도 | 드라이버가 `rotate=3` 보고 → 경고 후 R0 유도, **정확** |
| 379×512 @2x = 758×1024 풀패널, 레터박스 없음 | `viewWidth=758 viewHeight=1024` 일치 |
| Glo는 non-MT 싱글터치 | `isKoboNonMT=1`, `zForce-ir-touch` 싱글 컨택트 |
| musl 정적 링크로 glibc 무관 | FW 3.19(2016) 기기에서 그대로 실행 |
| FBInk는 링크가 아니라 런타임 의존 | KOReader의 `fbink`(GLIBC_2.4)를 그대로 씀 |
| nickel 복구는 `EXIT` 트랩 | 앱 크래시 시 **실제로 UI 복구됨** |
| Wi-Fi를 안 끄는 선택 | 크래시·재시작 내내 telnet 세션 유지 |

## 남은 STOP

`GATE 6`에서 1~3번이 해소됐다. 남은 것:

1. ~~셸 접근~~ — **해소.** `EnableDebugServices=true` → telnet(23) + FTP(21), root/무비번.
2. ~~프레임버퍼 실측~~ — **해소.** `mxc_epdc_fb 758×1024 Rgb565, stride 1536`.
3. ~~터치 축~~ — **해소.** 위 참조.
4. **FBInk `-s` 좌표계** — FBInk 문서상 **Kobo에서는 `-s` 사각형이 회전/뷰포트 퀵 없이
   ioctl로 그대로 전달**된다. 호스트는 `/dev/fb0`에 쓸 때와 같은 좌표계로 계산하므로
   일치해야 하지만, 갱신 영역이 엉뚱한 곳에 뜨면 여기부터 본다.
   **`GATE 6`에서 갱신 위치 이상은 관찰되지 않았다** — 즉 일치하는 것으로 보인다.
5. **파형 튜닝** — 기본값(DU 모션, ghost-budget 80)으로 이미 "거슬리지 않는" 수준이다.
   더 조일 여지는 남아 있다. `pocketjs.sh`가 남는 인자를 호스트로 넘기므로
   `./pocketjs.sh --motion-waveform A2 --ghost-budget 40` 식으로 세션마다 바꿔 본다.

## 디바이스 브링업 (P2.5)

`hosts/kobo/device/` 3개 + `hosts/kobo/tests/device-scripts.sh`.

| 스크립트 | 무엇 |
| --- | --- |
| `pocketjs.sh` | nickel 정지 → 호스트 실행 → **모든 종료 경로에서** nickel 복구 |
| `nickel.sh` | `stop` / `start` / `status` 단독 실행 (프로브·복구용) |
| `diagnose.sh` | 읽기 전용 진단 리포트. 아무것도 안 멈추고 안 쓴다 |

설계상 지킨 것:

- **복구는 `EXIT` 트랩.** 크래시든 `kill`이든 잘못된 번들이든 UI가 돌아온다.
- **tmpfs 재실행.** `/mnt/onboard`는 FAT32라 USB를 꽂는 순간 사라진다. 런처가 자기
  자신과 `nickel.sh`를 `/tmp`에 복사해 거기서 `exec` 한 뒤에야 nickel을 건드린다.
  안 그러면 스크립트 본문이 사라져 nickel이 영영 안 돌아온다.
- **Wi-Fi는 건드리지 않는다.** KOReader는 nickel 재시작 전에 인터페이스를 내리지만,
  개발 루프가 그 인터페이스 위에서 돈다(telnet/SSH). 대신 nickel이 자기가 안 올린
  인터페이스를 싫어할 수 있다는 점을 감수한다 — 이상하면 리부트.
- **nickel 재시작 시퀀스는 KOReader `platform/kobo/nickel.sh`를 따른다.** 직접
  발명하지 않았다. FW5 경로(`/etc/init.d/z-nickel-hardware-status`)가 보이면
  적용 대상이 아니라고 판단하고 리부트하라고 말하고 멈춘다.
- **하드웨어 없이 검증된다.** `NICKEL_ROOT`로 스테이지 루트를 가리키고 `pidof`/
  `killall`/`usleep`을 스텁으로 갈아끼워 실제 스크립트를 돌린다. 19개 체크 통과.

## 한글 폰트 (P4 선행 조사)

**결론: 된다. 폰트만 갈아끼우면 된다. 다만 비용이 글리프 수에 정비례한다.**

### 파이프라인이 실제로 하는 일

컴파일러 pass 1이 **소스 리터럴에서 코드포인트를 직접 수집**한다. 한글도 예외가 아니다 —
`apps/hangul-probe`를 붙였더니 수집 코드포인트가 86 → 157로 늘었다. 즉 별도 설정 없이
소스에 쓴 한글은 전부 후보에 오른다.

문제는 폰트다. 기본 Inter에는 한글 cmap이 없고, **매핑 없는 코드포인트는 조용히 버려진다**
(`bake-font.ts`: "Codepoints the font does not map are simply left out" → gid 0 = 두부).
에러가 아니라 두부로 나오므로 눈으로 보기 전엔 모른다.

| 빌드 | 수집 코드포인트 | baked glyph | pak |
| --- | --- | --- | --- |
| Inter (기본) | 157 | 98 (한글 전부 탈락) | 608KB |
| Nanum Gothic | 157 | **169** | 882KB |

Nanum Gothic으로 구우면 758×1024 Gray8에서 조합·받침·볼드·ASCII 혼용이 전부 정상이다
(`render_gray`로 육안 확인).

### 비용 — 정비례다

아틀라스는 **고정 셀 격자**다. 셀 크기는 슬롯 안 최대 잉크 폭/높이로 정해지는데,
한글을 넣는 순간 이미 최대치가 되므로 **글리프를 더 넣어도 셀은 안 커진다.**
1000자를 더 넣어 확인했다 — 셀 치수 13x12 / 17x17 / 21x20 그대로.

따라서 비용은 순수 선형이고, 이 앱 기준 **고유 음절당 약 5.2KB**(슬롯 5개 합산):

| 음절 수 | pak | 비고 |
| --- | --- | --- |
| 169 | 882KB | 실측. 정적 UI 문자열 수준 |
| 1,160 | 6.0MB | 실측 |
| 2,350 | ~12MB | KS X 1001 완성형 전체 (외삽) |
| 11,172 | ~58MB | 현대 한글 전 영역 (외삽). 불가 |

슬롯을 줄이면 그만큼 준다 — 16px 본문 + 20px 볼드만 쓰면 음절당 약 2.9KB다.

### 그래서 P4 앱 설계에 걸리는 제약

- **정적 한글 UI는 사실상 공짜다.** 라벨 100자 ≈ 520KB.
- **`net.http`로 받아오는 한글은 미리 구워야 한다.** 빌드 시점에 모르는 글자는 두부가 된다.
  날씨 앱이라면 어휘가 유한하므로(하늘 상태, 지역명, 단위) `--extra-chars`로 넣으면 된다.
  **임의의 한글 텍스트를 띄우는 앱은 이 아틀라스 설계로는 안 된다.**
- 두부가 나와도 빌드는 성공한다. 한글 앱은 `render_gray` 육안 확인이 필수다.

### 빌드 방법 (두 단계인 이유)

`tools/pocket.ts`는 `--font-regular` / `--font-bold` / `--extra-chars`를 **전달하지 않는다.**
플래그는 `tools/build.ts`에만 있다. `pocket.config.ts`에도 폰트 항목이 없다.
그래서 plan을 먼저 만들고 컴파일러를 직접 부른다:

```sh
# 폰트 (OFL). 저장소에 커밋하지 않는다 — 2MB짜리 바이너리 2개다.
curl -sSLO https://github.com/google/fonts/raw/main/ofl/nanumgothic/NanumGothic-Regular.ttf
curl -sSLO https://github.com/google/fonts/raw/main/ofl/nanumgothic/NanumGothic-Bold.ttf

# 1) plan.json 생성 (이 빌드 산출물은 두부다 — 버린다)
bun tools/pocket.ts compile --target kobo-glo \
  --manifest apps/hangul-probe/pocket.json --project-root .

# 2) 한글 폰트로 다시 굽는다
bun tools/build.ts --plan=.pocket/kobo-glo/plan.json --project-root=. \
  --outdir=dist --hz=60 \
  --font-regular=$PWD/NanumGothic-Regular.ttf \
  --font-bold=$PWD/NanumGothic-Bold.ttf \
  --extra-chars="동적으로받을수있는글자들"

# 3) 눈으로 확인 (두부는 빌드를 실패시키지 않는다)
cargo run --manifest-path hosts/kobo/Cargo.toml --example render_gray -- \
  dist/hangul-probe-main.js dist/hangul-probe-main.pak /tmp/hangul-probe
```

## 환경 (재현용)

**원 작업 머신** — Ubuntu 24.04, **aarch64**, 헤드리스(GPU 없음 → gpui 데스크톱 호스트 사용 불가)

```
rustc 1.98.1 / cargo 1.98.1        targets: aarch64-unknown-linux-gnu,
                                            armv7-unknown-linux-gnueabihf,
                                            armv7-unknown-linux-musleabihf
zig 0.15.2                          ~/.local/zig/zig
cargo-zigbuild 0.23.4
clang 18.1.3 / libclang             /usr/lib/llvm-18/lib
bun 1.4.0, node v24.19.0
```

**2차 재현 머신** — macOS 26.5.1, arm64. 게이트 (a)–(f) 전부 동일한 결과.

```
rustc 1.95.0 / cargo 1.95.0        target: armv7-unknown-linux-musleabihf
zig 0.15.2                          ~/.local/zig/zig  (macos-aarch64 타르볼)
cargo-zigbuild 0.23.4
libclang                            /Library/Developer/CommandLineTools/usr/lib
bun 1.4.0, node v24.20.0
```

즉 툴체인은 문서에 못박은 조합보다 넓게 동작한다. **zig만 0.15.2로 고정**하면 된다.
macOS에는 `readelf`가 없으므로 hard-float 확인은 ELF 헤더의 `e_flags`를 직접 읽는다
(`0x05000400`이어야 한다). `rustfmt`는 버전에 따라 `examples/render_gray.rs`에서
차이를 내는데, 원 작업 머신의 포매팅이므로 **건드리지 않는다.**

클론: `~/kobo-pocketjs/{pocketjs,inkview-rs,FBInk}`

헤드리스라 쿡북 §4-4의 "데스크톱 창 렌더 확인"은 불가능하다. 대신 `GATE 5-2`가
그 역할을 대체하며, e-ink 경로를 직접 검증하므로 더 적합하다:

```sh
cargo run --manifest-path hosts/kobo/Cargo.toml --example render_gray -- \
  dist/paper-ink-main.js dist/paper-ink-main.pak /tmp/paper-ink
```

## 다음 단계

`GATE 6`가 끝났으므로 갈림길이 셋이다.

1. **업스트림 draft PR.** §7의 조건("실기 검증 전에는 보내지 않는다")이 충족됐다.
   `agent/kindle-hero`가 아직 머지 전이라 공통 e-ink 계층 추출 요구가 나올 수 있다.
   `borderRadius` 건은 kindle 호스트에도 해당하므로 그쪽에도 알릴 가치가 있다.
2. **파형 튜닝** — 남은 STOP 5. 기기만 있으면 되고 코드 변경은 거의 없다.
3. **P4 = `net.http` 호스트 구현.** 아래.

### P4에 걸린 것: `net.http`가 아직 없다

- `kobo-glo` 프로필은 `net.http`를 광고하지 않는다.
- `engine/crates/pocket-net`은 transport-neutral `NetCore<T: HttpTransport>`만 준다.
  **어떤 Rust 호스트도 아직 이걸 물고 있지 않다** — 구현체는 브라우저(JS)와 sim(TS)뿐이다.

즉 날씨 앱을 하려면 Kobo 호스트가 **PocketJS 최초의 네이티브 Rust 네트워킹 호스트**가
된다. 붙어 있는 일: `HttpTransport` 구현, 게스트 모듈 마운트, 프로필에 capability 추가,
**armv7 musl 정적 링크에서의 TLS**(rustls 백엔드 선택이 리스크), 그리고 nickel을 죽인
상태에서의 **Wi-Fi 자체 기동**(지금은 개발 세션이 Wi-Fi를 들고 있어서 가려져 있다).

GATE 6 이후로 미룬 이유가 이것이다 — 한 번도 화면을 안 띄운 호스트 위에 네트워크
스택을 올리는 건 순서가 틀렸다. 이제 그 조건은 해소됐다.

Wi-Fi 항목 하나가 특히 가려져 있다: 지금은 **개발 세션이 Wi-Fi를 들고 있어서**
문제가 안 보인다. nickel 없이 단독 실행하려면 호스트나 스크립트가 직접 올려야 한다.

절차와 지뢰 목록은 [HANDOVER.md](HANDOVER.md)에 정리돼 있다.
