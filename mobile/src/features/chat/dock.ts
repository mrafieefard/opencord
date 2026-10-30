// The chat's dock: the voice bar and the composer, floating over the
// bottom of the messages as Telegram's. The list scrolls under it, so it
// leaves the dock's height free below its last message, and the keyboard
// lifts both by the same amount.
import { createContext, useContext } from 'react';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import type { SharedValue } from 'react-native-reanimated';

import { space } from '@/ui';

export interface ChatDock {
  /** The dock's height, for the list's scroll range (keyboard-controller's extraContentPadding). */
  padding?: SharedValue<number>;
  /** The same, for what floats above it (Jump to latest, the typing line). */
  height: number;
}

export const ChatDockContext = createContext<ChatDock>({ height: 0 });

export const useChatDock = (): ChatDock => useContext(ChatDockContext);

/** The dock's own bottom margin: on the home indicator's inset, a little lower. */
export function useDockBottom(): number {
  const insets = useSafeAreaInsets();
  return Math.max(insets.bottom - space.xs, space.sm);
}

/**
 * How far the keyboard lifts the dock and the messages: its height, less
 * the margin the dock already stands on, so the dock sits `space.sm` above it.
 */
export function useDockLift(): number {
  return useDockBottom() - space.sm;
}
