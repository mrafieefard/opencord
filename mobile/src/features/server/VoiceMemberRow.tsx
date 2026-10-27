import { memo } from 'react';
import { StyleSheet, View } from 'react-native';

import { useImageFile } from '@/features/servers/useImageFile';
import type { VoiceMember } from '@/state/types';
import { Avatar, Icon, Text, space, useTheme } from '@/ui';

export interface VoiceMemberRowProps {
  serverKey: string;
  member: VoiceMember;
}

export const VoiceMemberRow = memo(function VoiceMemberRow({ serverKey, member }: VoiceMemberRowProps) {
  const { colors } = useTheme();
  const avatarFile = useImageFile(serverKey, member.avatarHash);

  return (
    <View style={styles.row} accessibilityLabel={member.name}>
      <View
        style={[
          styles.avatarWrapper,
          member.speaking && { borderColor: colors.voice, borderWidth: 2 },
        ]}
        testID={member.speaking ? 'speaking-ring' : undefined}
      >
        <Avatar name={member.name} file={avatarFile} size={22} speaking={member.speaking} />
      </View>
      <Text
        variant="callout"
        color={member.speaking ? 'text' : 'textSecondary'}
        numberOfLines={1}
        style={[styles.name, member.speaking && styles.speakingName]}
      >
        {member.name}
      </Text>
      <View style={styles.glyphs}>
        {member.deafened ? (
          <Icon name="deafen" size={14} color="textMuted" testID="member-deafened" />
        ) : member.muted ? (
          <Icon name="micOff" size={14} color="textMuted" testID="member-muted" />
        ) : null}
      </View>
    </View>
  );
});

const styles = StyleSheet.create({
  row: {
    height: 32,
    flexDirection: 'row',
    alignItems: 'center',
    paddingLeft: space.xl + space.md,
    paddingRight: space.lg,
  },
  avatarWrapper: {
    width: 26,
    height: 26,
    borderRadius: 13,
    justifyContent: 'center',
    alignItems: 'center',
    marginRight: space.sm,
  },
  name: {
    flex: 1,
  },
  speakingName: {
    fontWeight: '600',
  },
  glyphs: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: space.xs,
  },
});
