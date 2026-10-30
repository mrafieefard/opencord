import type { ReactNode } from 'react';
import type { StyleProp, ViewStyle } from 'react-native';

export interface GlassSurfaceProps {
  /** Its shape comes from here: a borderRadius makes a pill or a circle. */
  style?: StyleProp<ViewStyle>;
  /** Glass that answers a touch, for a button. */
  interactive?: boolean;
  testID?: string;
  children?: ReactNode;
}
