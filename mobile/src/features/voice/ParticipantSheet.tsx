// A voice member's sheet (spec §8.6): avatar, name, volume slider (0–200 %)
// and mute-for-me switch, or "This is you." for self.
import { router } from 'expo-router';
import { useCallback, useState } from 'react';
import { StyleSheet, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { useImageFile } from '@/features/servers/useImageFile';
import { setUserLocalMute, setUserVolume } from '@/state/actions/voice';
import { useMember, useServer } from '@/state/selectors';
import { Avatar, SheetHeader, Slider, Switch, Text, space, useTheme } from '@/ui';

export interface ParticipantSheetProps {
  serverKey: string;
  userId: string;
  onClose?: () => void;
}

export function ParticipantSheet({ serverKey, userId, onClose }: ParticipantSheetProps) {
  const { colors } = useTheme();
  const insets = useSafeAreaInsets();
  const server = useServer(serverKey);
  const member = useMember(serverKey, userId);
  const selfId = server?.snapshot?.selfUser.id ?? server?.entry.userId;
  const isSelf = !!selfId && selfId === userId;

  const [volume, setVolume] = useState(1);
  const [localMuted, setLocalMuted] = useState(false);

  const name = member?.name ?? 'Member';
  const avatarFile = useImageFile(serverKey, member?.avatarHash);
  const close = onClose ?? (() => router.back());

  const handleVolumeChange = useCallback(
    (next: number) => {
      setVolume(next);
      setUserVolume(serverKey, userId, next);
    },
    [serverKey, userId],
  );

  const handleMuteChange = useCallback(
    (next: boolean) => {
      setLocalMuted(next);
      setUserLocalMute(serverKey, userId, next);
    },
    [serverKey, userId],
  );

  return (
    <View style={[styles.sheet, { backgroundColor: colors.elevated, paddingBottom: insets.bottom + space.lg }]}>
      <SheetHeader title={name} onClose={close} />
      <View style={styles.body}>
        <View style={styles.userRow}>
          <Avatar name={name} file={avatarFile} size={64} />
          <Text variant="title2" style={styles.userName}>
            {name}
          </Text>
        </View>

        {isSelf ? (
          <View style={styles.selfBox}>
            <Text variant="callout" color="textSecondary">
              This is you.
            </Text>
          </View>
        ) : (
          <View style={styles.controls}>
            {/* Volume */}
            <View style={styles.section}>
              <View style={styles.labelRow}>
                <Text variant="subhead">Volume</Text>
                <Text variant="subhead" color="textSecondary">
                  {Math.round(volume * 100)} %
                </Text>
              </View>
              <Slider
                value={volume}
                min={0}
                max={2}
                step={0.01}
                onChange={handleVolumeChange}
                accessibilityLabel="Volume"
              />
              <View style={styles.sliderMarkers}>
                <Text variant="caption" color="textMuted">0%</Text>
                <Text variant="caption" color="textMuted">100%</Text>
                <Text variant="caption" color="textMuted">200%</Text>
              </View>
            </View>

            {/* Mute for me */}
            <View style={styles.switchRow}>
              <Text variant="subhead">Mute for me</Text>
              <Switch
                value={localMuted}
                onChange={handleMuteChange}
                accessibilityLabel="Mute for me"
              />
            </View>
          </View>
        )}
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  sheet: {
    paddingHorizontal: space.lg,
  },
  body: {
    paddingTop: space.md,
    gap: space.lg,
  },
  userRow: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: space.md,
  },
  userName: {
    flexShrink: 1,
  },
  selfBox: {
    paddingVertical: space.sm,
  },
  controls: {
    gap: space.lg,
  },
  section: {
    gap: space.xs,
  },
  labelRow: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
  },
  sliderMarkers: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    paddingHorizontal: space.xs,
  },
  switchRow: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    minHeight: 44,
  },
});
