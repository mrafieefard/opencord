import type { IconName } from '../icons';

/** A row that opens another page; its value shows what is set there. */
export interface LinkRowProps {
  kind: 'link';
  label: string;
  value?: string;
  onPress: () => void;
  icon?: IconName;
}

export interface SwitchRowProps {
  kind: 'switch';
  label: string;
  /** A line under the label saying what the switch does. */
  detail?: string;
  value: boolean;
  onChange: (value: boolean) => void;
  disabled?: boolean;
}

export interface SliderRowProps {
  kind: 'slider';
  label: string;
  value: number;
  min: number;
  max: number;
  /** What values round to; a hundredth of the range when not given. */
  step?: number;
  /** The value as the row shows it and a screen reader reads it. */
  format: (value: number) => string;
  /** While the finger moves. */
  onChange: (value: number) => void;
  /** When the finger lifts. */
  onDone: (value: number) => void;
}

export interface ChoiceOption {
  key: string;
  label: string;
}

/** One of a few options, picked from the app's `Menu`. */
export interface ChoiceRowProps {
  kind: 'choice';
  label: string;
  /** The chosen option's key. */
  value: string;
  options: ChoiceOption[];
  onChange: (key: string) => void;
}

export interface TextRowProps {
  kind: 'text';
  label: string;
  value: string;
  placeholder?: string;
  maxLength: number;
  onChange: (value: string) => void;
  multiline?: boolean;
}

export interface DestructiveRowProps {
  kind: 'destructive';
  label: string;
  onPress: () => void;
}

export type SettingsRowProps =
  | LinkRowProps
  | SwitchRowProps
  | SliderRowProps
  | ChoiceRowProps
  | TextRowProps
  | DestructiveRowProps;

/** A slider's step when the row gives none. */
export function sliderStep(props: SliderRowProps): number {
  return props.step ?? (props.max - props.min) / 100;
}

/** The words of the chosen option; the key itself if no option has it. */
export function choiceLabel(props: ChoiceRowProps): string {
  return props.options.find((option) => option.key === props.value)?.label ?? props.value;
}

/** The menu's testID, by the row's label. */
export function choiceMenuId(label: string): string {
  return `settings-choice-${label}`;
}
