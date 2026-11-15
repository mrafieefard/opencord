import type { ViewStyle } from 'react-native';

export interface AreaEvent {
  areaId: string;
  /** A JSON string: an input for the core, or the area's status. */
  json: string;
}

export interface HostCallEvent {
  areaId: string;
  id: number;
  method: string;
  json: string;
}

export interface Subscription {
  remove(): void;
}

export interface PluginAreaViewProps {
  areaId: string;
  style?: ViewStyle;
}
