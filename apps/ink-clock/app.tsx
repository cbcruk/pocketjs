// Ink Clock — a status screen shaped by what e-ink is good at.
//
// The panel holds an image for free and pays for every change, so this app
// changes once a minute and never animates. Tapping swaps between the clock
// and a runtime panel; that is the only interaction, and it is the only
// full-screen redraw the app ever asks for.
//
// The clock is 36px, not the 54px slot it wants to be: the glyph atlas bakes
// the WHOLE charset into every slot it uses, so a 54px slot costs 2 MB once
// Korean is in the charset — for five digits and a colon. Font size is a pak
// budget decision on this target, not a typographic one.
//
// Time comes from the host's one-shot `__bootClock` (see publish_boot_clock in
// hosts/kobo/src/main.rs) plus virtualNow(). The runtime has no wall clock by
// design, so a long session drifts as dropped logic ticks accumulate — the
// runtime panel shows the drift-free thing it can prove, and a SIGHUP reload
// resynchronizes.
import { For, createMemo, createSignal } from "solid-js";
import { Text, View } from "@pocketjs/framework/components";
import { onFrame } from "@pocketjs/framework/lifecycle";
import { touches } from "@pocketjs/framework/input";
import { simulationHz, virtualNow } from "@pocketjs/framework/clock";
import {
  SECONDS_PER_DAY,
  WEEKDAY_NAMES,
  addDays,
  pad2,
  readBootClock,
} from "./calendar.ts";

const PAPER = "#f6f4ef";
const INK = "#161616";
const MUTED = "#6b675f";
const RULE = "#c9c3b8";
const PANEL = "#eceae3";
// <Text> draws one line: wrapping is an explicit host op (spec `wrapText`),
// which a fixed-layout screen like this one has no reason to pay for. The copy
// is broken to fit 331 px — the 379 px viewport less its 24 px gutters.
const CAPTION = [
  "이 화면은 1분에 한 번만 다시 그립니다.",
  "전자잉크는 정지 화면에 전력을 쓰지 않으니,",
  "움직이지 않는 것이 곧 절약입니다.",
] as const;

export default function InkClock() {
  const boot = readBootClock();
  // Minutes since boot. The whole screen is derived from this, so the frame
  // loop only pushes a new value 一 once per minute, and the damage tracker
  // sees nothing to repaint in between.
  const [minute, setMinute] = createSignal(-1);
  const [showRuntime, setShowRuntime] = createSignal(false);
  let contactWasDown = false;

  onFrame(() => {
    const elapsed = Math.floor(virtualNow());
    const current = Math.floor((boot.secondOfDay + elapsed) / 60);
    if (current !== minute()) setMinute(current);

    const down = touches().length > 0;
    if (down && !contactWasDown) setShowRuntime((value) => !value);
    contactWasDown = down;
  });

  // minute() counts whole minutes since local midnight of the boot day, so the
  // seconds it stands for are simply that times sixty.
  const clock = createMemo(() => {
    const secondsNow = Math.max(0, minute()) * 60;
    const days = Math.floor(secondsNow / SECONDS_PER_DAY);
    const withinDay = secondsNow - days * SECONDS_PER_DAY;
    return {
      ...addDays(boot, days),
      hour: Math.floor(withinDay / 3600),
      minute: Math.floor(withinDay / 60) % 60,
    };
  });

  const uptime = createMemo(() => {
    // Read the signal so this recomputes with the display; virtualNow() is a
    // plain function and would otherwise be sampled once and frozen.
    const minutes = Math.max(0, minute() - Math.floor(boot.secondOfDay / 60));
    return minutes >= 60 ? `${Math.floor(minutes / 60)}시간 ${minutes % 60}분` : `${minutes}분`;
  });

  return (
    <View class="relative w-full h-full overflow-hidden" style={{ bgColor: PAPER }}>
      <View class="absolute left-[24] top-[26] right-[24] flex-col gap-1">
        <Text class="text-sm tracking-wide" style={{ textColor: MUTED }}>
          {WEEKDAY_NAMES[clock().weekday]}요일
        </Text>
        <View class="mt-2 w-full h-[1]" style={{ bgColor: RULE }} />
      </View>

      <View class="absolute left-[24] top-[96] right-[24] flex-col gap-2">
        <Text class="text-4xl font-bold" style={{ textColor: INK }}>
          {pad2(clock().hour)}:{pad2(clock().minute)}
        </Text>
        <Text class="text-lg" style={{ textColor: MUTED }}>
          {clock().year}년 {clock().month}월 {clock().day}일
        </Text>
      </View>

      <View class="absolute left-[24] right-[24] top-[232] h-[1]" style={{ bgColor: RULE }} />

      <View class="absolute left-[24] right-[24] top-[258] flex-col gap-3">
        {showRuntime() ? (
          <>
            <Row label="가동" value={uptime()} />
            <Row label="논리 화면" value="379 × 512 @2x" />
            <Row label="패널" value="758 × 1024" />
            <Row label="논리 초당" value={`${simulationHz()}프레임`} />
            <Row label="부팅 시각" value={`${pad2(Math.floor(boot.secondOfDay / 3600))}:${pad2(Math.floor(boot.secondOfDay / 60) % 60)}`} />
          </>
        ) : (
          <>
            <Text class="text-base" style={{ textColor: INK }}>
              화면을 누르면 실행 정보를 봅니다
            </Text>
            <For each={CAPTION}>
              {(line) => (
                <Text class="text-xs" style={{ textColor: MUTED }}>
                  {line}
                </Text>
              )}
            </For>
          </>
        )}
      </View>

      <View
        class="absolute left-[24] right-[24] bottom-[24] flex-row justify-between items-center px-3 py-2 border-[1]"
        style={{ bgColor: PANEL, borderColor: RULE }}
      >
        <Text class="text-xs font-bold" style={{ textColor: INK }}>
          {showRuntime() ? "실행 정보" : "잉크 시계"}
        </Text>
        <Text class="text-xs" style={{ textColor: MUTED }}>
          KOBO GLO
        </Text>
      </View>
    </View>
  );
}

function Row(props: { label: string; value: string }) {
  return (
    <View class="flex-row justify-between items-center">
      <Text class="text-sm" style={{ textColor: MUTED }}>
        {props.label}
      </Text>
      <Text class="text-sm font-bold" style={{ textColor: INK }}>
        {props.value}
      </Text>
    </View>
  );
}
