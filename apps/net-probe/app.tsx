// Net Probe — does this device actually reach the network, and over TLS?
//
// The kobo host is the first to implement pocket-net's HttpTransport, so
// "net.http is advertised" and "net.http works here" are separate claims and
// this screen is the second one. It runs the two requests side by side
// because they fail for different reasons: plain HTTP proves routing, DNS and
// the transport, and HTTPS proves the TLS that had to be statically linked —
// this firmware's own OpenSSL is 0.9.8l from 2009 and its wget will not
// accept an https URL, so there was nothing on the device to borrow.
//
// Tapping runs both again. Timings are wall-clock from the guest's frame
// counter, so they are honest to about a frame — enough to tell a handshake
// from a hang, which is all they are here for.
import { For, createSignal } from "solid-js";
import { Text, View } from "@pocketjs/framework/components";
import { onFrame } from "@pocketjs/framework/lifecycle";
import { touches } from "@pocketjs/framework/input";
import { virtualNow } from "@pocketjs/framework/clock";
import { fetch } from "@pocketjs/framework/net";

const PAPER = "#f6f4ef";
const INK = "#161616";
const MUTED = "#6b675f";
const RULE = "#c9c3b8";

const TARGETS = [
  { key: "http", url: "http://example.com/" },
  { key: "https", url: "https://example.com/" },
] as const;

interface Probe {
  readonly state: "대기" | "요청 중" | "성공" | "실패";
  readonly detail: string;
  readonly seconds: number;
}

const WAITING: Probe = { state: "대기", detail: "", seconds: 0 };

// <Text> draws one line and wrapping is an explicit host op, so the copy is
// broken by hand to fit 331px — the 379px viewport less its 24px gutters.
const CAPTION = [
  "HTTP는 경로와 DNS를 확인하고,",
  "HTTPS는 이 바이너리에 정적 링크한 TLS를 확인합니다.",
  "기기의 OpenSSL은 2009년판이라 빌려 쓸 수 없었습니다.",
] as const;

export default function NetProbe() {
  const [results, setResults] = createSignal<Record<string, Probe>>({
    http: WAITING,
    https: WAITING,
  });
  const [runs, setRuns] = createSignal(0);
  let contactWasDown = false;
  let started = false;

  const update = (key: string, probe: Probe) =>
    setResults((previous) => ({ ...previous, [key]: probe }));

  const runAll = () => {
    setRuns((value) => value + 1);
    for (const target of TARGETS) {
      const startedAt = virtualNow();
      update(target.key, { state: "요청 중", detail: target.url, seconds: 0 });
      fetch(target.url, { timeoutMs: 20_000, maxBytes: 32 * 1024 })
        .then(async (response) => {
          // Read the body, not just the status: a transport that reports 200
          // and hands back nothing is a different bug, and one worth seeing.
          const text = await response.text();
          update(target.key, {
            state: response.ok ? "성공" : "실패",
            detail: `${response.status} · ${response.byteLength}바이트 · ${firstLine(text)}`,
            seconds: virtualNow() - startedAt,
          });
        })
        .catch((error: { code?: string; message?: string }) => {
          update(target.key, {
            state: "실패",
            // The portable code is the useful half: dns, tls and timeout each
            // mean something different about where this stopped working.
            detail: `${error.code ?? "?"} · ${error.message ?? ""}`,
            seconds: virtualNow() - startedAt,
          });
        });
    }
  };

  onFrame(() => {
    if (!started) {
      started = true;
      runAll();
    }
    const down = touches().length > 0;
    if (down && !contactWasDown) runAll();
    contactWasDown = down;
  });

  return (
    <View class="relative w-full h-full overflow-hidden" style={{ bgColor: PAPER }}>
      <View class="absolute left-[24] top-[26] right-[24] flex-col gap-1">
        <Text class="text-lg font-bold" style={{ textColor: INK }}>
          네트워크 확인
        </Text>
        <Text class="text-xs" style={{ textColor: MUTED }}>
          화면을 누르면 다시 요청합니다 · {runs()}회
        </Text>
        <View class="mt-2 w-full h-[1]" style={{ bgColor: RULE }} />
      </View>

      <View class="absolute left-[24] right-[24] top-[110] flex-col gap-4">
        <For each={TARGETS}>
          {(target) => {
            const probe = () => results()[target.key] ?? WAITING;
            return (
              <View class="flex-col gap-1">
                <View class="flex-row justify-between items-center">
                  <Text class="text-base font-bold" style={{ textColor: INK }}>
                    {target.key.toUpperCase()}
                  </Text>
                  <Text class="text-sm" style={{ textColor: INK }}>
                    {probe().state}
                    {probe().seconds > 0 ? ` · ${probe().seconds.toFixed(1)}초` : ""}
                  </Text>
                </View>
                <Text class="text-xs" style={{ textColor: MUTED }}>
                  {clip(probe().detail, 54)}
                </Text>
                <View class="mt-1 w-full h-[1]" style={{ bgColor: RULE }} />
              </View>
            );
          }}
        </For>
      </View>

      <View class="absolute left-[24] right-[24] bottom-[24] flex-col gap-1">
        <For each={CAPTION}>
          {(line) => (
            <Text class="text-xs" style={{ textColor: MUTED }}>
              {line}
            </Text>
          )}
        </For>
      </View>
    </View>
  );
}

function firstLine(text: string): string {
  const line = text.split("\n", 1)[0] ?? "";
  return line.trim() || "(빈 응답)";
}

function clip(text: string, limit: number): string {
  return text.length <= limit ? text : `${text.slice(0, limit - 1)}…`;
}
