export interface SliderProps {
  value: number;
  min: number;
  max: number;
  step?: number;
  /** While dragging. */
  onChange: (value: number) => void;
  /** When the finger lifts. */
  onCommit?: (value: number) => void;
  disabled?: boolean;
  accessibilityLabel: string;
}
