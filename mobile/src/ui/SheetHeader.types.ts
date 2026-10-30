import type { ReactNode } from 'react';

export interface SheetHeaderProps {
  title: string;
  subtitle?: string;
  /** Drawn before the title, such as a person's avatar. */
  leading?: ReactNode;
  onClose: () => void;
}

export const GRABBER = { width: 36, height: 5 } as const;
