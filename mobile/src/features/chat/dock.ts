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
  /** The chat's height under its header, once laid out. */
  bodyHeight?: number;
  /** How far the dock has stepped aside for the sidebar: 0 in place, 1 away (AWAY_DROP lower). */
  away?: SharedValue<number>;
}

/** Stepping aside for the sidebar, the dock drops this far as it fades. */
export const AWAY_DROP = 24;

export const ChatDockContext = createContext<ChatDock>({ height: 0 });

export const useChatDock = (): ChatDock => useContext(ChatDockContext);

/** The window's bottom inset: the home indicator, or Android's navigation bar. */
function useWindowBottom(): number {
  const insets = useSafeAreaInsets();
  return initialWindowMetrics?.insets.bottom ?? insets.bottom;
}

/**
 * The dock's own bottom margin: stands on the system bottom inset (the home
 * indicator on iOS, or the navigation/gesture bar on Android), a little
 * lower (`space.xs`), or at least `space.sm`.
 */
export function useDockBottom(): number {
  const bottom = useWindowBottom();
  return Math.max(bottom - space.xs, space.sm);
}

/**
 * How far the keyboard lifts the dock and the messages: its height, less
 * the margin the dock already stands on, so the dock sits `space.sm` above it.
 */
export function useDockLift(): number {
  const dock = useDockBottom();
  return dock - space.sm;
}

const IS_ANDROID = Platform.OS === 'android';

/**
 * The messages' own lift, keyboard-controller's `offset`: the dock's, but on
 * Android a point less. At exactly the navigation bar's height, its chat
 * scroll view moves the list down when a Modal opens over it (a long-press
 * menu), hiding the newest message under the composer; a point less keeps
 * the list still, and is a point no one sees (measured on the emulator).
 */
export function useMessagesLift(): number {
  const lift = useDockLift();
  return IS_ANDROID ? Math.max(lift - 1, 0) : lift;
}
