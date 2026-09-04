// Verifies that Hangul survives the whole text path: AST literal collection ->
// atlas bake -> Gray8 raster. The default Inter fonts have no Hangul cmap, so
// `pocket compile` alone renders tofu here; the atlas needs a Korean face:
//
//   bun tools/pocket.ts compile --target kobo-glo \
//     --manifest apps/hangul-probe/pocket.json --project-root .
//   bun tools/build.ts --plan=.pocket/kobo-glo/plan.json --project-root=. \
//     --outdir=dist --hz=60 \
//     --font-regular=<NanumGothic-Regular.ttf> --font-bold=<NanumGothic-Bold.ttf>
//
// The second step is separate because pocket.ts does not forward the font
// flags; it exists to write the plan the compiler reads.
import { For } from "solid-js";
import { Text, View } from "@pocketjs/framework/components";

const FORECAST = [
  { day: "오늘", sky: "맑음", high: 27, low: 18 },
  { day: "내일", sky: "구름 조금", high: 25, low: 17 },
  { day: "모레", sky: "소나기", high: 22, low: 16 },
];

export default function HangulProbe() {
  return (
    <View class="relative w-full h-full overflow-hidden" style={{ bgColor: "#f4f1e8" }}>
      <View class="absolute left-[18] top-[18] right-[18] flex-col gap-1">
        <Text class="text-xl font-bold" style={{ textColor: "#161616" }}>
          서울 날씨
        </Text>
        <Text class="text-xs tracking-wide" style={{ textColor: "#66625b" }}>
          한글 글리프 베이킹 검증 — ASCII mixed 0123456789
        </Text>
        <View class="mt-2 w-full h-[1]" style={{ bgColor: "#b8b2a7" }} />
      </View>

      <View class="absolute left-[18] right-[18] top-[112] flex-col gap-3">
        <For each={FORECAST}>
          {(entry) => (
            <View
              class="flex-row justify-between items-center px-3 py-2 border-[1]"
              style={{ bgColor: "#ebe7dd", borderColor: "#b8b2a7" }}
            >
              <Text class="text-base font-bold" style={{ textColor: "#2c2a27" }}>
                {entry.day}
              </Text>
              <Text class="text-base" style={{ textColor: "#44403a" }}>
                {entry.sky}
              </Text>
              <Text class="text-base" style={{ textColor: "#161616" }}>
                {entry.high}도 / {entry.low}도
              </Text>
            </View>
          )}
        </For>
      </View>

      <View class="absolute left-[18] right-[18] top-[300] flex-col gap-2">
        <Text class="text-base" style={{ textColor: "#161616" }}>
          16px 본문: 미세먼지 보통, 강수 확률 20퍼센트
        </Text>
        <Text class="text-xs" style={{ textColor: "#66625b" }}>
          12px 캡션: 습도와 바람은 오후에 다시 갱신됩니다
        </Text>
        <Text class="text-xs font-bold" style={{ textColor: "#2c2a27" }}>
          12px 볼드: 연결 실패 시 마지막 값을 표시
        </Text>
      </View>

      <View
        class="absolute left-[18] right-[18] bottom-[18] flex-row justify-between items-center px-3 py-2 border-[1]"
        style={{ bgColor: "#ebe7dd", borderColor: "#b8b2a7" }}
      >
        <Text class="text-xs font-bold" style={{ textColor: "#2c2a27" }}>
          한글 베이크
        </Text>
        <Text class="text-xs" style={{ textColor: "#66625b" }}>
          NANUM GOTHIC
        </Text>
      </View>
    </View>
  );
}
