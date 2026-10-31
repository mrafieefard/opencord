import {
  MusicPlayState,
  MusicRepeat,
  type MusicSessionRecord,
} from '@opencord/core-native';

import { positionAt } from '../position';

function makeSession(overrides: Partial<MusicSessionRecord> = {}): MusicSessionRecord {
  return {
    channelId: '101',
    version: 1,
    state: MusicPlayState.Playing,
    nowPlaying: {
      itemId: 'item-1',
      track: {
        title: 'Song',
        artist: 'Artist',
        durationMs: 60_000,
        isStream: false,
        isSeekable: true,
        source: 'youtube',
        sourceDisplayName: 'YouTube',
        uri: 'https://example.com/song',
        artworkId: 'art-1',
        removable: true,
      },
      requesterId: '1001',
      addedAtMs: 1_700_000_000_000,
    },
    queue: [],
    history: [],
    positionMs: 10_000,
    anchorMs: 1_700_000_000_000,
    repeat: MusicRepeat.Off,
    shuffle: false,
    waitingForNode: false,
    pausedEmpty: false,
    skipVotes: [],
    skipVotesNeeded: 0,
    ...overrides,
  };
}

describe('positionAt', () => {
  it('positionAt matches the desktop: paused', () => {
    const session = makeSession({
      state: MusicPlayState.Paused,
      positionMs: 10_000,
      anchorMs: 1_700_000_000_000,
    });
    // Even if now is 30s after anchor, paused stays at positionMs
    expect(positionAt(session, 1_700_000_030_000)).toBe(10_000);
  });

  it('positionAt matches the desktop: playing after the anchor', () => {
    const session = makeSession({
      state: MusicPlayState.Playing,
      positionMs: 10_000,
      anchorMs: 1_700_000_000_000,
    });
    // 5s after anchor: 10s + 5s = 15s
    expect(positionAt(session, 1_700_000_005_000)).toBe(15_000);
  });

  it('positionAt matches the desktop: before the anchor', () => {
    const session = makeSession({
      state: MusicPlayState.Playing,
      positionMs: 10_000,
      anchorMs: 1_700_000_000_000,
    });
    // If now is before anchor, it does not subtract; stays at positionMs
    expect(positionAt(session, 1_699_999_995_000)).toBe(10_000);
  });

  it('positionAt matches the desktop: past the end (capped)', () => {
    const session = makeSession({
      state: MusicPlayState.Playing,
      positionMs: 10_000,
      anchorMs: 1_700_000_000_000,
    });
    // 5 minutes after anchor: would be 310s, capped at duration (60s)
    expect(positionAt(session, 1_700_000_300_000)).toBe(60_000);
  });

  it('positionAt matches the desktop: a stream of length 0', () => {
    const session = makeSession({
      state: MusicPlayState.Playing,
      positionMs: 10_000,
      anchorMs: 1_700_000_000_000,
      nowPlaying: {
        itemId: 'item-stream',
        track: {
          title: 'Live Radio',
          artist: 'Radio',
          durationMs: 0,
          isStream: true,
          isSeekable: false,
          source: 'radio',
          sourceDisplayName: 'Radio',
          uri: 'https://example.com/radio',
          artworkId: '',
          removable: true,
        },
        requesterId: '1001',
        addedAtMs: 1_700_000_000_000,
      },
    });
    // 15s after anchor: 10s + 15s = 25s, not capped to 0
    expect(positionAt(session, 1_700_000_015_000)).toBe(25_000);
  });
});
