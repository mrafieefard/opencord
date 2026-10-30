// The call: a voice channel's page, grown out of the glass voice bar itself
// as Apple Maps' card grows: pulled up, the bar's own glass stretches with
// the finger until it fills the screen, and dragged down it shrinks back
// into the bar. One shared position drives it, so the bars' gestures, the
// sheet's own and a tap all move the same glass: 0 is the bar, 1 is open.
// `callOrigin` is where the bar was when it was last touched.
import { useWindowDimensions } from 'react-native';
import { makeMutable, withSpring, type WithSpringConfig } from 'react-native-reanimated';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { useAppStore } from '@/state/store';

export const callProgress = makeMutable(0);

/** A frame on the screen, in points. */
export interface CallFrame {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** The bar the call grows out of (and shrinks back into); 0 wide until one is touched. */
export const callOrigin = makeMutable<CallFrame>({ x: 0, y: 0, width: 0, height: 0 });

/** The voice bars' pill: 56 pt high, its row inset 16 pt left and 4 pt right. */
export const BAR_HEIGHT = 56;
const BAR_PAD_LEFT = 16;
const BAR_PAD_RIGHT = 4;

/** A voice bar's glass around its row: the row's frame, grown by the pill's own padding. */
export function barFrame(row: CallFrame): CallFrame {
  'worklet';
  const height = Math.max(row.height, BAR_HEIGHT);
  return {
    x: row.x - BAR_PAD_LEFT,
    y: row.y - (height - row.height) / 2,
    width: row.width + BAR_PAD_LEFT + BAR_PAD_RIGHT,
    height,
  };
}

export const CALL_SPRING: WithSpringConfig = { damping: 28, stiffness: 280, mass: 0.9 };

/** How far below the top of the screen the open sheet stops. */
export const CALL_TOP_GAP = 8;

/** Where the open call's top edge is. */
export function useCallTop(): number {
  return useSafeAreaInsets().top + CALL_TOP_GAP;
}

/**
 * The bar to grow from when none was touched (opened from the channel
 * list): where the voice bar stands above the tab bar.
 */
export function useFallbackOrigin(): CallFrame {
  const { width, height } = useWindowDimensions();
  const insets = useSafeAreaInsets();
  const side = 16;
  return { x: side, y: height - insets.bottom - 64 - BAR_HEIGHT, width: width - side * 2, height: BAR_HEIGHT };
}

/** How far the finger travels from the bar to the open call's top: never less than a third of the screen. */
export function callTravel(origin: CallFrame, top: number, screen: number): number {
  'worklet';
  return Math.max(origin.y - top, screen / 3);
}

/** Past this share of the way, or flicked, a drag carries on to the end. */
export const CALL_SETTLE = 0.25;
export const CALL_FLICK = 600;

/** Which call the sheet shows; it doesn't move it. */
export function showCall(serverKey: string, channelId: string): void {
  const held = useAppStore.getState().call;
  if (held?.serverKey === serverKey && held.channelId === channelId) return;
  useAppStore.setState({ call: { serverKey, channelId } });
}

/** Opens the call, springing up as if pulled. */
export function openCall(serverKey: string, channelId: string): void {
  showCall(serverKey, channelId);
  callProgress.set(withSpring(1, CALL_SPRING));
}

export function closeCall(): void {
  callProgress.set(withSpring(0, CALL_SPRING));
}
