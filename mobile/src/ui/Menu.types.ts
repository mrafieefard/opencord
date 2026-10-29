import { createContext, useContext, type ReactNode, type Ref } from 'react';

import type { IconName } from './icons';

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
