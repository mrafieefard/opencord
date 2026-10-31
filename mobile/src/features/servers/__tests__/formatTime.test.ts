import { formatTime } from '../formatTime';

function norm(s: string): string {
  return s.replace(/\s+/g, ' ').trim();
}

// Monday 26 October 2026, 21:30 local time.
const nowMs = new Date(2026, 9, 26, 21, 30).getTime();

describe('formatTime', () => {
  it('times read as today, yesterday, a weekday, or a date', () => {

    // 21:04 today
    const todayMs = new Date(2026, 9, 26, 21, 4).getTime();
    // yesterday
    const yesterdayMs = new Date(2026, 9, 25, 14, 0).getTime();
    // Thursday 22 October as "Thu" (26 October 2026 is itself a Monday)
    const thursdayMs = new Date(2026, 9, 22, 10, 0).getTime();
    // 3 October ("3 Oct" in en-GB, "Oct 3" in en-US)
    const oct3Ms = new Date(2026, 9, 3, 10, 0).getTime();

    // en-GB
    expect(norm(formatTime(todayMs, nowMs, 'en-GB'))).toBe('21:04');
    expect(norm(formatTime(yesterdayMs, nowMs, 'en-GB'))).toBe('Yesterday');
    expect(norm(formatTime(thursdayMs, nowMs, 'en-GB'))).toBe('Thu');
    expect(norm(formatTime(oct3Ms, nowMs, 'en-GB'))).toBe('3 Oct');

    // en-US
    expect(norm(formatTime(todayMs, nowMs, 'en-US'))).toBe('9:04 PM');
    expect(norm(formatTime(yesterdayMs, nowMs, 'en-US'))).toBe('Yesterday');
    expect(norm(formatTime(thursdayMs, nowMs, 'en-US'))).toBe('Thu');
    expect(norm(formatTime(oct3Ms, nowMs, 'en-US'))).toBe('Oct 3');
  });

  it('a time a little ahead of the clock reads as today, and another year says the year', () => {
    expect(norm(formatTime(nowMs + 60_000, nowMs, 'en-GB'))).toBe('21:31');
    expect(norm(formatTime(new Date(2025, 11, 30, 12, 0).getTime(), nowMs, 'en-GB'))).toBe('30 Dec 2025');
  });
});
