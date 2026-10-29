// A voice channel's pane (spec §8.6): its header, then who is in it.
// Connected: tiles, link banner, control bar.
// Not connected: tiles, Join voice button (or disabled reason).
import { router } from 'expo-router';
import { useCallback, useMemo, useState } from 'react';
import {
  type LayoutChangeEvent,
  ScrollView,
  StyleSheet,
  View,
  useWindowDimensions,
} from 'react-native';
import { Touchable } from 'react-native-gesture-handler';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { report } from '@/core';
import {
  joinVoice,
  leaveVoice,
  setDeafened,
  setMuted,
} from '@/state/actions/voice';
import {
  useChannelInfo,
  useMusicSettings,
  useMyVoice,
  useVoiceMembers,
} from '@/state/selectors';
import {
  EmptyState,
  PaneHeader,
  Text,
  radius,
  space,
  useTheme,
} from '@/ui';

import { ConnectedBar } from './ConnectedBar';
import { ControlBar } from './ControlBar';
import { LinkBanner } from './LinkBanner';
import { ParticipantTile } from './ParticipantTile';

export interface VoicePaneProps {
  serverKey: string;
  channelId: string;
  /** The sidebar button; on a tablet the list is always beside the pane, so there is none. */
  onOpenSidebar?: () => void;
}

const TABLET_MIN_WIDTH = 768;
const JOIN_BUTTON_HEIGHT = 52;

export function VoicePane({ serverKey, channelId, onOpenSidebar }: VoicePaneProps) {
  const { colors } = useTheme();
  const insets = useSafeAreaInsets();
  const { width: windowWidth } = useWindowDimensions();
  const isTablet = windowWidth >= TABLET_MIN_WIDTH;
  const columns = isTablet ? 3 : 2;

  const info = useChannelInfo(serverKey, channelId);
  const musicSettings = useMusicSettings(serverKey);
  const myVoice = useMyVoice();
  const members = useVoiceMembers(serverKey, channelId);

  const [containerWidth, setContainerWidth] = useState(windowWidth);

  const isConnectedHere = myVoice?.serverKey === serverKey && myVoice?.channelId === channelId;

  // Music button in header if music is supported and enabled
  const showMusicAction = !!(musicSettings?.supported && musicSettings?.enabled);

  // Sort members: self first, then others
  const sortedMembers = useMemo(() => {
    return [...members].sort((a, b) => {
      if (a.isSelf) return -1;
      if (b.isSelf) return 1;
      return a.name.localeCompare(b.name);
    });
  }, [members]);

  const handleOpenMember = useCallback(
    (userId: string) => {
      router.push(`/server/${serverKey}/member/${userId}`);
    },
    [serverKey],
  );

  const handleJoin = useCallback(() => {
    report(joinVoice(serverKey, channelId), 'join voice');
  }, [serverKey, channelId]);

  const handleRejoin = useCallback(() => {
    report(joinVoice(serverKey, channelId), 'rejoin voice');
  }, [serverKey, channelId]);

  const handleOpenMusic = useCallback(() => {
    router.push(`/server/${serverKey}/music/${channelId}`);
  }, [serverKey, channelId]);

  const onLayout = useCallback((e: LayoutChangeEvent) => {
    const width = e.nativeEvent.layout.width;
    if (width > 0) setContainerWidth(width);
  }, []);

  const gap = space.md;
  const padding = space.md;
  const tileWidth = Math.max(
    Math.floor((containerWidth - padding * 2 - gap * (columns - 1)) / columns),
    80,
  );

  // Access checks when not connected
  const userLimit = info?.channel.userLimit ?? 0;
  const canConnect = info?.access?.connect !== false;
  const isFull = userLimit > 0 && members.length >= userLimit;
  const joinDisabled = !canConnect || isFull;
  const disabledReason = !canConnect
    ? "You can't join this channel"
    : isFull
    ? 'This channel is full'
    : null;

  return (
    <View style={[styles.pane, { backgroundColor: colors.chat }]}>
      <PaneHeader
        title={info?.channel.name ?? ''}
        titleIcon="voiceChannel"
        left={onOpenSidebar ? { icon: 'sidebar', label: 'Show channels', onPress: onOpenSidebar } : undefined}
        right={
          showMusicAction
            ? [{ icon: 'music', label: 'Music', onPress: handleOpenMusic }]
            : undefined
        }
      />

      {isConnectedHere && <LinkBanner link={myVoice.link} onRejoin={handleRejoin} />}

      <ScrollView
        style={styles.scroll}
        contentContainerStyle={[styles.grid, { padding }]}
        onLayout={onLayout}
      >
        {sortedMembers.length === 0 ? (
          <EmptyState title="No one's here yet" />
        ) : (
          <View style={[styles.tilesRow, { gap }]}>
            {sortedMembers.map((member) => (
              <ParticipantTile
                key={member.userId}
                serverKey={serverKey}
                member={member}
                size={tileWidth}
                onPress={() => handleOpenMember(member.userId)}
              />
            ))}
          </View>
        )}
      </ScrollView>

      {/* Bottom area */}
      {isConnectedHere ? (
        <View style={{ paddingBottom: insets.bottom }}>
          <ControlBar
            muted={myVoice.muted}
            deafened={myVoice.deafened}
            onToggleMute={() => setMuted(!myVoice.muted)}
            onToggleDeafen={() => setDeafened(!myVoice.deafened)}
            onOpenMusic={handleOpenMusic}
            onLeave={leaveVoice}
          />
        </View>
      ) : (
        <View
          style={[
            styles.bottomSection,
            { paddingBottom: insets.bottom + space.md },
          ]}
        >
          {joinDisabled ? (
            <View
              style={[
                styles.disabledButton,
                { backgroundColor: colors.surface },
              ]}
              accessible
              accessibilityRole="button"
              accessibilityLabel={disabledReason ?? 'Disabled'}
              accessibilityState={{ disabled: true }}
            >
              <Text variant="headline" color="textSecondary">
                {disabledReason}
              </Text>
            </View>
          ) : (
            <Touchable
              onPress={handleJoin}
              accessible
              accessibilityRole="button"
              accessibilityLabel="Join voice"
            >
              <View
                style={[
                  styles.joinButton,
                  { backgroundColor: colors.voice },
                ]}
              >
                <Text variant="headline" color="onAccent">
                  Join voice
                </Text>
              </View>
            </Touchable>
          )}

          {/* Connected elsewhere bar */}
          <ConnectedBar serverKey={serverKey} channelId={channelId} />
        </View>
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  pane: { flex: 1 },
  scroll: { flex: 1 },
  grid: { flexGrow: 1 },
  tilesRow: {
    flexDirection: 'row',
    flexWrap: 'wrap',
  },
  bottomSection: {
    paddingHorizontal: space.md,
    gap: space.sm,
  },
  joinButton: {
    height: JOIN_BUTTON_HEIGHT,
    borderRadius: radius.icon,
    alignItems: 'center',
    justifyContent: 'center',
  },
  disabledButton: {
    height: JOIN_BUTTON_HEIGHT,
    borderRadius: radius.icon,
    alignItems: 'center',
    justifyContent: 'center',
  },
});
