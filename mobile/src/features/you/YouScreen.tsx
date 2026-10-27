// The You tab (spec §8.8): the person's profile, identity backup, appearance,
// music listening preferences, and version information.
import { useCallback } from 'react';
import { ScrollView, StyleSheet, View } from 'react-native';
import * as Clipboard from 'expo-clipboard';
import Constants from 'expo-constants';
import { File, Paths } from 'expo-file-system';
import { useRouter } from 'expo-router';
import * as Sharing from 'expo-sharing';
import { Touchable } from 'react-native-gesture-handler';

import { getCore } from '@/core';
import { loadIdentity } from '@/core/identity';
import { setTheme } from '@/state/actions/prefs';
import { toast } from '@/state/actions/ui';
import { usePrefs, useSelf } from '@/state/selectors';
import type { Theme } from '@/state/types';
import { Avatar, Divider, Icon, Row, Segmented, Text, radius, space, typography, useTheme } from '@/ui';

import { MusicListeningRows } from './MusicListeningRows';

const THEME_OPTIONS: { label: string; value: Theme }[] = [
  { label: 'System', value: 'system' },
  { label: 'Light', value: 'light' },
  { label: 'Dark', value: 'dark' },
];

export function YouScreen() {
  const router = useRouter();
  const self = useSelf();
  const { theme } = usePrefs();
  const { colors } = useTheme();

  const themeIndex = THEME_OPTIONS.findIndex((opt) => opt.value === theme);

  const fingerprint = self?.fingerprint;
  const copyFingerprint = useCallback(async () => {
    if (!fingerprint) return;
    await Clipboard.setStringAsync(fingerprint);
    toast('Copied fingerprint');
  }, [fingerprint]);

  const handleBackup = useCallback(async () => {
    const core = getCore();
    const identity = await loadIdentity(core);
    if (!identity) {
      toast('No identity loaded', 'error');
      return;
    }
    const backupText = core.identityBackupEncode(identity.secret);
    const file = new File(Paths.cache, 'opencord-identity.txt');
    await file.write(backupText);
    await Sharing.shareAsync(file.uri, {
      dialogTitle: 'Back up your identity',
    });
  }, []);

  const handleThemeChange = useCallback((index: number) => {
    const chosen = THEME_OPTIONS[index]?.value ?? 'system';
    setTheme(chosen);
  }, []);

  const appVersion = Constants.expoConfig?.version ?? '0.0.1';
  let coreVersion = 'unknown';
  try {
    coreVersion = getCore().coreVersion();
  } catch {
    // core not initialized or in tests
  }

  return (
    <ScrollView
      style={[styles.screen, { backgroundColor: colors.sidebar }]}
      contentContainerStyle={styles.content}
    >
      {/* Header */}
      <View style={styles.header}>
        <Avatar name={self?.displayName ?? ''} size={72} />
        <View style={styles.headerText}>
          <Text variant="title2">{self?.displayName ?? ''}</Text>
          <Touchable
            onPress={() => void copyFingerprint()}
            style={styles.fingerprintButton}
          >
            <View
              accessible
              accessibilityRole="button"
              accessibilityLabel="Copy fingerprint"
              style={styles.fingerprintContent}
            >
              <Text variant="mono" color="textSecondary" style={typography.footnote}>
                {self?.fingerprint ?? ''}
              </Text>
              <Icon name="copy" size={14} color="textSecondary" />
            </View>
          </Touchable>
        </View>
      </View>

      {/* Profile */}
      <View style={styles.section}>
        <Text variant="caption" color="textMuted" style={styles.sectionTitle}>
          Profile
        </Text>
        <View style={[styles.card, { backgroundColor: colors.surface }]}>
          <Row
            accessibilityLabel="Display name"
            onPress={() => router.push('/you/name')}
            style={styles.rowItem}
          >
            <View style={styles.rowContent}>
              <Text variant="body">Display name</Text>
              <View style={styles.rowRight}>
                <Text variant="callout" color="textSecondary">
                  {self?.displayName ?? ''}
                </Text>
                <View style={styles.chevron}>
                  <Icon name="back" size={14} color="textSecondary" />
                </View>
              </View>
            </View>
          </Row>
        </View>
      </View>

      {/* Identity */}
      <View style={styles.section}>
        <Text variant="caption" color="textMuted" style={styles.sectionTitle}>
          Identity
        </Text>
        <View style={[styles.card, { backgroundColor: colors.surface }]}>
          <Row
            accessibilityLabel="Back up your identity"
            onPress={() => void handleBackup()}
            style={styles.rowItem}
          >
            <View style={styles.rowContent}>
              <Text variant="body">Back up your identity</Text>
              <Icon name="share" size={18} color="textSecondary" />
            </View>
          </Row>
        </View>
        <Text variant="footnote" color="textSecondary" style={styles.sectionFooter}>
          This file is your identity. Anyone who has it can be you. Keep it somewhere safe.
        </Text>
      </View>

      {/* Appearance */}
      <View style={styles.section}>
        <Text variant="caption" color="textMuted" style={styles.sectionTitle}>
          Appearance
        </Text>
        <View style={[styles.card, styles.paddedCard, { backgroundColor: colors.surface }]}>
          <Segmented
            values={THEME_OPTIONS.map((o) => o.label)}
            index={themeIndex >= 0 ? themeIndex : 0}
            onChange={handleThemeChange}
          />
        </View>
      </View>

      {/* Music */}
      <View style={styles.section}>
        <Text variant="caption" color="textMuted" style={styles.sectionTitle}>
          Music
        </Text>
        <View style={[styles.card, styles.paddedCard, { backgroundColor: colors.surface }]}>
          <MusicListeningRows />
        </View>
      </View>

      {/* About */}
      <View style={styles.section}>
        <Text variant="caption" color="textMuted" style={styles.sectionTitle}>
          About
        </Text>
        <View style={[styles.card, { backgroundColor: colors.surface }]}>
          <View style={styles.infoRow}>
            <Text variant="body">Opencord</Text>
            <Text variant="callout" color="textSecondary">
              {appVersion}
            </Text>
          </View>
          <Divider />
          <View style={styles.infoRow}>
            <Text variant="body">Core</Text>
            <Text variant="callout" color="textSecondary">
              {coreVersion}
            </Text>
          </View>
        </View>
      </View>
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  screen: {
    flex: 1,
  },
  content: {
    padding: space.lg,
    gap: space.lg,
  },
  header: {
    alignItems: 'center',
    gap: space.md,
    paddingVertical: space.md,
  },
  headerText: {
    alignItems: 'center',
    gap: space.xs,
  },
  fingerprintButton: {
    paddingVertical: space.xs,
    paddingHorizontal: space.sm,
  },
  fingerprintContent: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: space.xs,
  },
  section: {
    gap: space.xs,
  },
  sectionTitle: {
    paddingHorizontal: space.sm,
  },
  sectionFooter: {
    paddingHorizontal: space.sm,
    paddingTop: space.xs,
  },
  card: {
    borderRadius: radius.icon,
    overflow: 'hidden',
  },
  paddedCard: {
    padding: space.md,
  },
  rowItem: {
    paddingHorizontal: space.md,
  },
  rowContent: {
    flex: 1,
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
  },
  rowRight: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: space.xs,
  },
  chevron: {
    transform: [{ rotate: '180deg' }],
  },
  infoRow: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    paddingHorizontal: space.md,
    paddingVertical: space.md,
    minHeight: 44,
  },
});
