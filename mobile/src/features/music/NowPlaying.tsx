// What plays (spec §8.7): the artwork, which settles back a little while
// paused; the title and artist with the ⋯ menu beside them; who added it
// and from where; then the position, which a finger can drag with Control
// music on a seekable track.
import type { MusicQueueItemRecord } from '@opencord/core-native';
import { useEffect, useRef, useState } from 'react';
import { StyleSheet, View } from 'react-native';
import Animated, { useReducedMotion } from 'react-native-reanimated';

import { toast } from '@/state/actions/ui';
import { useMemberName } from '@/state/selectors';
import { IconButton, Menu, type MenuAction, type MenuHandle, Scrubber, Text, onMedia, radius, space, useTheme } from '@/ui';

import { Artwork } from './Artwork';
import type { ControlAccess } from './musicAccess';
import { formatDuration } from './position';
import { sourceLabel } from './SourceChips';

export interface NowPlayingProps {
  serverKey: string;
  item: MusicQueueItemRecord;
  playing: boolean;
  positionMs: number;
  seek: ControlAccess;
  artworkSize: number;
  menu: MenuAction[];
  onMenu: (id: string) => void;
  onSeek: (positionMs: number) => void;
}

const PAUSED_SCALE = 0.86;
const SETTLE_MS = 400;
/** After a seek the bar stays where the finger left it until the session catches up, or this long. */
const HOLD_MS = 2000;
const HOLD_NEAR_MS = 1500;
const SEEK_STEP_MS = 1000;

export function NowPlaying({ serverKey, item, playing, positionMs, seek, artworkSize, menu, onMenu, onSeek }: NowPlayingProps) {
  const { colors, scheme } = useTheme();
  const reduceMotion = useReducedMotion();
  const { track } = item;
  const requester = useMemberName(serverKey, item.requesterId);

  const menuRef = useRef<MenuHandle>(null);
  const [drag, setDrag] = useState<number | null>(null);
  const [held, setHeld] = useState<number | null>(null);
  useEffect(() => {
    if (held === null) return;
    const timer = setTimeout(() => setHeld(null), HOLD_MS);
    return () => clearTimeout(timer);
  }, [held]);

  const durationMs = track.durationMs;
  const canScrub = !track.isStream && durationMs > 0;
  const live = Math.min(positionMs, canScrub ? durationMs : positionMs);
  const caughtUp = held === null || Math.abs(live - held) < HOLD_NEAR_MS;
  const shown = drag ?? (caughtUp ? live : held);

  const commit = (value: number) => {
    setDrag(null);
    setHeld(value);
    onSeek(Math.round(value));
  };
  const source = track.sourceDisplayName || sourceLabel(track.source);

  return (
    <View>
      <View style={styles.center}>
        <Animated.View
          style={[
            {
              borderRadius: radius.tile,
              backgroundColor: colors.elevated,
              transform: [{ scale: playing ? 1 : PAUSED_SCALE }],
              transitionProperty: 'transform',
              transitionDuration: reduceMotion ? 0 : SETTLE_MS,
              transitionTimingFunction: 'ease-out',
            },
            scheme === 'light' && (playing ? styles.liftedHigh : styles.liftedLow),
          ]}
        >
          <Artwork serverKey={serverKey} artworkId={track.artworkId} title={track.title} size={artworkSize} radius={radius.tile} />
        </Animated.View>
      </View>

      <View style={styles.titleRow}>
        <View style={styles.titles}>
          <Text variant="title2" numberOfLines={1} accessibilityRole="header">
            {track.title}
          </Text>
          <Text variant="body" color="textSecondary" numberOfLines={1}>
            {track.artist}
          </Text>
          <Text variant="footnote" color="textMuted" numberOfLines={1}>
            {`Added by ${requester}${source ? ` · ${source}` : ''}`}
          </Text>
        </View>
        {menu.length > 0 && (
          <Menu ref={menuRef} testID="now-playing-menu" trigger="press" actions={menu} onAction={onMenu}>
            <IconButton icon="more" label="More" size={36} onPress={() => menuRef.current?.open()} />
          </Menu>
        )}
      </View>

      <View style={styles.progress}>
        <Scrubber
          testID="progress"
          value={canScrub ? shown : 0}
          max={canScrub ? durationMs : 1}
          step={SEEK_STEP_MS}
          accessibilityStep={10_000}
          smooth={playing && drag === null}
          disabled={!seek.allowed || !canScrub}
          onDisabledPress={() => seek.reason && toast(seek.reason)}
          onChange={setDrag}
          onCommit={commit}
          accessibilityLabel="Progress"
          accessibilityValue={canScrub ? `${formatDuration(shown)} of ${formatDuration(durationMs)}` : 'Live'}
        />
        <View style={styles.times}>
          <Text variant="footnote" color={drag === null ? 'textMuted' : 'text'} tabular>
            {formatDuration(shown)}
          </Text>
          <Text variant="footnote" color={drag === null ? 'textMuted' : 'text'} tabular>
            {canScrub ? `-${formatDuration(Math.max(0, durationMs - shown))}` : 'Live'}
          </Text>
        </View>
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  center: { alignItems: 'center', paddingTop: space.lg, paddingBottom: space.xxl },
  liftedHigh: { shadowColor: onMedia.rail, shadowOpacity: 0.22, shadowRadius: 24, shadowOffset: { width: 0, height: 14 }, elevation: 16 },
  liftedLow: { shadowColor: onMedia.rail, shadowOpacity: 0.12, shadowRadius: 12, shadowOffset: { width: 0, height: 6 }, elevation: 6 },
  titleRow: { flexDirection: 'row', alignItems: 'center', gap: space.md },
  titles: { flex: 1, gap: 2 },
  progress: { marginTop: space.lg },
  times: { flexDirection: 'row', justifyContent: 'space-between', marginTop: -space.xs },
});
