// Prompt shown when connecting to a server with a self-signed or unknown certificate.
import { StyleSheet, View } from 'react-native';

import { Button, Text, radius, space, useTheme } from '@/ui';

export interface TrustStepProps {
  address: string;
  fingerprint: string;
  busy?: boolean;
  error?: string | null;
  onTrust: () => void;
  onCancel: () => void;
}

export function formatFingerprint(fingerprint: string): string {
  return fingerprint.match(/.{1,4}/g)?.join(' ') ?? fingerprint;
}

export function TrustStep({ fingerprint, busy = false, error = null, onTrust, onCancel }: TrustStepProps) {
  const { colors } = useTheme();

  return (
    <View style={styles.container}>
      <Text variant="title2">Trust this server?</Text>
      <Text variant="callout" color="textSecondary">
        {"Its certificate isn't from a public authority. Check this fingerprint with the server's owner."}
      </Text>
      <View style={[styles.fingerprintBox, { backgroundColor: colors.surface }]}>
        <Text variant="mono" color="text" style={styles.fingerprint}>
          {formatFingerprint(fingerprint)}
        </Text>
      </View>
      {error !== null && (
        <Text variant="footnote" color="recording" accessibilityRole="alert">
          {error}
        </Text>
      )}
      <View style={styles.actions}>
        <Button title="Trust and add" variant="primary" loading={busy} onPress={onTrust} />
        <Button title="Cancel" variant="secondary" disabled={busy} onPress={onCancel} />
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    padding: space.lg,
    gap: space.md,
  },
  fingerprintBox: {
    borderRadius: radius.icon,
    padding: space.md,
    marginTop: space.sm,
  },
  fingerprint: {
    textAlign: 'center',
  },
  actions: {
    marginTop: space.md,
    gap: space.sm,
  },
});
