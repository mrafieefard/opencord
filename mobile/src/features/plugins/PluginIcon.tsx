// A plugin's icon from its assets (a channel type's or an activity's),
// fetched through the core; the `extension` icon when it has none or the
// file can't be had.
import { useEffect, useState } from 'react';
import { Image, View } from 'react-native';

import { getCore } from '@/core';
import { errorKind } from '@/core/errors';
import { Icon } from '@/ui';

export interface PluginIconProps {
  serverKey: string;
  pluginId: string;
  /** A path under the plugin's assets/, or empty. */
  icon: string;
  size: number;
  radius?: number;
  testID?: string;
}

function useAssetFile(serverKey: string, pluginId: string, icon: string): string | null {
  const [file, setFile] = useState<{ key: string; path: string | null } | null>(null);
  const key = `${serverKey}/${pluginId}/${icon}`;
  useEffect(() => {
    if (icon === '' || pluginId === '') return undefined;
    let live = true;
    getCore()
      .pluginAssetFile(serverKey, pluginId, icon)
      .then(
        (path) => live && setFile({ key, path }),
        (error: unknown) => {
          console.warn(`plugin icon failed: ${errorKind(error)}`);
          if (live) setFile({ key, path: null });
        },
      );
    return () => {
      live = false;
    };
  }, [key, serverKey, pluginId, icon]);
  return file?.key === key ? file.path : null;
}

export function PluginIcon({ serverKey, pluginId, icon, size, radius = 4, testID }: PluginIconProps) {
  const file = useAssetFile(serverKey, pluginId, icon);
  if (file) {
    return <Image testID={testID} source={{ uri: `file://${file}` }} style={{ width: size, height: size, borderRadius: radius }} />;
  }
  return (
    <View style={{ width: size, height: size, alignItems: 'center', justifyContent: 'center' }}>
      <Icon name="extension" size={Math.round(size * 0.9)} color="textSecondary" />
    </View>
  );
}
