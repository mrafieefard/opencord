// The theme: the person's choice (usePrefs), or the system's. Native parts
// follow it too, through Appearance.
import { createContext, useEffect, useMemo, type ReactNode } from 'react';
import { Appearance, useColorScheme } from 'react-native';

import { usePrefs } from '@/state/selectors';

import { tokens, type ColorTokens, type Scheme } from './tokens';

export interface Theme {
  colors: ColorTokens;
  scheme: Scheme;
}

export const ThemeContext = createContext<Theme>({ colors: tokens.dark, scheme: 'dark' });

export function ThemeProvider({ children }: { children: ReactNode }) {
  const { theme } = usePrefs();
  const system = useColorScheme();
  useEffect(() => {
    Appearance.setColorScheme(theme === 'system' ? 'unspecified' : theme);
  }, [theme]);
  const scheme: Scheme = theme === 'system' ? (system === 'light' ? 'light' : 'dark') : theme;
  const value = useMemo(() => ({ colors: tokens[scheme], scheme }), [scheme]);
  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}
