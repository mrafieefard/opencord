import { requireNativeView, requireOptionalNativeModule } from 'expo';
import type { ComponentType } from 'react';

import type { PluginAreaViewProps } from './PluginArea.types';

// Only looked up when the module is in the build: a missing view manager
// throws.
const NativeView: ComponentType<PluginAreaViewProps> | null =
  requireOptionalNativeModule('PluginArea') != null ? requireNativeView('PluginArea') : null;

/** Shows an area while mounted; its engine and state outlive the view. */
export function PluginAreaView(props: PluginAreaViewProps) {
  return NativeView ? <NativeView {...props} /> : null;
}
