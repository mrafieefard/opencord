// Search section for music (spec §8.7): search field, source menu,
// search only on submit, result rows with Local pill, tap to add, long press options.
import { MusicPlacement, type MusicSearchResultRecord } from '@opencord/core-native';
import * as Haptics from 'expo-haptics';
import { useCallback, useState } from 'react';
import { StyleSheet, View } from 'react-native';

import { report } from '@/core';
import { enqueue, search } from '@/state/actions/music';
import { toast } from '@/state/actions/ui';
import { useChannelInfo, useMusicSettings } from '@/state/selectors';
import { SearchField, Text, space } from '@/ui';

import { ResultRow } from './ResultRow';
import { SourceMenu } from './SourceMenu';

export interface MusicSearchProps {
  serverKey: string;
  channelId: string;
}

export function MusicSearch({ serverKey, channelId }: MusicSearchProps) {
  const info = useChannelInfo(serverKey, channelId);
  const settings = useMusicSettings(serverKey);

  const canAdd = info?.access.musicAdd ?? false;
  const canManageQueue = info?.access.musicManageQueue ?? false;

  const sources = settings?.availableSources ?? ['youtube', 'soundcloud'];
  const defaultSource = settings?.defaultSource ?? sources[0] ?? 'youtube';

  const [query, setQuery] = useState('');
  const [selectedSource, setSelectedSource] = useState(defaultSource);
  const [results, setResults] = useState<MusicSearchResultRecord[]>([]);
  const [error, setError] = useState<string | null>(null);

  const handleSearch = useCallback(async () => {
    setError(null);
    try {
      const found = await search(serverKey, channelId, query, selectedSource);
      setResults(found);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : 'Search failed';
      setError(msg);
    }
  }, [serverKey, channelId, query, selectedSource]);

  const handleAddEnd = useCallback(
    async (resultId: string) => {
      await Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Light);
      report(
        enqueue(
          serverKey,
          channelId,
          [resultId],
          [],
          undefined,
          MusicPlacement.End,
        ),
        'add track to queue',
      );
      toast('Added to the queue');
    },
    [serverKey, channelId],
  );

  const handlePlayNext = useCallback(
    (resultId: string) => {
      report(
        enqueue(
          serverKey,
          channelId,
          [resultId],
          [],
          undefined,
          MusicPlacement.Next,
        ),
        'play track next',
      );
    },
    [serverKey, channelId],
  );

  const handlePlayNow = useCallback(
    (resultId: string) => {
      report(
        enqueue(
          serverKey,
          channelId,
          [resultId],
          [],
          undefined,
          MusicPlacement.Now,
        ),
        'play track now',
      );
    },
    [serverKey, channelId],
  );

  return (
    <View style={styles.container}>
      <View style={styles.searchRow}>
        <View style={styles.fieldWrapper}>
          <SearchField
            value={query}
            onChangeText={setQuery}
            onSubmit={handleSearch}
            placeholder="Search songs or paste a link"
          />
        </View>
        {sources.length > 1 ? (
          <SourceMenu
            sources={sources}
            selectedSource={selectedSource}
            onSelectSource={setSelectedSource}
          />
        ) : null}
      </View>

      {error ? (
        <View style={styles.errorContainer}>
          <Text variant="callout" color="recording">
            {error}
          </Text>
        </View>
      ) : null}

      <View style={styles.resultsList}>
        {results.map((result) => (
          <ResultRow
            key={result.resultId}
            serverKey={serverKey}
            result={result}
            canAdd={canAdd}
            canManageQueue={canManageQueue}
            onAddEnd={() => handleAddEnd(result.resultId)}
            onPlayNext={() => handlePlayNext(result.resultId)}
            onPlayNow={() => handlePlayNow(result.resultId)}
          />
        ))}
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    width: '100%',
  },
  searchRow: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: space.sm,
    paddingHorizontal: space.md,
    paddingBottom: space.sm,
  },
  fieldWrapper: {
    flex: 1,
  },
  errorContainer: {
    paddingHorizontal: space.md,
    paddingVertical: space.sm,
  },
  resultsList: {
    width: '100%',
  },
});
