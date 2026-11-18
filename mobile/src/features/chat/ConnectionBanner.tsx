import { router } from 'expo-router';
import { StyleSheet, TouchableOpacity, View } from 'react-native';
import { ConnectionRecord_Tags, FailureKind } from '@opencord/core-native';

import { connectionText } from '@/features/servers/connectionText';
import { retryServer } from '@/state/actions/servers';
import { useAppStore } from '@/state/store';
import { Icon, Text, useTheme } from '@/ui';
import { radius, space } from '@/ui/theme/space';

export interface ConnectionBannerProps {
  serverKey: string;
}

export function ConnectionBanner({ serverKey }: ConnectionBannerProps) {
  const { colors } = useTheme();
  const server = useAppStore((state) => state.servers[serverKey]);
  const connection = server?.connection;
  if (!server || !connection) {
    return null;
  }
  if (
    connection.tag === ConnectionRecord_Tags.Connected ||
    connection.tag === ConnectionRecord_Tags.Connecting
  ) {
    return null;
  }

  const isFingerprintChanged =
    connection.tag === ConnectionRecord_Tags.Failed &&
    connection.inner.reason === FailureKind.FingerprintChanged;

  const info = connectionText(connection);
  const text = isFingerprintChanged ? 'Server identity changed' : (info?.text ?? "Can't reach server");

  const canRetry =
    connection.tag === ConnectionRecord_Tags.Reconnecting ||
    (connection.tag === ConnectionRecord_Tags.Failed &&
      connection.inner.reason === FailureKind.Incompatible);

  const handlePress = () => {
    if (isFingerprintChanged) {
      router.push(`/server/${serverKey}/identity-changed`);
    }
  };

  const handleRetry = () => {
    void retryServer(serverKey);
  };

  return (
    <TouchableOpacity
      activeOpacity={isFingerprintChanged ? 0.7 : 1}
      onPress={handlePress}
      testID="connection-banner"
      style={[
        styles.banner,
        {
          backgroundColor: colors.hover,
          borderBottomColor: colors.border,
        },
      ]}
    >
      <View style={styles.left}>
        <Icon name={isFingerprintChanged ? 'warning' : 'retry'} size={18} color="textSecondary" />
        <Text variant="subhead" style={[styles.text, { color: colors.text }]} numberOfLines={1}>
          {text}
        </Text>
      </View>
      {canRetry && (
        <TouchableOpacity
          onPress={handleRetry}
          style={[styles.retryButton, { backgroundColor: colors.surface, borderColor: colors.border }]}
          testID="retry-now-button"
        >
          <Text variant="caption" style={{ color: colors.text }}>
            Retry now
          </Text>
        </TouchableOpacity>
      )}
      {isFingerprintChanged && (
        <Text variant="caption" style={{ color: colors.textSecondary }}>
          Review
        </Text>
      )}
    </TouchableOpacity>
  );
}

const styles = StyleSheet.create({
  banner: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    paddingHorizontal: space.lg,
    paddingVertical: space.md,
    borderBottomWidth: StyleSheet.hairlineWidth,
  },
  left: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: space.sm,
    flex: 1,
    marginRight: space.md,
  },
  text: {
    flexShrink: 1,
  },
  retryButton: {
    paddingHorizontal: space.md,
    paddingVertical: space.xs,
    borderRadius: radius.pill,
    borderWidth: StyleSheet.hairlineWidth,
  },
});
