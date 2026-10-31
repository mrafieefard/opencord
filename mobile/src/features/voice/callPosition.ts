// The call: a voice channel's page, grown out of the glass voice bar itself
// as Apple Maps' card grows: pulled up, the bar's own glass stretches with
// the finger until it fills the screen, and dragged down it shrinks back
// into the bar. One shared position drives it, so the bars' gestures, the
// sheet's own and a tap all move the same glass: 0 is the bar, 1 is open.
// `callOrigin` is where the bar was when it was last touched.
import { useWindowDimensions } from 'react-native';
import { Gesture } from 'react-native-gesture-handler';
import { makeMutable, type SharedValue, withSpring, type WithSpringConfig } from 'react-native-reanimated';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { chrome } from '@/features/servers/chrome';
import { useAppStore } from '@/state/store';

export const callProgress = makeMutable(0);

/**
 * How far a chat's field has grown over its messages (iOS): the floating
 * voice pill resting above the composer rises with it (ChatVoicePill.ios).
 */
export const slotRise = makeMutable(0);

/**
 * How far a phone's sidebar is out over a chat: 0 with the chat in front,
 * 1 with the list out (PhonePanes). In a chat, the floating pill moves with
 * it between its room above the composer and its place above the tab bar,
 * as the sidebar is dragged.
 */
export const sidebarOut = makeMutable(0);

/** A frame on the screen, in points. */
export interface CallFrame {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** The bar the call grows out of (and shrinks back into); 0 wide until one is touched. */
export const callOrigin = makeMutable<CallFrame>({ x: 0, y: 0, width: 0, height: 0 });

/** The voice bars' pill: 56 pt high (the tab bar's own is the system's), its row inset 16 pt left and 4 pt right. */
export const BAR_HEIGHT = 56;
const BAR_PAD_LEFT = 16;
const BAR_PAD_RIGHT = 4;

/**
 * A voice bar's glass around its row: the row fills the pill's height, so
 * only its sides grow by the pill's padding. The glass that grows into the
 * call starts exactly there, as the bar it replaces.
 */
export function barFrame(row: CallFrame): CallFrame {
  'worklet';
  return {
    x: row.x - BAR_PAD_LEFT,
    y: row.y,
    width: row.width + BAR_PAD_LEFT + BAR_PAD_RIGHT,
    height: row.height,
  };
}

/** The floating voice pill (iOS) stands this far above the tab bar. */
export const PILL_GAP = 8;

/**
 * What a tab's list leaves free at its end for the floating voice pill
 * (iOS), while in voice, so its last rows can scroll out from under it as
 * from under the tab bar.
 */
export function useVoicePillRoom(): number {
  const inVoice = useAppStore((state) => state.myVoice !== null);
  return chrome.floatingVoice && inVoice ? BAR_HEIGHT + PILL_GAP : 0;
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

/**
 * A downward drag on the open call: it shrinks back towards the bar with
 * the finger, springing away or back on letting go. Given the scroll of
 * what it is dragged on, it lets that scroll until it is at its top and
 * takes over from there, as Apple Maps' card does.
 */
export function callDrag(top: number, screen: number, scrollY?: SharedValue<number>) {
  // How far the finger had come when the call began to follow it; NaN until then.
  const from = makeMutable(Number.NaN);
  return Gesture.Pan()
    .activeOffsetY(8)
    .failOffsetX([-16, 16])
    .onStart(() => {
      from.set(scrollY ? Number.NaN : 0);
    })
    .onUpdate((event) => {
      if (Number.isNaN(from.get())) {
        if (!scrollY || scrollY.get() > 0.5 || event.translationY <= 0) return;
        from.set(event.translationY);
      }
      const travel = callTravel(callOrigin.get(), top, screen);
      const moved = event.translationY - from.get();
      callProgress.set(Math.min(Math.max(1 - moved / travel, 0), 1));
    })
    .onEnd((event) => {
      if (Number.isNaN(from.get())) return;
      const travel = callTravel(callOrigin.get(), top, screen);
      const moved = event.translationY - from.get();
      const away = moved > travel * CALL_SETTLE || (moved > 0 && event.velocityY > CALL_FLICK);
      callProgress.set(withSpring(away ? 0 : 1, { ...CALL_SPRING, velocity: -event.velocityY / travel }));
    });
}

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
