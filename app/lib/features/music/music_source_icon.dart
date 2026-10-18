import 'package:flutter/material.dart';
import 'package:font_awesome_flutter/font_awesome_flutter.dart';

import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';

/// Returns the FontAwesome icon data for a music search provider.
FaIconData musicSourceFaIcon(String sourceKey) {
  final normalized = sourceKey.toLowerCase().replaceAll(
    RegExp(r'[^a-z0-9]'),
    '',
  );

  return switch (normalized) {
    'youtubemusic' || 'ytm' => FontAwesomeIcons.youtube,
    'youtube' || 'yt' => FontAwesomeIcons.youtube,
    'soundcloud' || 'sc' => FontAwesomeIcons.soundcloud,
    'bandcamp' => FontAwesomeIcons.bandcamp,
    'spotify' => FontAwesomeIcons.spotify,
    'applemusic' || 'apple' => FontAwesomeIcons.apple,
    'deezer' => FontAwesomeIcons.deezer,
    _ => FontAwesomeIcons.music,
  };
}

/// Returns standard [IconData] for a music search provider.
IconData musicSourceIconData(String sourceKey) =>
    musicSourceFaIcon(sourceKey).data;

/// Renders the FontAwesome brand icon for a music search provider
/// ('youtube_music', 'youtube', 'soundcloud', 'bandcamp', 'spotify',
/// 'applemusic', 'deezer').
class MusicSourceIcon extends StatelessWidget {
  const MusicSourceIcon(
    this.sourceKey, {
    super.key,
    this.size = 18,
    this.color,
  });

  final String sourceKey;
  final double size;
  final Color? color;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final normalized = sourceKey.toLowerCase().replaceAll(
      RegExp(r'[^a-z0-9]'),
      '',
    );

    switch (normalized) {
      case 'youtubemusic' || 'ytm':
      case 'youtube' || 'yt':
      case 'soundcloud' || 'sc':
      case 'bandcamp':
      case 'spotify':
      case 'applemusic' || 'apple':
      case 'deezer':
        return FaIcon(
          musicSourceFaIcon(sourceKey),
          size: size,
          color: color ?? colors.text,
        );
      default:
        return Icon(OcIcons.musicNote, size: size, color: color ?? colors.text);
    }
  }
}
