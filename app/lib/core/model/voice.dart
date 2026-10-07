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
