// The chat's dock: the voice bar and the composer, floating over the
// bottom of the messages as Telegram's. The list scrolls under it, so it
// leaves the dock's height free below its last message, and the keyboard
// lifts both by the same amount. A field grown past one line rises over
// the messages from a one-line slot, so the dock's size never changes
// while typing and the messages never move, as Telegram's.
import { createContext, useContext } from 'react';
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
}

export const ChatDockContext = createContext<ChatDock>({ height: 0 });

export const useChatDock = (): ChatDock => useContext(ChatDockContext);

/**
 * The dock's own bottom margin: on the home indicator's inset, a little
 * lower. The window's inset, not the screen's: inside the tabs the screen's
 * grows by the tab bar whenever it shows (the sidebar open over a chat),
 * and the dock, with the messages it frames, must not move then.
 */
export function useDockBottom(): number {
  const insets = useSafeAreaInsets();
  const bottom = initialWindowMetrics?.insets.bottom ?? insets.bottom;
  return Math.max(bottom - space.xs, space.sm);
}

/**
 * How far the keyboard lifts the dock and the messages: its height, less
 * the margin the dock already stands on, so the dock sits `space.sm` above it.
 */
export function useDockLift(): number {
  return useDockBottom() - space.sm;
}
