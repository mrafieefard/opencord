import { StyleSheet, View } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';

import { PresenceStatus } from '@/core/native';
import { useImageFile } from '@/features/servers/useImageFile';
import { Avatar, Text, space, useTheme } from '@/ui';
import type { MemberItem } from './groupMembers';

export const MEMBER_ROW_HEIGHT = 52;

export interface MemberRowProps {
  serverKey: string;
  item: MemberItem;
  onPress: () => void;
}

export function MemberRow({ serverKey, item, onPress }: MemberRowProps) {
  const { colors } = useTheme();
  const avatarFile = useImageFile(serverKey, item.avatarHash);
  const isOffline = item.presence === PresenceStatus.Offline;

  const nameColor = item.roleColor != null && item.roleColor !== 0
    ? `#${item.roleColor.toString(16).padStart(6, '0')}`
    : colors.text;

  return (
    <Touchable
      onPress={onPress}
      accessibilityRole="button"
      accessibilityLabel={item.name}
      underlayColor={colors.hover}
      activeUnderlayOpacity={1}
      style={[styles.row, isOffline && styles.offline]}
    >
      <View style={styles.content}>
        <Avatar
          name={item.name}
          file={avatarFile}
          size={32}
          presence={item.presence}
          ringColor="sidebar"
        />
        <View style={styles.textContainer}>
          <Text
            variant="body"
            numberOfLines={1}
            style={[styles.name, { color: nameColor }]}
          >
            {item.name}
          </Text>
          {item.activity && (
            <Text variant="caption" color="textSecondary" numberOfLines={1}>
              {item.activity}
            </Text>
          )}
        </View>
      </View>
    </Touchable>
  );
}

const styles = StyleSheet.create({
  row: {
    height: MEMBER_ROW_HEIGHT,
    justifyContent: 'center',
    paddingHorizontal: space.lg,
  },
  content: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: space.md,
  },
  textContainer: {
    flex: 1,
    justifyContent: 'center',
  },
  name: {
    fontWeight: '500',
  },
  offline: {
    opacity: 0.5,
  },
});
