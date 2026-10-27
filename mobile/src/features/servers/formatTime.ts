// Format time for server rows (spec §8.2):
// today 21:04, yesterday "Yesterday", within 6 days the weekday ("Mon"),
// else the short date, all in the device's locale.

export function formatTime(atMs: number, nowMs: number, locale: string): string {
  const atDate = new Date(atMs);
  const nowDate = new Date(nowMs);

  const atMidnight = new Date(atDate.getFullYear(), atDate.getMonth(), atDate.getDate());
  const nowMidnight = new Date(nowDate.getFullYear(), nowDate.getMonth(), nowDate.getDate());

  const diffDays = Math.round((nowMidnight.getTime() - atMidnight.getTime()) / (24 * 60 * 60 * 1000));

  if (diffDays === 0) {
    return new Intl.DateTimeFormat(locale, { timeStyle: 'short' }).format(atDate);
  }

  if (diffDays === 1) {
    return 'Yesterday';
  }

  if (diffDays > 1 && diffDays <= 6) {
    return new Intl.DateTimeFormat(locale, { weekday: 'short' }).format(atDate);
  }

  if (atDate.getFullYear() !== nowDate.getFullYear()) {
    return new Intl.DateTimeFormat(locale, { day: 'numeric', month: 'short', year: 'numeric' }).format(atDate);
  }
  return new Intl.DateTimeFormat(locale, { day: 'numeric', month: 'short' }).format(atDate);
}
