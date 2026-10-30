// The trust step (spec §8.3), shown in the Add server sheet when a server's
// certificate is self-signed or unknown: the address, and the fingerprint
// as a 4 × 4 grid of groups to read out and compare.
import { StyleSheet, View } from 'react-native';

import { Button, Icon, SheetGrabber, Text, radius, sheetLook, space, useTheme } from '@/ui';

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

const BADGE = 48;

export function TrustStep({ address, fingerprint, busy = false, error = null, onTrust, onCancel }: TrustStepProps) {
  const { colors } = useTheme();

  return (
    <View style={styles.container}>
      <SheetGrabber />
      <View style={[styles.badge, { backgroundColor: sheetLook.well(colors) }]}>
        <Icon name="lock" size={22} color="text" />
      </View>

      <View style={styles.heading}>
        <Text variant="title2" accessibilityRole="header" style={styles.center}>
          Trust this server?
        </Text>
        <Text variant="headline" color="textSecondary" numberOfLines={1} style={styles.center}>
          {address}
        </Text>
      </View>

      <Text variant="callout" color="textSecondary" style={styles.center}>
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
        <Button title="Cancel" variant="plain" disabled={busy} onPress={onCancel} />
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    gap: space.lg,
  },
  badge: {
    alignSelf: 'center',
    width: BADGE,
    height: BADGE,
    borderRadius: BADGE / 2,
    alignItems: 'center',
    justifyContent: 'center',
  },
  heading: {
    gap: space.xs,
  },
  center: {
    textAlign: 'center',
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
    justifyContent: 'center',
    gap: space.xs + 2,
  },
  errorText: {
    flexShrink: 1,
  },
  actions: {
    marginTop: space.xs,
    gap: space.xs,
  },
});
