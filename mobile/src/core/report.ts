// Promises nobody waits on end here: logged by kind, and shown to the
// person as a toast with describeError's sentence.
import { describeError, errorKind } from './errors';

type Shower = (text: string) => void;

let show: Shower = () => {};

/** The store's toast, set once at start. */
export function setErrorShower(shower: Shower): void {
  show = shower;
}

export function report(promise: Promise<unknown>, context: string): void {
  promise.catch((error: unknown) => {
    console.warn(`${context} failed: ${errorKind(error)}`);
    show(describeError(error));
  });
}
