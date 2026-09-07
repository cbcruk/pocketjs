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
//   REFERENCE never repainted after the first frame. Anything that shows up
//             here came from its neighbours, not from its own updates.
//
// Tapping freezes every band. Frozen is when you judge: a moving panel hides
// residue behind the next update. The header keeps counting so you can see
// how far past the ghost budget the run is.
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

const SWEEP_STEPS = 12;
const SWEEP_TRACK = 331;
const SWEEP_BAR = Math.floor(SWEEP_TRACK / SWEEP_STEPS);
const FLIP_COLUMNS = 8;
/** Greys the reference band is drawn in, darkest first. */
const REFERENCE_GREYS = ["#000000", "#404040", "#808080", "#c0c0c0"] as const;

export default function GhostProbe() {
  const policy = readInkPolicy();
  // One integer drives every band, so a frozen probe is genuinely frozen:
  // nothing recomputes and the damage tracker finds nothing to repaint.
  const [step, setStep] = createSignal(0);
  const [frozen, setFrozen] = createSignal(false);
  let contactWasDown = false;
  let steppedAt = 0;

  onFrame(() => {
    const down = touches().length > 0;
    if (down && !contactWasDown) setFrozen((value) => !value);
    contactWasDown = down;

    if (frozen()) return;
    // Twice a second. Faster than the panel can honestly present, and the
    // bands blur into each other; slower and a session takes minutes to
    // reach the ghost budget.
    const now = virtualNow();
    if (now - steppedAt < 0.5) return;
    steppedAt = now;
    setStep((value) => value + 1);
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
          {frozen() ? "정지 — 지금 잔상을 본다" : "동작 중 — 누르면 멈춘다"}
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
          정지시킨 뒤 SWEEP의 지나간 자리와 FLIP의 경계를 본다.
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
