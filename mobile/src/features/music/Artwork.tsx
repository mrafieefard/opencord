// A track's artwork (spec §8.7): the core's file, or, without one or when
// it fails to load, a note on a shade picked by the title, so rows without
// artwork line up with those that have it.
import { Image } from 'expo-image';
import { useEffect, useState } from 'react';
import { StyleSheet, View, type StyleProp, type ViewStyle } from 'react-native';

import { getCore } from '@/core';
import { fileUri } from '@/core/paths';
import { Icon, nameHash, useTheme, withAlpha } from '@/ui';

const MAX_CACHE_ENTRIES = 200;
const cache = new Map<string, string>();

/** The sizes the server makes (the core's `artwork::SIZES`): thumbnails and the cover. */
export const THUMBNAIL = 128;
export const COVER = 512;
const LIST_MAX = 64;

/** The artwork's file URI at a size the server makes, once the core has it; null without one. */
export function useArtwork(serverKey: string, artworkId: string | undefined, size: typeof THUMBNAIL | typeof COVER): string | null {
  const key = artworkId ? `${serverKey}:${artworkId}:${size}` : null;
  const [loaded, setLoaded] = useState<{ key: string; uri: string } | null>(null);
  const cached = key ? (cache.get(key) ?? null) : null;

  useEffect(() => {
    if (!key || !artworkId || cache.has(key)) return;
    let active = true;
    getCore()
      .musicArtwork(serverKey, artworkId, size)
      .then((path) => {
        const uri = fileUri(path);
        if (cache.size >= MAX_CACHE_ENTRIES) {
          const oldest = cache.keys().next().value;
          if (oldest !== undefined) cache.delete(oldest);
        }
        cache.set(key, uri);
        if (active) setLoaded({ key, uri });
      })
      .catch(() => {
        // The placeholder stays.
      });
    return () => {
      active = false;
    };
  }, [serverKey, artworkId, key, size]);

  return cached ?? (loaded?.key === key ? loaded.uri : null);
}

export interface ArtworkProps {
  serverKey: string;
  artworkId?: string;
  /** Picks the placeholder's shade. */
  title: string;
  size: number;
  radius: number;
  style?: StyleProp<ViewStyle>;
}

export function Artwork({ serverKey, artworkId, title, size, radius, style }: ArtworkProps) {
  const { colors } = useTheme();
  // A list's picture (48 pt) is about the thumbnail at 3x; anything bigger is the cover.
  const uri = useArtwork(serverKey, artworkId || undefined, size > LIST_MAX ? COVER : THUMBNAIL);
  const [failed, setFailed] = useState<string | null>(null);
  const shade = colors.avatarShades[nameHash(title) % colors.avatarShades.length];
  const showImage = uri !== null && failed !== uri;

  return (
    <View
      style={[
        styles.box,
        { width: size, height: size, borderRadius: radius, backgroundColor: shade, borderColor: withAlpha(colors.text, 0.08) },
        style,
      ]}
    >
      {showImage ? (
        <Image
          source={{ uri }}
          style={StyleSheet.absoluteFill}
          contentFit="cover"
          transition={200}
          accessibilityIgnoresInvertColors
          onError={() => setFailed(uri)}
        />
      ) : (
        <Icon name="music" size={Math.round(size * 0.3)} color="textMuted" />
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  box: { alignItems: 'center', justifyContent: 'center', overflow: 'hidden', borderWidth: StyleSheet.hairlineWidth },
});
