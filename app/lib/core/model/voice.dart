import 'dart:math' as math;

import 'package:flutter/foundation.dart';

/// A screen share preset: a maximum pixel count (Phase 2 plan §9.2).
enum ScreenShareResolution {
  p480('480p'),
  p720('720p'),
  p1080('1080p'),
  p1440('1440p'),
  source('Source');

  const ScreenShareResolution(this.label);

  final String label;
}

/// Server-wide voice, video and soundboard settings (Phase 2 plan §5.2,
/// §11.7).
@immutable
class VoiceSettings {
  const VoiceSettings({
    this.screenShareMaxResolution = ScreenShareResolution.p720,
    this.screenShareMaxFps = 30,
    this.maxStreamViewers = 50,
    this.cameraAllowed = true,
    this.maxCameraParticipants = 25,
    this.maxVoiceBitrate = 96000,
    this.afkChannelId,
    this.afkTimeout = const Duration(minutes: 5),
    this.soundboardEnabled = true,
    this.allowDefaultSounds = true,
    this.allowExternalSounds = true,
    this.soundCooldown = const Duration(seconds: 3),
    this.maxSounds = 48,
  });

  static const fpsChoices = [15, 30, 60];
  static const afkTimeoutChoices = [
    Duration(minutes: 1),
    Duration(minutes: 5),
    Duration(minutes: 15),
    Duration(minutes: 30),
    Duration(minutes: 60),
  ];
  static const minVoiceBitrate = 32000;
  static const maxVoiceBitrateLimit = 256000;
  static const maxStreamViewersLimit = 200;
  static const maxCameraParticipantsLimit = 50;

  /// The soundboard (Phase 2 plan §11.2): a cooldown of 0–30 s, room for
  /// 0–200 sounds.
  static const maxSoundCooldown = Duration(seconds: 30);
  static const maxSoundsLimit = 200;

  final ScreenShareResolution screenShareMaxResolution;
  final int screenShareMaxFps;
  final int maxStreamViewers;
  final bool cameraAllowed;
  final int maxCameraParticipants;

  /// Bits per second; caps every voice channel's bitrate.
  final int maxVoiceBitrate;

  /// Where idle people are moved; null for none.
  final int? afkChannelId;
  final Duration afkTimeout;
  final bool soundboardEnabled;
  final bool allowDefaultSounds;
  final bool allowExternalSounds;
  final Duration soundCooldown;
  final int maxSounds;

  VoiceSettings copyWith({
    ScreenShareResolution? screenShareMaxResolution,
    int? screenShareMaxFps,
    int? maxStreamViewers,
    bool? cameraAllowed,
    int? maxCameraParticipants,
    int? maxVoiceBitrate,
    int? Function()? afkChannelId,
    Duration? afkTimeout,
    bool? soundboardEnabled,
    bool? allowDefaultSounds,
    bool? allowExternalSounds,
    Duration? soundCooldown,
    int? maxSounds,
  }) => VoiceSettings(
    screenShareMaxResolution:
        screenShareMaxResolution ?? this.screenShareMaxResolution,
    screenShareMaxFps: screenShareMaxFps ?? this.screenShareMaxFps,
    maxStreamViewers: maxStreamViewers ?? this.maxStreamViewers,
    cameraAllowed: cameraAllowed ?? this.cameraAllowed,
    maxCameraParticipants: maxCameraParticipants ?? this.maxCameraParticipants,
    maxVoiceBitrate: maxVoiceBitrate ?? this.maxVoiceBitrate,
    afkChannelId: afkChannelId == null ? this.afkChannelId : afkChannelId(),
    afkTimeout: afkTimeout ?? this.afkTimeout,
    soundboardEnabled: soundboardEnabled ?? this.soundboardEnabled,
    allowDefaultSounds: allowDefaultSounds ?? this.allowDefaultSounds,
    allowExternalSounds: allowExternalSounds ?? this.allowExternalSounds,
    soundCooldown: soundCooldown ?? this.soundCooldown,
    maxSounds: maxSounds ?? this.maxSounds,
  );

  @override
  bool operator ==(Object other) =>
      other is VoiceSettings &&
      other.screenShareMaxResolution == screenShareMaxResolution &&
      other.screenShareMaxFps == screenShareMaxFps &&
      other.maxStreamViewers == maxStreamViewers &&
      other.cameraAllowed == cameraAllowed &&
      other.maxCameraParticipants == maxCameraParticipants &&
      other.maxVoiceBitrate == maxVoiceBitrate &&
      other.afkChannelId == afkChannelId &&
      other.afkTimeout == afkTimeout &&
      other.soundboardEnabled == soundboardEnabled &&
      other.allowDefaultSounds == allowDefaultSounds &&
      other.allowExternalSounds == allowExternalSounds &&
      other.soundCooldown == soundCooldown &&
      other.maxSounds == maxSounds;

  @override
  int get hashCode => Object.hash(
    screenShareMaxResolution,
    screenShareMaxFps,
    maxStreamViewers,
    cameraAllowed,
    maxCameraParticipants,
    maxVoiceBitrate,
    afkChannelId,
    afkTimeout,
    soundboardEnabled,
    allowDefaultSounds,
    allowExternalSounds,
    soundCooldown,
    maxSounds,
  );
}

/// Where this device's voice connection is (Phase 2 plan §7.14), as the
/// voice panel names it.
enum VoiceConnectionPhase {
  awaitingEndpoint('Awaiting endpoint'),
  authenticating('Authenticating'),
  rtcConnecting('RTC connecting'),
  connected('Voice connected'),
  reconnecting('Reconnecting'),
  noRoute('No route'),
  disconnected('Voice disconnected');

  const VoiceConnectionPhase(this.label);

  final String label;
}

@immutable
class VoiceConnectionStatus {
  const VoiceConnectionStatus(this.phase, {this.reason});

  final VoiceConnectionPhase phase;

  /// Why it disconnected, in the core's words.
  final String? reason;

  /// More about the phase, where there is something to say.
  String? get detail => switch (phase) {
    VoiceConnectionPhase.noRoute =>
      "UDP port 7711 may be blocked by your network or the server's firewall",
    VoiceConnectionPhase.disconnected => reason,
    _ => null,
  };

  @override
  bool operator ==(Object other) =>
      other is VoiceConnectionStatus &&
      other.phase == phase &&
      other.reason == reason;

  @override
  int get hashCode => Object.hash(phase, reason);

  @override
  String toString() => 'VoiceConnectionStatus($phase, $reason)';
}

/// A microphone or speaker the system offers.
@immutable
class AudioDevice {
  const AudioDevice({required this.id, required this.name});

  /// What settings keep.
  final String id;
  final String name;

  @override
  bool operator ==(Object other) =>
      other is AudioDevice && other.id == id && other.name == name;

  @override
  int get hashCode => Object.hash(id, name);
}

@immutable
class AudioDeviceList {
  const AudioDeviceList({
    this.inputs = const [],
    this.outputs = const [],
    this.defaultInput,
    this.defaultOutput,
  });

  final List<AudioDevice> inputs;
  final List<AudioDevice> outputs;

  /// The system's default devices' ids.
  final String? defaultInput;
  final String? defaultOutput;

  @override
  bool operator ==(Object other) =>
      other is AudioDeviceList &&
      listEquals(other.inputs, inputs) &&
      listEquals(other.outputs, outputs) &&
      other.defaultInput == defaultInput &&
      other.defaultOutput == defaultOutput;

  @override
  int get hashCode => Object.hash(
    Object.hashAll(inputs),
    Object.hashAll(outputs),
    defaultInput,
    defaultOutput,
  );
}

/// What a hotkey does (Phase 2 plan §7.13).
enum HotkeyAction {
  pushToTalk('Push to talk'),
  prioritySpeaker('Priority speaker'),
  toggleMute('Toggle mute'),
  toggleDeafen('Toggle deafen'),
  toggleInputMode('Toggle push-to-talk');

  const HotkeyAction(this.label);

  final String label;

  /// Acts while held, rather than once per press.
  bool get held => this == pushToTalk || this == prioritySpeaker;
}

/// A key the system or the app watches for: [keys] as the XDG shortcuts
/// specification writes them (`CTRL+SHIFT+m`, `grave`). An action may have
/// several; it is down while any of them is.
typedef HotkeyBinding = ({HotkeyAction action, String keys});

/// Whether hotkeys work while Opencord is in the background.
@immutable
sealed class HotkeySupport {
  const HotkeySupport();

  /// System-wide, through [HotkeysGlobal.method].
  const factory HotkeySupport.global(String method) = HotkeysGlobal;

  /// Only while Opencord is focused, because of [HotkeysFocusedOnly.reason];
  /// [HotkeysFocusedOnly.settingsUrl] opens where the user can allow them.
  const factory HotkeySupport.focusedOnly(
    String reason, {
    String? settingsUrl,
  }) = HotkeysFocusedOnly;
}

final class HotkeysGlobal extends HotkeySupport {
  const HotkeysGlobal(this.method);

  final String method;

  @override
  bool operator ==(Object other) =>
      other is HotkeysGlobal && other.method == method;

  @override
  int get hashCode => method.hashCode;
}

final class HotkeysFocusedOnly extends HotkeySupport {
  const HotkeysFocusedOnly(this.reason, {this.settingsUrl});

  final String reason;

  /// The system settings where the user can allow global hotkeys, when
  /// that is what's missing (macOS's Input Monitoring). Opencord binds them
  /// again when its window is focused, so a grant takes effect.
  final String? settingsUrl;

  @override
  bool operator ==(Object other) =>
      other is HotkeysFocusedOnly &&
      other.reason == reason &&
      other.settingsUrl == settingsUrl;

  @override
  int get hashCode => Object.hash(reason, settingsUrl);
}

/// Noise suppression on the microphone (Phase 2 plan §7.3).
enum NoiseSuppression {
  off,

  /// RNNoise: very light.
  standard,

  /// DeepFilterNet: much better on keyboards, dogs and fans, heavier.
  high,
}

/// Audio choices, as voice media takes them.
@immutable
class AudioConfig {
  const AudioConfig({
    this.inputDevice,
    this.outputDevice,
    this.pushToTalk = false,
    this.pushToTalkRelease = const Duration(milliseconds: 200),
    this.automaticSensitivity = true,
    this.sensitivityDbfs = -45,
    this.echoCancellation = true,
    this.noiseSuppression = NoiseSuppression.standard,
    this.automaticGain = true,
    this.inputVolume = 100,
    this.outputVolume = 100,
    this.attenuationPercent = 0,
    this.attenuateWhenOthersSpeak = true,
    this.attenuateWhenISpeak = false,
  });

  /// A device id; null follows the system's default.
  final String? inputDevice;
  final String? outputDevice;
  final bool pushToTalk;

  /// How long push-to-talk keeps sending after the key is let go.
  final Duration pushToTalkRelease;

  /// Voice activity opens on a voice, whatever its level; otherwise at
  /// [sensitivityDbfs].
  final bool automaticSensitivity;
  final double sensitivityDbfs;
  final bool echoCancellation;
  final NoiseSuppression noiseSuppression;
  final bool automaticGain;

  /// 0–200 %.
  final int inputVolume;
  final int outputVolume;

  /// How much other apps are lowered while people talk, 0–100 %; 0 is
  /// off (Phase 2 §12).
  final int attenuationPercent;
  final bool attenuateWhenOthersSpeak;
  final bool attenuateWhenISpeak;

  @override
  bool operator ==(Object other) =>
      other is AudioConfig &&
      other.attenuationPercent == attenuationPercent &&
      other.attenuateWhenOthersSpeak == attenuateWhenOthersSpeak &&
      other.attenuateWhenISpeak == attenuateWhenISpeak &&
      other.inputDevice == inputDevice &&
      other.outputDevice == outputDevice &&
      other.pushToTalk == pushToTalk &&
      other.pushToTalkRelease == pushToTalkRelease &&
      other.automaticSensitivity == automaticSensitivity &&
      other.sensitivityDbfs == sensitivityDbfs &&
      other.echoCancellation == echoCancellation &&
      other.noiseSuppression == noiseSuppression &&
      other.automaticGain == automaticGain &&
      other.inputVolume == inputVolume &&
      other.outputVolume == outputVolume;

  @override
  int get hashCode => Object.hash(
    inputDevice,
    outputDevice,
    pushToTalk,
    pushToTalkRelease,
    automaticSensitivity,
    sensitivityDbfs,
    echoCancellation,
    noiseSuppression,
    automaticGain,
    inputVolume,
    outputVolume,
    attenuationPercent,
    attenuateWhenOthersSpeak,
    attenuateWhenISpeak,
  );

  @override
  String toString() =>
      'AudioConfig($inputDevice, $outputDevice, push-to-talk: $pushToTalk '
      '(${pushToTalkRelease.inMilliseconds} ms), automatic sensitivity: '
      '$automaticSensitivity ($sensitivityDbfs dBFS), echo cancellation: '
      '$echoCancellation, noise suppression: ${noiseSuppression.name}, '
      'automatic gain: $automaticGain, $inputVolume %, $outputVolume %, '
      'others lowered $attenuationPercent % (others: '
      '$attenuateWhenOthersSpeak, me: $attenuateWhenISpeak))';
}

/// A voice event's sound on this device (Phase 2 §12).
enum VoiceCue {
  /// This device joined a voice channel, or was moved to another.
  join,

  /// This device left, or was disconnected.
  leave,

  /// Someone else joined the channel this device is in.
  userJoin,

  /// Someone else left it.
  userLeave,
  mute,
  unmute,
  deafen,
  undeafen,

  /// Someone in the channel went live, this device included.
  streamStart,
  streamStop,
}

/// A voice connection over the last second (Phase 2 §12, connection
/// diagnostics).
@immutable
class VoiceStats {
  const VoiceStats({
    this.pingMs,
    this.jitterMs,
    this.lossInPercent,
    this.lossOutPercent,
    this.bitrateIn = 0,
    this.bitrateOut = 0,
    this.voiceNode = '',
    this.cameraEncoder,
    this.screenEncoder,
  });

  final int? pingMs;
  final double? jitterMs;
  final double? lossInPercent;
  final double? lossOutPercent;

  /// Bits per second, everything included.
  final int bitrateIn;
  final int bitrateOut;
  final String voiceNode;
  final String? cameraEncoder;
  final String? screenEncoder;

  /// Packet loss, the worse of the two ways; null before either is known.
  double? get lossPercent => switch ((lossInPercent, lossOutPercent)) {
    (null, null) => null,
    (final lossIn, final lossOut) => math.max(lossIn ?? 0, lossOut ?? 0),
  };

  /// Signal bars, 1–3: three for a call that sounds right, one for one
  /// that breaks up. Without a ping yet, three.
  int get bars {
    final ping = pingMs ?? 0;
    final loss = lossPercent ?? 0;
    if (ping >= 300 || loss >= 8) return 1;
    if (ping >= 150 || loss >= 2) return 2;
    return 3;
  }

  @override
  bool operator ==(Object other) =>
      other is VoiceStats &&
      other.pingMs == pingMs &&
      other.jitterMs == jitterMs &&
      other.lossInPercent == lossInPercent &&
      other.lossOutPercent == lossOutPercent &&
      other.bitrateIn == bitrateIn &&
      other.bitrateOut == bitrateOut &&
      other.voiceNode == voiceNode &&
      other.cameraEncoder == cameraEncoder &&
      other.screenEncoder == screenEncoder;

  @override
  int get hashCode => Object.hash(
    pingMs,
    jitterMs,
    lossInPercent,
    lossOutPercent,
    bitrateIn,
    bitrateOut,
    voiceNode,
    cameraEncoder,
    screenEncoder,
  );
}

/// Why recording a voice message stopped.
enum VoiceStopReason { maxLength, deviceLost }
