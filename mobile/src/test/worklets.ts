// react-native-worklets in Jest: JavaScript runs where it is.
export function scheduleOnRN<A extends unknown[]>(fn: (...args: A) => void, ...args: A): void {
  fn(...args);
}

export function scheduleOnUI<A extends unknown[]>(fn: (...args: A) => void, ...args: A): void {
  fn(...args);
}

export function runOnUISync<A extends unknown[], R>(fn: (...args: A) => R, ...args: A): R {
  return fn(...args);
}

export function isWorkletRuntime(): boolean {
  return false;
}
