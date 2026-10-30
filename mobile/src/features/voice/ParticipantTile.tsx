// A person in a voice channel (spec §8.6): a surface square, 16 pt corners,
// a 64 pt avatar with a 3 pt voice ring while speaking, name and glyphs.
// While a sound they played plays, a dashed ring sits outside the voice
// ring (Phase 2 plan §11.7, as the desktop's soundboard_ring.dart).
import { StyleSheet, View } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';

import { useImageFile } from '@/features/servers/useImageFile';
import { useSoundRing } from '@/state/selectors';
import type { VoiceMember } from '@/state/types';
import { Avatar, Icon, radius, space, Text, useTheme } from '@/ui';

export interface ParticipantTileProps {
  serverKey: string;
  channelId: string;
  member: VoiceMember;
  size: number;
  onPress?: () => void;
}

const AVATAR = 64;
/** The dashed ring: 2 pt, 2 pt outside the voice ring. */
const SOUND_RING = 2;
const SOUND_GAP = 2;

export function ParticipantTile({ serverKey, channelId, member, size, onPress }: ParticipantTileProps) {
  const { colors } = useTheme();
  const avatarFile = useImageFile(serverKey, member.avatarHash);
  const playing = useSoundRing(serverKey, channelId, member.userId);

  return (
    <Touchable onPress={onPress} accessible accessibilityRole="button" accessibilityLabel={member.name}>
      <View
        style={[
          styles.tile,
          {
            width: size,
            height: size,
            backgroundColor: colors.surface,
          },
        ]}
      >
        <View style={styles.ringed}>
          <Avatar name={member.name} file={avatarFile} size={AVATAR} speaking={member.speaking} />
          {playing && (
            <View
              testID="participant-sound-ring"
              pointerEvents="none"
              style={[styles.soundRing, { borderColor: colors.text }]}
            />
          )}
        </View>
        <View style={styles.footer}>
          <Text variant="callout" numberOfLines={1} style={styles.name}>
            {member.name}
          </Text>
          {member.deafened ? (
            <Icon name="deafen" size={14} color="textMuted" testID="participant-deafened" />
          ) : member.muted ? (
            <Icon name="micOff" size={14} color="textMuted" testID="participant-muted" />
          ) : null}
        </View>
      </View>
    </Touchable>
  );
}

const styles = StyleSheet.create({
  ringed: {
    padding: SOUND_RING + SOUND_GAP,
  },
  soundRing: {
    ...StyleSheet.absoluteFill,
    borderRadius: AVATAR,
    borderWidth: SOUND_RING,
    borderStyle: 'dashed',
  },
  tile: {
    borderRadius: radius.tile,
    alignItems: 'center',
    justifyContent: 'center',
    padding: space.sm,
    gap: space.sm,
  },
  footer: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
    gap: space.xs,
    maxWidth: '100%',
    paddingHorizontal: space.xs,
  },
  name: {
    flexShrink: 1,
  },
});
