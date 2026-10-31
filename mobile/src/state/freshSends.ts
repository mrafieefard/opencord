// Sends made on this device a moment ago: their bubble rises into the list
// once, as Telegram's, whether it shows first as pending or arrives sent.
// Keyed by nonce, which the pending item and the sent message share, so the
// bubble doesn't rise again when the server's copy replaces the pending one.
const FRESH_MS = 2_000;
const fresh = new Map<string, number>();

export function markFreshSend(nonce: string, nowMs = Date.now()): void {
  fresh.set(nonce, nowMs);
}

/** Whether a nonce was sent in the last 2 s and its bubble hasn't risen yet. */
export function isFreshSend(nonce: string | undefined, nowMs = Date.now()): boolean {
  if (nonce === undefined) return false;
  const at = fresh.get(nonce);
  return at !== undefined && nowMs - at <= FRESH_MS;
}

/** Its bubble has risen: the server's copy, replacing the pending one, stays put. */
export function forgetFreshSend(nonce: string | undefined): void {
  if (nonce !== undefined) fresh.delete(nonce);
}
