import 'dart:async';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/rust/core_api.dart';
import 'package:opencord/src/rust/api/types.dart' as core;

/// The Rust core's calls, recorded and answered from fields the test sets.
class FakeCoreApi extends Fake implements CoreApi {
  final events = StreamController<core.CoreEvent>.broadcast();
  final media = StreamController<core.MediaEvent>.broadcast();

  /// What audioDevices answers.
  var devices = const core.AudioDevices(inputs: [], outputs: []);

  /// What audioApplySettings was last sent.
  core.AudioSettings? audioSettings;

  /// What audioRecommendedNoiseSuppression answers.
  var recommended = core.NoiseSuppressionMode.standard;
  final calls = <String>[];
  var servers = <core.Server>[];
  var identityInfo = const core.IdentityInfo(
    publicKeyHex: 'abcd',
    fingerprint: 'ABCD-EFGH-IJKL-MNOP',
  );

  /// Per channel, newest first.
  final history = <int, List<core.Message>>{};

  /// Holds fetchMessages until completed, when set.
  Completer<void>? fetchGate;

  /// Holds only fetches of older pages (with `before`), when set.
  Completer<void>? olderGate;

  /// Thrown by fetchMessages, when set: something the core never says.
  Exception? fetchFailure;

  /// Thrown by fetchMessages, when set, as the core would.
  core.CoreError? fetchError;
  core.CoreError? sendError;
  core.AddServerOutcome? addOutcome;

  void emit(String server, core.CoreEventPayload payload) =>
      events.add(core.CoreEvent(serverKey: server, payload: payload));

  @override
  Stream<core.CoreEvent> eventStream() => events.stream;

  @override
  Stream<core.MediaEvent> mediaEventStream() => media.stream;

  @override
  Future<core.AudioDevices> audioDevices() async => devices;

  @override
  void audioApplySettings(core.AudioSettings settings) =>
      audioSettings = settings;

  @override
  void voiceSetPushToTalk(bool held) => calls.add('pushToTalk:$held');

  @override
  void voiceSetPrioritySpeaker(bool held) => calls.add('priority:$held');

  @override
  void audioSetLevelMeter(bool enabled) => calls.add('levelMeter:$enabled');

  @override
  Future<void> audioMicTest(bool enabled) async =>
      calls.add('micTest:$enabled');

  @override
  Future<core.NoiseSuppressionMode> audioRecommendedNoiseSuppression() async =>
      recommended;

  @override
  void voiceSetUserVolume(String serverKey, int userId, double volume) =>
      calls.add('userVolume:$serverKey:$userId:$volume');

  @override
  void voiceSetUserLocalMute(String serverKey, int userId, bool muted) =>
      calls.add('localMute:$serverKey:$userId:$muted');

  @override
  core.GeneratedIdentity identityGenerate() => core.GeneratedIdentity(
    secret: Uint8List.fromList([1, 2, 3]),
    info: identityInfo,
  );

  @override
  core.IdentityInfo identityCheck(List<int> secret, String displayName) {
    if (displayName.isEmpty) {
      throw const core.CoreError.invalidInput(message: 'Choose a name.');
    }
    return identityInfo;
  }

  @override
  core.IdentityInfo identityLoad(List<int> secret, String displayName) {
    calls.add('identityLoad:$displayName');
    return identityCheck(secret, displayName);
  }

  @override
  String identityBackupEncode(List<int> secret) => 'backup:${secret.join(',')}';

  @override
  Uint8List identityBackupDecode(String backup) {
    if (!backup.startsWith('backup:')) {
      throw const core.CoreError.invalidInput(message: 'Not a backup.');
    }
    return Uint8List.fromList([
      for (final part in backup.substring(7).split(',')) int.parse(part),
    ]);
  }

  @override
  List<core.Server> serversList() => servers;

  @override
  Future<core.AddServerOutcome> serverAdd(
    String linkOrAddress,
    String? claimToken,
  ) async {
    calls.add('add:$linkOrAddress');
    final outcome = addOutcome!;
    if (outcome case core.AddServerOutcome_Added(:final field0)) {
      servers = [...servers, field0];
    }
    return outcome;
  }

  @override
  Future<void> serverRetryNow(String serverKey) async =>
      calls.add('retry:$serverKey');

  @override
  Future<List<core.Message>> fetchMessages(
    String serverKey,
    int channelId,
    int? before,
    int limit,
  ) async {
    calls.add('fetch:$channelId');
    await fetchGate?.future;
    if (before != null) await olderGate?.future;
    if (fetchFailure case final failure?) throw failure;
    if (fetchError case final error?) throw error;
    return (history[channelId] ?? const [])
        .where((message) => before == null || message.id < before)
        .take(limit)
        .toList();
  }

  @override
  Future<core.Message> sendMessage(
    String serverKey,
    int channelId,
    String content,
    String nonce,
  ) async {
    if (sendError case final error?) throw error;
    return core.Message(
      id: 900,
      channelId: channelId,
      authorId: 1,
      content: content,
      createdAtMs: 0,
      nonce: nonce,
    );
  }

  @override
  Future<core.Channel> createChannel(
    String serverKey,
    core.ChannelKind kind,
    String name,
    String? topic,
    int? parentId,
  ) async {
    calls.add('createChannel:$name');
    return core.Channel(
      bitrate: 0,
      userLimit: 0,
      textInVoice: false,
      id: 77,
      kind: kind,
      name: name,
      topic: topic,
      parentId: parentId,
      position: 0,
      overwrites: const [],
    );
  }

  /// Thrown by setChannelOverwrite, when set.
  core.CoreError? overwriteError;

  /// Thrown by updateProfile for these servers.
  final profileErrors = <String, core.CoreError>{};

  @override
  Future<core.Channel> setChannelOverwrite(
    String serverKey,
    int channelId,
    core.PermissionOverwrite overwrite,
  ) async {
    calls.add(
      'overwrite:$channelId:${overwrite.targetKind.name}:${overwrite.targetId}'
      ':allow=${overwrite.allow}:deny=${overwrite.deny}',
    );
    if (overwriteError case final error?) throw error;
    return core.Channel(
      bitrate: 0,
      userLimit: 0,
      textInVoice: false,
      id: channelId,
      kind: core.ChannelKind.text,
      name: 'private',
      position: 0,
      overwrites: [overwrite],
    );
  }

  /// Thrown by voiceJoin, when set.
  core.CoreError? joinError;

  /// What updateVoiceSettings was last sent.
  core.VoiceSettingsChanges? voiceChanges;

  @override
  Future<core.VoiceState> voiceJoin(String serverKey, int channelId) async {
    calls.add('voiceJoin:$serverKey:$channelId');
    if (joinError case final error?) throw error;
    return voiceState(1, channelId);
  }

  @override
  Future<void> voiceLeave() async => calls.add('voiceLeave');

  @override
  void voiceSetSelfMute(bool muted) => calls.add('selfMute:$muted');

  @override
  void voiceSetSelfDeaf(bool deafened) => calls.add('selfDeaf:$deafened');

  @override
  Future<core.VoiceSettings> updateVoiceSettings(
    String serverKey,
    core.VoiceSettingsChanges changes,
  ) async {
    calls.add('voiceSettings:$serverKey');
    voiceChanges = changes;
    return coreVoiceSettings;
  }

  /// Thrown by deleteChannel, when set.
  core.CoreError? deleteError;

  @override
  Future<void> deleteChannel(String serverKey, int channelId) async {
    calls.add('deleteChannel:$channelId');
    if (deleteError case final error?) throw error;
  }

  @override
  Future<core.User> updateProfile(String serverKey, String displayName) async {
    calls.add('profile:$serverKey:$displayName');
    if (profileErrors[serverKey] case final error?) throw error;
    return core.User(
      id: 1,
      publicKeyHex: identityInfo.publicKeyHex,
      fingerprint: identityInfo.fingerprint,
      displayName: displayName,
    );
  }

  @override
  Future<void> updatePresence(
    String serverKey,
    core.PresenceStatus status,
  ) async => calls.add('presence:$serverKey:${status.name}');
}

/// The server's default voice settings, as the core reports them.
const coreVoiceSettings = core.VoiceSettings(
  screenShareMaxResolution: core.ScreenShareResolution.p720,
  screenShareMaxFps: 30,
  maxStreamViewers: 50,
  cameraAllowed: true,
  maxCameraParticipants: 25,
  maxVoiceBitrate: 96000,
  afkTimeoutS: 300,
  soundboardEnabled: true,
  allowDefaultSounds: true,
  allowExternalSounds: true,
  soundCooldownS: 3,
  maxSounds: 48,
);

/// Someone in voice, as the core reports them; [channelId] null has left.
core.VoiceState voiceState(
  int userId,
  int? channelId, {
  bool thisDevice = true,
  bool selfMute = false,
  bool serverMute = false,
  bool suppress = false,
}) => core.VoiceState(
  userId: userId,
  channelId: channelId,
  thisDevice: thisDevice,
  selfMute: selfMute,
  selfDeaf: false,
  serverMute: serverMute,
  serverDeaf: false,
  suppress: suppress,
  selfVideo: false,
  selfStream: false,
);
