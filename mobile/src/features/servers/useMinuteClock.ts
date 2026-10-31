// The time, moved on once a minute: enough for "21:04", "Yesterday" and
// the weekdays to change while a list stays on screen.
import { useEffect, useState } from 'react';

const MINUTE_MS = 60_000;

export function useMinuteClock(): number {
  const [nowMs, setNowMs] = useState(() => Date.now());
  useEffect(() => {
    const timer = setInterval(() => setNowMs(Date.now()), MINUTE_MS);
    return () => clearInterval(timer);
  }, []);
  return nowMs;
}
