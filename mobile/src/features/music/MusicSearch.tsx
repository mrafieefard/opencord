// Search (spec §8.7): the field opens with the keyboard up and a Cancel
// beside it; chips pick the source. It searches when submitted, not while
// typing, and says what it's doing: a hint before, a spinner during, and
// "No results" or the server's reason after.
import { MusicPlacement, type MusicSearchResultRecord } from '@opencord/core-native';
import * as Haptics from 'expo-haptics';
import { useCallback, useState } from 'react';
import { AccessibilityInfo, ActivityIndicator, StyleSheet, View } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';

import { report } from '@/core';
import { enqueue, search } from '@/state/actions/music';
import { useChannelInfo, useMusicSettings } from '@/state/selectors';
import { SearchField, Text, TOUCH, space, useTheme } from '@/ui';

import { ResultRow } from './ResultRow';
import { SectionNote } from './SectionHeader';
import { SourceChips, sourceLabel } from './SourceChips';
import { GUTTER } from './TrackRow';

export interface MusicSearchProps {
  serverKey: string;
  channelId: string;
  onCancel: () => void;
}

type Found =
  | { state: 'idle' }
  | { state: 'searching' }
  | { state: 'done'; query: string; results: MusicSearchResultRecord[] }
  | { state: 'failed'; message: string };

export function MusicSearch({ serverKey, channelId, onCancel }: MusicSearchProps) {
  const { colors } = useTheme();
  const info = useChannelInfo(serverKey, channelId);
  const settings = useMusicSettings(serverKey);

  const canAdd = info?.access.musicAdd ?? false;
  const canManageQueue = info?.access.musicManageQueue ?? false;
  const sources = settings?.availableSources ?? [];
  const [source, setSource] = useState(settings?.defaultSource ?? sources[0] ?? 'youtube');
  const [query, setQuery] = useState('');
  const [found, setFound] = useState<Found>({ state: 'idle' });
  const [added, setAdded] = useState<ReadonlySet<string>>(new Set());

  const run = useCallback(
    async (text: string, from: string) => {
      if (text.trim() === '') return;
      setFound({ state: 'searching' });
      try {
        const results = await search(serverKey, channelId, text.trim(), from);
        setFound({ state: 'done', query: text.trim(), results });
      } catch (error: unknown) {
        setFound({ state: 'failed', message: error instanceof Error ? error.message : 'Search failed' });
      }
    },
    [serverKey, channelId],
  );

  const chooseSource = (next: string) => {
    setSource(next);
    if (found.state === 'done') void run(query, next);
  };

  const add = useCallback(
    (resultId: string, placement: MusicPlacement, what: string) => {
      void Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Light);
      report(
        enqueue(serverKey, channelId, [resultId], [], undefined, placement).then(() => {
          setAdded((current) => new Set(current).add(resultId));
          AccessibilityInfo.announceForAccessibility('Added to the queue');
        }),
        what,
      );
    },
    [serverKey, channelId],
  );

  return (
    <View>
      <View style={styles.field}>
        <View style={styles.grow}>
          <SearchField
            value={query}
            onChangeText={setQuery}
            onSubmit={() => void run(query, source)}
            placeholder="Search songs or paste a link"
            autoFocus
            onSheet
          />
        </View>
        <Touchable onPress={onCancel} accessible accessibilityRole="button" accessibilityLabel="Cancel" activeOpacity={0.5} style={styles.cancel}>
          <Text variant="body" color="textSecondary">
            Cancel
          </Text>
        </Touchable>
      </View>
      <SourceChips sources={sources} selected={source} onSelect={chooseSource} />

      {found.state === 'idle' && (
        <SectionNote title={`Search ${sourceLabel(source)}`} body="Or paste a link to a song or playlist." />
      )}
      {found.state === 'searching' && (
        <View style={styles.busy}>
          <ActivityIndicator color={colors.textSecondary} />
        </View>
      )}
      {found.state === 'failed' && (
        <View style={styles.failed}>
          <Text variant="callout" color="recording">
            {found.message}
          </Text>
        </View>
      )}
      {found.state === 'done' && found.results.length === 0 && (
        <SectionNote title={`No results for “${found.query}”`} body="Try other words, or another source." />
      )}
      {found.state === 'done' &&
        found.results.map((result) => (
          <ResultRow
            key={result.resultId}
            serverKey={serverKey}
            result={result}
            added={added.has(result.resultId)}
            canAdd={canAdd}
            canManageQueue={canManageQueue}
            onAddEnd={() => add(result.resultId, MusicPlacement.End, 'add track to queue')}
            onPlayNext={() => add(result.resultId, MusicPlacement.Next, 'play track next')}
            onPlayNow={() => add(result.resultId, MusicPlacement.Now, 'play track now')}
          />
        ))}
    </View>
  );
}

const styles = StyleSheet.create({
  field: { flexDirection: 'row', alignItems: 'center', gap: space.md, paddingLeft: GUTTER, paddingRight: GUTTER - space.xs },
  grow: { flex: 1 },
  cancel: { minHeight: TOUCH, justifyContent: 'center', paddingHorizontal: space.xs },
  busy: { paddingVertical: space.xxxl, alignItems: 'center' },
  failed: { paddingHorizontal: GUTTER, paddingVertical: space.md },
});
