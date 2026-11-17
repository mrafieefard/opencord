import { VoiceCueKind } from '@opencord/core-native';
import { cuesBetween, playVoiceCues, type CueScene } from '../voiceCues';

const _out: CueScene = {};
const _in: CueScene = { serverKey: 's', channelId: '1', others: new Set(['2', '3']) };

const _with = ({
  channelId = '1',
  muted = false,
  deafened = false,
  others = new Set(['2', '3']),
  live = new Set<string>(),
}: Partial<CueScene> = {}): CueScene => ({
  serverKey: 's',
  channelId,
  muted,
  deafened,
  others,
  live,
});

describe('voice cues parity table', () => {
  // Fails if joining, moving or leaving produces the wrong cue or order.
  it('joining, moving and leaving', () => {
    expect(cuesBetween(_out, _in)).toEqual([VoiceCueKind.Join]);
    expect(cuesBetween(_in, _with({ channelId: '2' }))).toEqual([VoiceCueKind.Join]);
    expect(cuesBetween(_in, _out)).toEqual([VoiceCueKind.Leave]);
    expect(cuesBetween(_out, _out)).toEqual([]);
    expect(cuesBetween(_in, _in)).toEqual([]);
  });

  // Fails if mute/unmute/deafen/undeafen order does not prioritize deafen.
  it('muting and deafening, deafen first when both change', () => {
    expect(cuesBetween(_in, _with({ muted: true }))).toEqual([VoiceCueKind.Mute]);
    expect(cuesBetween(_with({ muted: true }), _in)).toEqual([VoiceCueKind.Unmute]);
    expect(cuesBetween(_in, _with({ muted: true, deafened: true }))).toEqual([VoiceCueKind.Deafen]);
    expect(cuesBetween(_with({ muted: true, deafened: true }), _in)).toEqual([VoiceCueKind.Undeafen]);
  });

  // Fails if changes in others or live streams in the channel do not trigger cues.
  it('others coming, going and going live', () => {
    expect(cuesBetween(_in, _with({ others: new Set(['2', '3', '4']) }))).toEqual([VoiceCueKind.UserJoin]);
    expect(cuesBetween(_in, _with({ others: new Set(['2']) }))).toEqual([VoiceCueKind.UserLeave]);
    expect(cuesBetween(_in, _with({ live: new Set(['2']) }))).toEqual([VoiceCueKind.StreamStart]);
    expect(cuesBetween(_with({ live: new Set(['2']) }), _in)).toEqual([VoiceCueKind.StreamStop]);
  });

  // Fails if a deafened user misses sounds from others in the same channel.
  it('deafened, others coming, going and going live are still heard', () => {
    const deaf = _with({ deafened: true, muted: true });
    expect(cuesBetween(deaf, _with({ deafened: true, muted: true, others: new Set(['2', '3', '4']) }))).toEqual([VoiceCueKind.UserJoin]);
    expect(cuesBetween(deaf, _with({ deafened: true, muted: true, others: new Set(['2']) }))).toEqual([VoiceCueKind.UserLeave]);
    expect(cuesBetween(deaf, _with({ deafened: true, muted: true, live: new Set(['2']) }))).toEqual([VoiceCueKind.StreamStart]);
  });

  // Fails if a cue group turned off in settings still plays its sounds.
  it('a group turned off plays nothing', () => {
    const played: VoiceCueKind[] = [];
    const play = (cue: VoiceCueKind) => played.push(cue);

    // With joinLeave turned off, joining plays nothing
    playVoiceCues(_out, _in, ['joinLeave'], play);
    expect(played).toEqual([]);

    // With joinLeave enabled, joining plays Join
    playVoiceCues(_out, _in, [], play);
    expect(played).toEqual([VoiceCueKind.Join]);
  });
});
