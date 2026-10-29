// A person in a voice channel (spec §8.6): a surface square, 16 pt corners,
// a 64 pt avatar with a 3 pt voice ring while speaking, name and glyphs.
import { StyleSheet, View } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';

import { useImageFile } from '@/features/servers/useImageFile';
import type { VoiceMember } from '@/state/types';
import { Avatar, Icon, radius, space, Text, useTheme } from '@/ui';

export interface ParticipantTileProps {
  serverKey: string;
  member: VoiceMember;
  size: number;
  onPress?: () => void;
}

export function ParticipantTile({ serverKey, member, size, onPress }: ParticipantTileProps) {
  const { colors } = useTheme();
  const avatarFile = useImageFile(serverKey, member.avatarHash);

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
        <Avatar name={member.name} file={avatarFile} size={64} speaking={member.speaking} />
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
