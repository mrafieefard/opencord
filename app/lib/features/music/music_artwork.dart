import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';

/// Which picture, from which server, at which size the server makes.
typedef ArtworkRef = ({String server, String id, int size});

/// Files fetched lately; the least recently shown go first (§0.1).
final _paths = <ArtworkRef, String?>{};
const _pathsKept = 200;

/// The file of a track's artwork on this device, fetched through the
/// server's proxy (Phase 3 plan §15.4); null when there is none.
final musicArtworkPathProvider = FutureProvider.autoDispose
    .family<String?, ArtworkRef>((ref, artwork) async {
      if (_paths.containsKey(artwork)) {
        final path = _paths.remove(artwork);
        return _paths[artwork] = path;
      }
      final path = await ref
          .read(repositoryProvider)
          .musicArtwork(artwork.server, artwork.id, artwork.size);
      _paths[artwork] = path;
      if (_paths.length > _pathsKept) _paths.remove(_paths.keys.first);
      return path;
    });

/// A track's artwork, square or rectangular, cropped to fill, decoded at the
/// size the server made it: 128 px for lists, 512 for the cover (§0.1). Without
/// one, a monochrome music note.
class MusicArtwork extends ConsumerWidget {
  const MusicArtwork({
    super.key,
    required this.serverKey,
    required this.artworkId,
    this.size = 128,
    this.width,
    this.height,
    this.radius = 6,
    this.placeholderColor,
    this.placeholderIconSize,
  });

  final String serverKey;
  final String artworkId;

  /// Logical pixels for square sizing when width/height are not set.
  final double size;
  final double? width;
  final double? height;
  final double radius;
  final Color? placeholderColor;
  final double? placeholderIconSize;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final w = width ?? size;
    final h = height ?? size;
    final decode = (w > 128 || h > 128) ? 512 : 128;
    final path = artworkId.isEmpty
        ? null
        : ref
              .watch(
                musicArtworkPathProvider((
                  server: serverKey,
                  id: artworkId,
                  size: decode,
                )),
              )
              .value;
    return Container(
      width: width ?? size,
      height: height ?? size,
      decoration: BoxDecoration(
        color: placeholderColor ?? colors.hover,
        border: Border.all(color: colors.border),
        borderRadius: BorderRadius.circular(radius),
      ),
      clipBehavior: Clip.antiAlias,
      child: path == null
          ? Center(
              child: Icon(
                OcIcons.musicNote,
                size: placeholderIconSize ?? (h * 0.42),
                color: colors.textMuted,
              ),
            )
          : Image(
              image: ResizeImage(
                FileImage(File(path)),
                width: decode,
                height: decode,
                policy: ResizeImagePolicy.fit,
              ),
              fit: BoxFit.cover,
              gaplessPlayback: true,
              excludeFromSemantics: true,
              errorBuilder: (_, _, _) => Center(
                child: Icon(
                  OcIcons.musicNote,
                  size: placeholderIconSize ?? (h * 0.42),
                  color: colors.textMuted,
                ),
              ),
            ),
    );
  }
}
