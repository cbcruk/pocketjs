// The host's panel policy, as published for the guest to read.
//
// Tuning ghosting means running the same screen twice under different
// waveforms and comparing by eye. An unlabelled screen makes that worthless an
// hour later, so the host writes what it is using into the same contract slot
// as `__simHz` and `__bootClock` (publish in hosts/kobo/src/main.rs) and this
// reads it back.

export interface InkPolicy {
  /** Waveform used for a shallow, fast update: `"DU"` or `"A2"`. */
  readonly motionWaveform: string;
  /** Fast updates the host allows before forcing a full GC16 cleanup. */
  readonly ghostBudget: number;
  /** Panel presentation rate in Hz. */
  readonly presentHz: number;
}

/** What a host that published nothing looks like — a desktop render, say. */
export const UNKNOWN_POLICY: InkPolicy = {
  motionWaveform: "?",
  ghostBudget: 0,
  presentHz: 0,
};

/** Reads the host's published panel policy, or {@linkcode UNKNOWN_POLICY}. */
export function readInkPolicy(): InkPolicy {
  const published = (globalThis as { __inkPolicy?: Partial<InkPolicy> }).__inkPolicy;
  if (!published || typeof published.motionWaveform !== "string") return UNKNOWN_POLICY;
  return { ...UNKNOWN_POLICY, ...published } as InkPolicy;
}
