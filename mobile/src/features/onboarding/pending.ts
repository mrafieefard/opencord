// A secret read from a backup, kept in memory between the import and the
// name step. Never in a route parameter.
let pending: ArrayBuffer | null = null;

export function holdSecret(secret: ArrayBuffer | null): void {
  pending = secret;
}

export function takeSecret(): ArrayBuffer | null {
  const secret = pending;
  pending = null;
  return secret;
}

export function peekSecret(): ArrayBuffer | null {
  return pending;
}
