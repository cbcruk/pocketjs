// Ghost Probe — the screen STOP 5 needs to be judged on.
//
// Ghosting is residue an e-ink panel leaves when a fast waveform does not
// fully drive the pixels to their new state. It cannot be measured from the
// host: the framebuffer holds what we asked for, not what the panel shows. So
// the judgement is a person looking at the panel, and the only thing software
// can do is make the residue obvious and say which run produced it.
//
// Three bands, each provoking a different failure:
//
//   SWEEP     a black bar stepping across white. Residue appears as a trail
//             behind it — the clearest signal, and the one A2 is worst at.
//   FLIP      blocks inverting in place. Residue appears as grey where black
//             and white alternated, which is what DU leaves after many cycles.
//   REFERENCE never repainted after the first frame, and laid out clear of the
//             bands above so the host's bounding-box update never reaches it.
//             Anything that shows up here bled in from a neighbour.
//
// TWO THINGS THE HOST DOES DECIDE WHAT THIS APP MAY LOOK LIKE, and the first
// version of it got both wrong:
//
//   The motion waveform is only used while the screen is MOVING, which
//   Refresher defines as damage within MOTION_WINDOW (120ms) of the last.
//   A probe stepping every 500ms is never moving by that definition: every
//   update took the static path and rendered with Waveform::Auto, so three
//   runs of --motion-waveform proved nothing about DU or A2. Step faster than
//   that window or you are not testing what you think you are.
//
//   Going quiet triggers on_idle, which runs a full GC16 over everything that
//   moved. So "freeze it and look" erases the residue 200ms before you look.
//   Freezing here stops the bands but not the heartbeat, whose damage keeps
//   the host in motion and the evidence on the panel.
//
// The heartbeat sits inside the sweep track on purpose: damage is merged into
// ONE bounding box (merge_all), so a heartbeat in a far corner would stretch
// every update across the whole panel and repaint the very residue we came
// to read.
import { For, createMemo, createSignal } from "solid-js";
import { Text, View } from "@pocketjs/framework/components";
import { onFrame } from "@pocketjs/framework/lifecycle";
import { touches } from "@pocketjs/framework/input";
import { simulationHz, virtualNow } from "@pocketjs/framework/clock";
import { readInkPolicy } from "./policy.ts";

const PAPER = "#ffffff";
const INK = "#000000";
const MUTED = "#666666";
const RULE = "#bbbbbb";

/** Seconds between steps. Must stay under the host's 120ms MOTION_WINDOW.
 *
 * Measured, not chosen: at 0.1 the 30Hz virtual clock lands 3 or 4 ticks apart
 * and the slow case is 133ms, which is outside the window — the probe drops
 * back to the static path for a fifth of its updates and stops testing the
 * waveform. Two ticks is 67ms with no rounding to argue about. */
const STEP_SECONDS = 0.05;
const SWEEP_STEPS = 11;
const SWEEP_BAR = 26;
/** Left edge of the heartbeat, clear of the bar's travel (10 * 26 + 26). */
const HEARTBEAT_AT = 300;
const HEARTBEAT_SIZE = 26;
const FLIP_COLUMNS = 8;
/** Greys the reference band is drawn in, darkest first. */
const REFERENCE_GREYS = ["#000000", "#404040", "#808080", "#c0c0c0"] as const;

export default function GhostProbe() {
  const policy = readInkPolicy();
  // One integer drives every band, so a frozen probe is genuinely frozen:
  // nothing recomputes and the damage tracker finds nothing to repaint.
  const [step, setStep] = createSignal(0);
  // Beats whether or not the bands do, so the host never sees the screen go
  // quiet and never runs the cleanup that would wipe what we are reading.
  const [beat, setBeat] = createSignal(0);
  const [frozen, setFrozen] = createSignal(false);
  let contactWasDown = false;
  let steppedAt = 0;

  onFrame(() => {
    const down = touches().length > 0;
    if (down && !contactWasDown) setFrozen((value) => !value);
    contactWasDown = down;

    const now = virtualNow();
    if (now - steppedAt < STEP_SECONDS) return;
    steppedAt = now;
    setBeat((value) => value + 1);
    if (!frozen()) setStep((value) => value + 1);
  });

  const sweepAt = createMemo(() => {
    // Bounce rather than wrap: a wrap repaints the whole track at once, which
    // is a cleanup in disguise and hides exactly what we are looking for.
    const span = SWEEP_STEPS - 1;
    const phase = step() % (span * 2);
    return phase <= span ? phase : span * 2 - phase;
  });

  const flipped = createMemo(() => step() % 2 === 1);

  return (
    <View class="relative w-full h-full overflow-hidden" style={{ bgColor: PAPER }}>
      <View class="absolute left-[24] top-[20] right-[24] flex-col gap-1">
        <View class="flex-row justify-between items-center">
          <Text class="text-sm font-bold" style={{ textColor: INK }}>
            {policy.motionWaveform} · budget {policy.ghostBudget}
          </Text>
          <Text class="text-sm" style={{ textColor: INK }}>
            {step()}
          </Text>
        </View>
        <Text class="text-xs" style={{ textColor: MUTED }}>
          {policy.presentHz}Hz 표시 · {simulationHz()}Hz 논리 ·{" "}
          {frozen() ? "정지 — 지금 잔상을 본다" : "동작 중 — 누르면 멈춘다"} ·{" "}
          {beat()}
        </Text>
        <View class="mt-2 w-full h-[1]" style={{ bgColor: RULE }} />
      </View>

      <Band label="SWEEP — 막대 뒤에 꼬리가 남는가" top={92}>
        <View class="relative w-full h-[44]" style={{ bgColor: PAPER }}>
          <View
            class="absolute top-0 h-[44]"
            style={{
              bgColor: INK,
              insetL: sweepAt() * SWEEP_BAR,
              width: SWEEP_BAR,
            }}
          />
          <View
            class="absolute top-0 h-[44]"
            style={{
              bgColor: beat() % 2 === 0 ? INK : PAPER,
              insetL: HEARTBEAT_AT,
              width: HEARTBEAT_SIZE,
            }}
          />
        </View>
      </Band>

      <Band label="FLIP — 반전 자리에 회색이 남는가" top={196}>
        <View class="flex-row w-full h-[44]">
          <For each={Array.from({ length: FLIP_COLUMNS }, (_, index) => index)}>
            {(column) => (
              <View
                class="flex-1 h-[44]"
                style={{
                  bgColor: (column % 2 === 0) === flipped() ? INK : PAPER,
                }}
              />
            )}
          </For>
        </View>
      </Band>

      <Band label="REFERENCE — 여기가 더러워지면 이웃이 번진 것" top={300}>
        <View class="flex-row w-full h-[44]">
          <For each={REFERENCE_GREYS}>
            {(grey) => <View class="flex-1 h-[44]" style={{ bgColor: grey }} />}
          </For>
        </View>
      </Band>

      <View class="absolute left-[24] right-[24] top-[380] flex-col gap-2">
        <View class="w-full h-[1]" style={{ bgColor: RULE }} />
        <Text class="text-xs" style={{ textColor: MUTED }}>
          정지해도 오른쪽 끝 사각형은 계속 깜빡인다 — 그게 멈춰 있으면
        </Text>
        <Text class="text-xs" style={{ textColor: MUTED }}>
          호스트가 전면 정리를 걸어 증거를 지운 뒤다.
        </Text>
        <Text class="text-xs" style={{ textColor: MUTED }}>
          잔상이 거슬리면 --ghost-budget을 낮춘다. 전면 갱신이 잦아지고
        </Text>
        <Text class="text-xs" style={{ textColor: MUTED }}>
          화면이 그만큼 자주 번쩍인다 — 둘 중 하나를 고르는 일이다.
        </Text>
      </View>

      <View
        class="absolute left-[24] right-[24] bottom-[24] flex-row justify-between items-center px-3 py-2 border-[1]"
        style={{ borderColor: RULE }}
      >
        <Text class="text-xs font-bold" style={{ textColor: INK }}>
          잔상 프로브
        </Text>
        <Text class="text-xs" style={{ textColor: MUTED }}>
          {frozen() ? "정지" : "동작"}
        </Text>
      </View>
    </View>
  );
}

function Band(props: { label: string; top: number; children: unknown }) {
  return (
    <View class="absolute left-[24] right-[24] flex-col gap-1" style={{ insetT: props.top }}>
      <Text class="text-xs" style={{ textColor: MUTED }}>
        {props.label}
      </Text>
      {props.children as never}
    </View>
  );
}
