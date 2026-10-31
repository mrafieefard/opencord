import type { IconName } from './icons';

export interface ButtonProps {
  title: string;
  onPress: () => void;
  /** `plain` is text alone, for the quieter of two actions on a sheet. */
  variant: 'primary' | 'secondary' | 'plain' | 'voice' | 'danger';
  loading?: boolean;
  disabled?: boolean;
  icon?: IconName;
}
