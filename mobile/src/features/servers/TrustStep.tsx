// The trust step (spec §8.3), shown in the Add server sheet when a server's
// certificate is self-signed or unknown: the address, and the fingerprint
// as a 4 × 4 grid of groups to read out and compare. Its top is every
// sheet's (SheetHeader): the title, the address under it, and the close
// button, which goes back to the address.
import { StyleSheet, View } from 'react-native';

import { Button, Icon, SheetHeader, Text, radius, sheetLook, space, useTheme } from '@/ui';

export interface TrustStepProps {
  address: string;
  fingerprint: string;
  busy?: boolean;
  error?: string | null;
  onTrust: () => void;
  onCancel: () => void;
}

const GROUPS_PER_LINE = 4;

export function formatFingerprint(fingerprint: string): string {
  return fingerprint.match(/.{1,4}/g)?.join(' ') ?? fingerprint;
}

/** The fingerprint's groups of four, four to a line. */
export function fingerprintLines(fingerprint: string): string[] {
  const groups = fingerprint.match(/.{1,4}/g) ?? [fingerprint];
  const lines: string[] = [];
  for (let i = 0; i < groups.length; i += GROUPS_PER_LINE) {
    lines.push(groups.slice(i, i + GROUPS_PER_LINE).join(' '));
  }
  return lines;
}

export function TrustStep({ address, fingerprint, busy = false, error = null, onTrust, onCancel }: TrustStepProps) {
  const { colors } = useTheme();

  return (
    <View style={styles.container}>
      <SheetHeader title="Trust this server?" subtitle={address} onClose={onCancel} />

      <Text variant="callout" color="textSecondary">
        {"Its certificate isn't from a public authority. Check this fingerprint with the server's owner."}
      </Text>

      <View
        accessible
        accessibilityLabel={`Fingerprint ${formatFingerprint(fingerprint)}`}
        style={[styles.fingerprintBox, { backgroundColor: sheetLook.well(colors), borderColor: sheetLook.wellBorder(colors) }]}
      >
        <Text variant="caption" color="textMuted">
          SHA-256 fingerprint
        </Text>
        <View style={styles.lines}>
          {fingerprintLines(fingerprint).map((line, index) => (
            <Text key={index} variant="mono" color="text" selectable style={styles.line}>
              {line}
            </Text>
          ))}
        </View>
      </View>

      {error !== null && (
        <View style={styles.error}>
          <Icon name="warning" size={14} color="recording" />
          <Text variant="footnote" color="recording" accessibilityRole="alert" style={styles.errorText}>
            {error}
          </Text>
        </View>
      )}

      <View style={styles.actions}>
        <Button title="Trust and add" variant="primary" loading={busy} onPress={onTrust} />
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    gap: space.lg,
  },
  fingerprintBox: {
    borderRadius: radius.icon,
    borderWidth: StyleSheet.hairlineWidth,
    paddingVertical: space.md,
    paddingHorizontal: space.lg,
    alignItems: 'center',
    gap: space.sm,
  },
  lines: {
    gap: 2,
  },
  line: {
    letterSpacing: 1,
    textAlign: 'center',
  },
  error: {
    flexDirection: 'row',
    alignItems: 'flex-start',
    gap: space.xs + 2,
  },
  errorText: {
    flexShrink: 1,
  },
  actions: {
    marginTop: space.xs,
  },
});
