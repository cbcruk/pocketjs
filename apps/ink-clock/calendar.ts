// Civil-date arithmetic for the ink clock.
//
// The runtime has no wall clock and no Date to lean on: the host publishes one
// broken-down local time at boot and everything after is virtual seconds. That
// makes "what day is it now" a matter of adding whole days to a date by hand,
// which is all this module does.

export interface BootClock {
  readonly year: number;
  /** 1..12. */
  readonly month: number;
  /** 1..31. */
  readonly day: number;
  /** 0 = Sunday, matching struct tm's tm_wday. */
  readonly weekday: number;
  /** Seconds since local midnight at boot. */
  readonly secondOfDay: number;
}

export const SECONDS_PER_DAY = 86_400;

/** Sunday-first, matching {@linkcode BootClock.weekday}. */
export const WEEKDAY_NAMES = ["일", "월", "화", "수", "목", "금", "토"] as const;

/** Placeholder for a host that published no clock — a headless render, say. */
export const UNKNOWN_CLOCK: BootClock = {
  year: 1970,
  month: 1,
  day: 1,
  weekday: 4,
  secondOfDay: 0,
};

export function readBootClock(): BootClock {
  const published = (globalThis as { __bootClock?: Partial<BootClock> }).__bootClock;
  if (!published || typeof published.year !== "number") return UNKNOWN_CLOCK;
  return { ...UNKNOWN_CLOCK, ...published } as BootClock;
}

function isLeapYear(year: number): boolean {
  return (year % 4 === 0 && year % 100 !== 0) || year % 400 === 0;
}

export function daysInMonth(year: number, month: number): number {
  if (month === 2) return isLeapYear(year) ? 29 : 28;
  return month === 4 || month === 6 || month === 9 || month === 11 ? 30 : 31;
}

/**
 * Advance a date by whole days.
 *
 * @param days Days to add. Zero for all but one frame of any given day, so the
 * loop below is not the hot path it looks like.
 */
export function addDays(
  clock: BootClock,
  days: number,
): { year: number; month: number; day: number; weekday: number } {
  let { year, month, day } = clock;
  for (let step = 0; step < days; step++) {
    day += 1;
    if (day > daysInMonth(year, month)) {
      day = 1;
      month += 1;
      if (month > 12) {
        month = 1;
        year += 1;
      }
    }
  }
  return { year, month, day, weekday: (clock.weekday + days) % 7 };
}

export function pad2(value: number): string {
  return value < 10 ? `0${value}` : `${value}`;
}
