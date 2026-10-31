// The chat's dock: the voice bar and the composer, floating over the
// bottom of the messages as Telegram's. The list scrolls under it, so it
// leaves the dock's height free below its last message, and the keyboard
// lifts both by the same amount. A field grown past one line rises over
// the messages from a one-line slot, so the dock's size never changes
// while typing and the messages never move, as Telegram's.
import { createContext, useContext } from 'react';
import { Platform } from 'react-native';
import { initialWindowMetrics, useSafeAreaInsets } from 'react-native-safe-area-context';
import type { SharedValue } from 'react-native-reanimated';

import { space } from '@/ui';

export interface ChatDock {
  /** The dock's height, for the list's scroll range (keyboard-controller's extraContentPadding). */
  padding?: SharedValue<number>;
  /** How far the field has grown above its one-line slot: what sits above it rises with it. */
  fieldGrowth?: SharedValue<number>;
  /** The same, for what floats above it (Jump to latest, the typing line). */
  height: number;
  /** How far the dock has stepped aside for the sidebar: 0 in place, 1 away (AWAY_DROP lower). */
  away?: SharedValue<number>;
}

/** Stepping aside for the sidebar, the dock drops this far as it fades. */
export const AWAY_DROP = 24;

export const ChatDockContext = createContext<ChatDock>({ height: 0 });

export const useChatDock = (): ChatDock => useContext(ChatDockContext);

/**
 * Android's screens end above the system's navigation bar (the tabs keep
 * clear of it), though the window's inset still counts it; iOS's reach the
 * screen's bottom edge, under the home indicator.
 */
const SCREEN_ABOVE_NAV_BAR = Platform.OS === 'android';

/** The window's bottom inset: the home indicator, or Android's navigation bar. */
function useWindowBottom(): number {
  const insets = useSafeAreaInsets();
  return initialWindowMetrics?.insets.bottom ?? insets.bottom;
}

/**
 * The dock's own bottom margin: on the home indicator's inset, a little
 * lower. The window's inset, not the screen's: inside the tabs the screen's
 * grows by the tab bar whenever it shows (the sidebar open over a chat),
 * and the dock, with the messages it frames, must not move then. On
 * Android, just above the navigation bar the screen already ends on.
 */
export function useDockBottom(): number {
  const bottom = useWindowBottom();
  return SCREEN_ABOVE_NAV_BAR ? space.sm : Math.max(bottom - space.xs, space.sm);
}

/**
 * How far the keyboard lifts the dock and the messages: its height, less
 * the margin the dock already stands on, so the dock sits `space.sm` above
 * it. Android's keyboard height counts the navigation bar, which the
 * screen already ends above: less that too.
 */
export function useDockLift(): number {
  const bottom = useWindowBottom();
  const dock = useDockBottom();
  return dock - space.sm + (SCREEN_ABOVE_NAV_BAR ? bottom : 0);
}
