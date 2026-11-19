// A voice member's sheet (spec §8.6): avatar and name, a volume slider
// (0–200 %, 100 % marked) and a mute-for-me switch, or "This is you." for
// self. The values come from the store, so the sheet opens on what is set.
import { router } from 'expo-router';
import { StyleSheet, View } from 'react-native';

import { MemberProfile } from '@/features/members/MemberProfile';
import { setUserLocalMute, setUserVolume } from '@/state/actions/voice';
import { useServer, useUserAudio } from '@/state/selectors';
import { SheetHeader, Slider, Switch, Text, sheetLook, space, useSheetBottomInset, useTheme } from '@/ui';

export interface ParticipantSheetProps {
  serverKey: string;
  userId: string;
  onClose?: () => void;
}

/** "100 %", as the spec writes percentages. */
function percent(fraction: number): string {
  return `${Math.round(fraction * 100)} %`;
}

export function ParticipantSheet({ serverKey, userId, onClose }: ParticipantSheetProps) {
  const { colors } = useTheme();
  const bottomInset = useSheetBottomInset();
  const server = useServer(serverKey);
  const audio = useUserAudio(serverKey, userId);
  const selfId = server?.snapshot?.selfUser.id ?? server?.entry.userId;
  const isSelf = !!selfId && selfId === userId;
  const close = onClose ?? (() => router.back());

  return (
    <View style={[styles.sheet, { backgroundColor: sheetLook.background(colors), paddingBottom: bottomInset + space.lg }]}>
      <SheetHeader title="" onClose={close} />
      <MemberProfile serverKey={serverKey} userId={userId} />
      {isSelf ? (
        <Text variant="callout" color="textSecondary" style={styles.self}>
          This is you.
        </Text>
      ) : (
        <View style={[styles.controls, sheetLook.group(colors)]}>
          <View style={styles.section}>
            <View style={styles.labelRow}>
              <Text variant="body">Volume</Text>
              <Text variant="body" color="textSecondary" tabular>
                {percent(audio.volume)}
              </Text>
            </View>
            <Slider
              value={audio.volume}
              min={0}
              max={2}
              step={0.01}
              onChange={(volume) => setUserVolume(serverKey, userId, volume)}
              accessibilityLabel="Volume"
            />
            <View style={styles.marks}>
              <Text variant="caption" color="textMuted">{percent(0)}</Text>
              <Text variant="caption" color="textMuted">{percent(1)}</Text>
              <Text variant="caption" color="textMuted">{percent(2)}</Text>
            </View>
          </View>
          <View style={styles.switchRow}>
            <Text variant="body">Mute for me</Text>
            <Switch
              value={audio.mutedForMe}
              onChange={(muted) => setUserLocalMute(serverKey, userId, muted)}
              accessibilityLabel="Mute for me"
            />
          </View>
        </View>
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  sheet: { paddingHorizontal: space.lg },
  self: { paddingTop: space.lg, textAlign: 'center' },
  controls: { marginTop: space.lg, gap: space.lg },
  section: { gap: space.xs },
  labelRow: { flexDirection: 'row', justifyContent: 'space-between', alignItems: 'center' },
  marks: { flexDirection: 'row', justifyContent: 'space-between' },
  switchRow: { flexDirection: 'row', justifyContent: 'space-between', alignItems: 'center', minHeight: 44 },
});
