// The player's top (spec §8.7): a chevron that closes it, where the music
// plays, and, while the user isn't listening there, Join in `voice`.
import { StyleSheet, View } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';

import { Icon, IconButton, Text, onHue, space, useTheme } from '@/ui';

import type { PlayerTopBarProps } from './PlayerTopBar.types';

export type { PlayerTopBarProps } from './PlayerTopBar.types';

const HEIGHT = 52;
const SIDE = 40;
const JOIN_HEIGHT = 32;

export function PlayerTopBar({ channelName, onJoin, onClose }: PlayerTopBarProps) {
  const { colors } = useTheme();
  return (
    <View style={styles.bar}>
      <View pointerEvents="none" style={styles.middle}>
        <Text variant="caption" color="textMuted">
          Playing in
        </Text>
        <Text variant="subhead" numberOfLines={1}>
          {channelName}
        </Text>
      </View>
      <IconButton icon="dismiss" label="Close" size={SIDE} onPress={onClose} />
      {onJoin ? (
        <Touchable
          onPress={onJoin}
          accessible
          accessibilityRole="button"
          accessibilityLabel={`Join ${channelName} to listen`}
          activeScale={0.94}
          style={[styles.join, { backgroundColor: colors.voice }]}
        >
          <Icon name="listen" size={16} tint={onHue} />
          <Text variant="subhead" style={{ color: onHue }}>
            Join
          </Text>
        </Touchable>
      ) : (
        <View style={styles.side} />
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  bar: {
    height: HEIGHT,
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    paddingHorizontal: space.lg,
  },
  middle: { ...StyleSheet.absoluteFill, alignItems: 'center', justifyContent: 'center', paddingHorizontal: 104 },
  side: { width: SIDE },
  join: {
    height: JOIN_HEIGHT,
    borderRadius: JOIN_HEIGHT / 2,
    paddingHorizontal: space.md,
    flexDirection: 'row',
    alignItems: 'center',
    gap: space.xs + 2,
  },
});
