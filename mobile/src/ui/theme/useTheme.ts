import { useContext } from 'react';

import { ThemeContext, type Theme } from './ThemeProvider';

export function useTheme(): Theme {
  return useContext(ThemeContext);
}
