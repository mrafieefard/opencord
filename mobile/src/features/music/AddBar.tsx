// Adding music (spec §8.7): what looks like a search field opens search;
// the button beside it uploads audio files from the phone. Dimmed when the
// user may not upload, and a tap says why.
import { StyleSheet, View } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';

import { Icon, Text, radius, space, useTheme } from '@/ui';

import { GUTTER } from './TrackRow';
import { useUploadPicker } from './Uploads';

export interface AddBarProps {
  serverKey: string;
  channelId: string;
  onSearch: () => void;
}

const HEIGHT = 44;

export function AddBar({ serverKey, channelId, onSearch }: AddBarProps) {
  const { colors } = useTheme();
  const upload = useUploadPicker(serverKey, channelId);
  return (
    <View style={styles.row}>
      <Touchable
        onPress={onSearch}
        accessible
        accessibilityRole="search"
        accessibilityLabel="Search songs or paste a link"
        underlayColor={colors.selected}
        activeUnderlayOpacity={1}
        style={[styles.field, { backgroundColor: colors.selected }]}
      >
        <Icon name="search" size={16} color="textMuted" />
        <Text variant="body" color="textMuted" numberOfLines={1}>
          Search songs or paste a link
        </Text>
      </Touchable>
      <Touchable
        onPress={upload.pick}
        accessible
        accessibilityRole="button"
        accessibilityLabel="Upload audio files"
        accessibilityHint={upload.missing ?? 'MP3, FLAC, M4A, AAC, OGG or WAV'}
        activeScale={0.92}
        style={[styles.upload, { backgroundColor: colors.selected }, upload.missing !== null && styles.dimmed]}
      >
        <Icon name="upload" size={20} color="text" />
      </Touchable>
    </View>
  );
}

const styles = StyleSheet.create({
  row: { flexDirection: 'row', alignItems: 'center', gap: space.sm, paddingHorizontal: GUTTER, paddingTop: space.xxl },
  field: {
    flex: 1,
    height: HEIGHT,
    borderRadius: radius.icon,
    flexDirection: 'row',
    alignItems: 'center',
    gap: space.sm,
    paddingHorizontal: space.md,
  },
  upload: { width: HEIGHT, height: HEIGHT, borderRadius: HEIGHT / 2, alignItems: 'center', justifyContent: 'center' },
  dimmed: { opacity: 0.4 },
});
