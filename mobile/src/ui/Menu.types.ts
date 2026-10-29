import { createContext, useContext, type ReactNode, type Ref } from 'react';

import type { IconName } from './icons';
import type { ColorToken } from './theme/tokens';

export interface MenuAction {
  id: string;
  title: string;
  icon?: IconName;
  destructive?: boolean;
  disabled?: boolean;
}

export interface MenuProps {
  actions: MenuAction[];
  onAction: (id: string) => void;
  trigger?: 'press' | 'longPress';
  /**
   * How long-pressed content is sized on iOS, where it is hosted on its own:
   * a row fills the width it is given; a bubble keeps the width of its
   * content.
   */
  fit?: 'fill' | 'content';
  /**
   * The surface the content sits on. iOS lifts long-pressed content out
   * while its menu is open and shows this where it was, not what lies
   * under the content (a row's swipe actions).
   */
  background?: ColorToken;
  testID?: string;
  ref?: Ref<MenuHandle>;
  children: ReactNode;
}

export interface MenuHandle {
  open(): void;
}

/**
 * Opens the menu around a view from inside it, for a child with its own
 * touch handling (a picture in a bubble) that a long press must still reach
 * the menu through. Nothing on iOS, whose menu takes the press itself.
 */
export const MenuOpener = createContext<() => void>(() => {});

export function useMenuOpener(): () => void {
  return useContext(MenuOpener);
}
