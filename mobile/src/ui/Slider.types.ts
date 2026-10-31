export interface SliderProps {
  value: number;
  min: number;
  max: number;
  step?: number;
  /** Accent color variant: 'text' (default) or 'voice'. */
  accent?: 'text' | 'voice';
  /** While dragging. */
  onChange: (value: number) => void;
  /** When the finger lifts. */
  onCommit?: (value: number) => void;
  disabled?: boolean;
  accessibilityLabel: string;
}
