import type { ComponentRef, ReactNode, Ref } from 'react';
import type { StyleProp, View, ViewStyle } from 'react-native';

export interface GlassSurfaceProps {
  /** Its shape comes from here: a borderRadius makes a pill or a circle. */
  style?: StyleProp<ViewStyle>;
  /** Glass that answers a touch, for a button. */
  interactive?: boolean;
  testID?: string;
  /** `none` for glass that's only a backdrop: on Android a raised surface is
   * handed touches before the views laid over it. */
  pointerEvents?: 'auto' | 'none' | 'box-none';
  children?: ReactNode;
  /** The surface's own view: Reanimated animates its corners through it. */
  ref?: Ref<ComponentRef<typeof View>>;
}
