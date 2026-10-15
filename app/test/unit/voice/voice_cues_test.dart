import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/voice_cues.dart';

const _out = CueScene();
const _in = CueScene(serverKey: 's', channelId: 1, others: {2, 3});

CueScene _with({
  int channelId = 1,
  bool muted = false,
  bool deafened = false,
  Set<int> others = const {2, 3},
  Set<int> live = const {},
}) => CueScene(
  serverKey: 's',
  channelId: channelId,
  muted: muted,
  deafened: deafened,
  others: others,
  live: live,
);

void main() {
  test('joining, moving and leaving', () {
    expect(cuesBetween(_out, _in), [VoiceCue.join]);
    expect(cuesBetween(_in, _with(channelId: 2)), [VoiceCue.join]);
    expect(cuesBetween(_in, _out), [VoiceCue.leave]);
    expect(cuesBetween(_out, _out), isEmpty);
    expect(cuesBetween(_in, _in), isEmpty);
  });

  test('muting and deafening, deafen first when both change', () {
    expect(cuesBetween(_in, _with(muted: true)), [VoiceCue.mute]);
    expect(cuesBetween(_with(muted: true), _in), [VoiceCue.unmute]);
    expect(cuesBetween(_in, _with(muted: true, deafened: true)), [
      VoiceCue.deafen,
    ]);
    expect(cuesBetween(_with(muted: true, deafened: true), _in), [
      VoiceCue.undeafen,
    ]);
  });

  test('others coming, going and going live', () {
    expect(cuesBetween(_in, _with(others: {2, 3, 4})), [VoiceCue.userJoin]);
    expect(cuesBetween(_in, _with(others: {2})), [VoiceCue.userLeave]);
    expect(cuesBetween(_in, _with(live: {2})), [VoiceCue.streamStart]);
    expect(cuesBetween(_with(live: {2}), _in), [VoiceCue.streamStop]);
  });

  test('deafened, only your own changes are heard', () {
    final deaf = _with(deafened: true, muted: true);
    expect(
      cuesBetween(deaf, _with(deafened: true, muted: true, others: {2, 4})),
      isEmpty,
    );
  });
}
