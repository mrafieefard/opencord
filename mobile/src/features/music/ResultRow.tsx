// A search result (spec §8.7): a tap adds it at the end, and its button
// turns into a tick once the server has it; a long press offers Play next
// and Play now with Manage queue.
import type { MusicSearchResultRecord } from '@opencord/core-native';
import { memo, useRef } from 'react';

import { IconButton, Menu, type MenuAction, type MenuHandle } from '@/ui';

import { TrackRow, trackLength } from './TrackRow';

export interface ResultRowProps {
  serverKey: string;
  result: MusicSearchResultRecord;
  added: boolean;
  canAdd: boolean;
  canManageQueue: boolean;
  onAddEnd: () => void;
  onPlayNext: () => void;
  onPlayNow: () => void;
}

const ACTIONS: MenuAction[] = [
  { id: 'play-next', title: 'Play next', icon: 'play' },
  { id: 'play-now', title: 'Play now', icon: 'play' },
];

export const ResultRow = memo(function ResultRow({
  serverKey,
  result,
  added,
  canAdd,
  canManageQueue,
  onAddEnd,
  onPlayNext,
  onPlayNow,
}: ResultRowProps) {
  const menu = useRef<MenuHandle>(null);
  const { track } = result;
  const add = () => {
    if (!added) onAddEnd();
  };

  return (
    <Menu
      ref={menu}
      testID={`search-result-menu-${result.resultId}`}
      trigger="longPress"
      actions={canManageQueue ? ACTIONS : []}
      onAction={(id) => (id === 'play-now' ? onPlayNow() : onPlayNext())}
    >
      <TrackRow
        serverKey={serverKey}
        track={track}
        onPress={canAdd ? add : undefined}
        onLongPress={canManageQueue ? () => menu.current?.open() : undefined}
        accessibilityLabel={`${track.title}, ${track.artist}, ${trackLength(track)}`}
        trailing={
          canAdd ? (
            <IconButton
              icon={added ? 'check' : 'add'}
              label={added ? `${track.title} is queued` : `Add ${track.title}`}
              size={36}
              onPress={add}
            />
          ) : undefined
        }
      />
    </Menu>
  );
});
