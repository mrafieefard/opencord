import { router } from 'expo-router';
import { ScrollView, StyleSheet, View } from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';
import { ConnectionRecord_Tags, FailureKind } from '@opencord/core-native';

import { leaveServer } from '@/state/actions/servers';
import { useAppStore } from '@/state/store';
import { Button, PaneHeader, Text, useTheme } from '@/ui';
import { radius, space } from '@/ui/theme/space';

export interface IdentityChangedScreenProps {
  serverKey: string;
}

export function groupedFingerprint(hex: string): string {
  const clean = hex.replace(/[^0-9a-fA-F]/g, '').toUpperCase();
  return clean.match(/.{1,4}/g)?.join(' ') ?? clean;
}

export function IdentityChangedScreen({ serverKey }: IdentityChangedScreenProps) {
  const { colors } = useTheme();
  const server = useAppStore((state) => state.servers[serverKey]);
  const connection = server?.connection;

  const expected =
    connection?.tag === ConnectionRecord_Tags.Failed &&
    connection.inner.reason === FailureKind.FingerprintChanged
      ? connection.inner.expectedFingerprint
      : undefined;

  const presented =
    connection?.tag === ConnectionRecord_Tags.Failed &&
    connection.inner.reason === FailureKind.FingerprintChanged
      ? connection.inner.presentedFingerprint
      : undefined;

  const serverName = server?.entry.name ?? serverKey;

  const handleForget = async () => {
    await leaveServer(serverKey);
    router.back();
  };

  const handleDisconnect = () => {
    router.back();
  };

  const renderFingerprint = (label: string, hex?: string) => (
    <View style={styles.fpSection}>
      <Text variant="caption" color="textMuted" style={styles.fpLabel}>
        {label}
      </Text>
      <View
        style={[
          styles.fpBox,
          {
            backgroundColor: colors.surface,
            borderColor: colors.border,
          },
        ]}
      >
        <Text variant="mono" selectable color="text">
          {hex ? groupedFingerprint(hex) : 'Not known'}
        </Text>
      </View>
    </View>
  );

  return (
    <SafeAreaView style={[styles.container, { backgroundColor: colors.chat }]} edges={['top', 'bottom']}>
      <PaneHeader
        title="Server identity changed"
        left={{
          icon: 'close',
          label: 'Disconnect',
          onPress: handleDisconnect,
        }}
      />
      <ScrollView contentContainerStyle={styles.content}>
        <Text variant="body" color="text" style={styles.body}>
          {`${serverName} no longer shows the identity you trusted. Someone may be pretending to be it, or its owner replaced its certificate. Opencord will not connect until you know which: ask the owner, then forget the server and add it again.`}
        </Text>

        {renderFingerprint('Trusted fingerprint', expected)}
        {renderFingerprint('Fingerprint shown now', presented)}

        <View style={styles.actions}>
          <Button
            title="Forget server"
            variant="danger"
            onPress={handleForget}
          />
          <Button
            title="Disconnect"
            variant="secondary"
            onPress={handleDisconnect}
          />
        </View>
      </ScrollView>
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
  },
  content: {
    padding: space.lg,
    gap: space.lg,
  },
  body: {
    lineHeight: 22,
  },
  fpSection: {
    gap: space.xs,
  },
  fpLabel: {
    textTransform: 'uppercase',
  },
  fpBox: {
    padding: space.md,
    borderRadius: radius.image,
    borderWidth: StyleSheet.hairlineWidth,
  },
  actions: {
    marginTop: space.lg,
    gap: space.md,
  },
});
