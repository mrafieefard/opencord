// The You tab (spec §8.8): the person's profile, identity backup, Voice &
// audio, appearance, music listening preferences, and version information.
import { useCallback, useMemo } from 'react';
import { ScrollView, StyleSheet, View } from 'react-native';
import Constants from 'expo-constants';
import { useRouter } from 'expo-router';

import { getCore } from '@/core';
import { useVoicePillRoom } from '@/features/voice/callPosition';
import { setTheme } from '@/state/actions/prefs';
import { toPresenceStatus } from '@/state/actions/presence';
import { usePrefs, useSelf } from '@/state/selectors';
import type { Theme, UserStatus } from '@/state/types';
import { Avatar, Divider, Icon, Row, Segmented, Text, radius, space, useTheme } from '@/ui';

import { MusicListeningRows } from './MusicListeningRows';

const THEME_OPTIONS: { label: string; value: Theme }[] = [
  { label: 'System', value: 'system' },
  { label: 'Light', value: 'light' },
  { label: 'Dark', value: 'dark' },
];

function statusLabel(status: UserStatus): string {
  switch (status) {
    case 'online':
      return 'Online';
    case 'idle':
      return 'Idle';
    case 'dnd':
      return 'Do not disturb';
  }
}

export function YouScreen() {
  const router = useRouter();
  const self = useSelf();
  const { theme, status } = usePrefs();
  const { colors } = useTheme();
  const pillRoom = useVoicePillRoom();

  const themeIndex = THEME_OPTIONS.findIndex((opt) => opt.value === theme);

  const handleThemeChange = useCallback((index: number) => {
    const chosen = THEME_OPTIONS[index]?.value ?? 'system';
    setTheme(chosen);
  }, []);

  const avatarHash = self?.avatarHash;
  const ownAvatar = useMemo(() => {
    void avatarHash;
    return getCore().ownAvatar();
  }, [avatarHash]);
  const appVersion = Constants.expoConfig?.version ?? '0.0.1';
  const coreVersion = getCore().coreVersion();

  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      style={[styles.screen, { backgroundColor: colors.chat }]}
      contentContainerStyle={[styles.content, { paddingBottom: space.lg + pillRoom }]}
    >
      {/* Profile */}
      <View style={[styles.card, { backgroundColor: colors.surface }]}>
        <Row
          accessibilityLabel="Profile"
          onPress={() => router.push('/you/profile')}
          style={styles.profileRowItem}
        >
          <View style={styles.profileRowContent}>
            <Avatar
              name={self?.displayName ?? ''}
              file={ownAvatar.file}
              size={56}
              presence={toPresenceStatus(status)}
            />
            <View style={styles.profileDetails}>
              <Text variant="headline">{self?.displayName ?? ''}</Text>
              <Text variant="subhead" color="textSecondary">
                {statusLabel(status)}
              </Text>
            </View>
            <View style={styles.chevron}>
              <Icon name="back" size={16} color="textSecondary" />
            </View>
          </View>
        </Row>
      </View>

      {/* Voice */}
      <View style={styles.section}>
        <Text variant="caption" color="textMuted" style={styles.sectionTitle}>
          Voice
        </Text>
        <View style={[styles.card, { backgroundColor: colors.surface }]}>
          <Row
            accessibilityLabel="Voice & audio"
            onPress={() => router.push('/you/voice')}
            style={styles.rowItem}
          >
            <View style={styles.rowContent}>
              <Text variant="body">Voice & audio</Text>
              <View style={styles.chevron}>
                <Icon name="back" size={14} color="textSecondary" />
              </View>
            </View>
          </Row>
        </View>
      </View>

      {/* Sounds */}
      <View style={styles.section}>
        <Text variant="caption" color="textMuted" style={styles.sectionTitle}>
          Sounds
        </Text>
        <View style={[styles.card, { backgroundColor: colors.surface }]}>
          <Row
            accessibilityLabel="Sounds"
            onPress={() => router.push('/you/sounds')}
            style={styles.rowItem}
          >
            <View style={styles.rowContent}>
              <Text variant="body">Sounds</Text>
              <View style={styles.chevron}>
                <Icon name="back" size={14} color="textSecondary" />
              </View>
            </View>
          </Row>
        </View>
      </View>

      {/* Storage */}
      <View style={styles.section}>
        <Text variant="caption" color="textMuted" style={styles.sectionTitle}>
          Storage
        </Text>
        <View style={[styles.card, { backgroundColor: colors.surface }]}>
          <Row
            accessibilityLabel="Storage and logs"
            onPress={() => router.push('/you/storage')}
            style={styles.rowItem}
          >
            <View style={styles.rowContent}>
              <Text variant="body">Storage and logs</Text>
              <View style={styles.chevron}>
                <Icon name="back" size={14} color="textSecondary" />
              </View>
            </View>
          </Row>
        </View>
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
  profileRowItem: {
    paddingHorizontal: space.md,
    paddingVertical: space.md,
  },
  profileRowContent: {
    flex: 1,
    flexDirection: 'row',
    alignItems: 'center',
    gap: space.md,
  },
  profileDetails: {
    flex: 1,
    gap: 2,
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
