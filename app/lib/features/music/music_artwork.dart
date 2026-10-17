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

/// A track's artwork, square, cropped to fill, decoded at the size the
/// server made it: 128 px for lists, 512 for the cover (§0.1). Without one,
/// a monochrome music note.
class MusicArtwork extends ConsumerWidget {
  const MusicArtwork({
    super.key,
    required this.serverKey,
    required this.artworkId,
    required this.size,
    this.radius = 6,
  });

  final String serverKey;
  final String artworkId;

  /// Logical pixels.
  final double size;
  final double radius;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final decode = size > 128 ? 512 : 128;
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
    final placeholder = DecoratedBox(
      decoration: BoxDecoration(
        color: colors.hover,
        border: Border.all(color: colors.border),
        borderRadius: BorderRadius.circular(radius),
      ),
      child: Center(
        child: Icon(
          OcIcons.musicNote,
          size: size * 0.42,
          color: colors.textMuted,
        ),
      ),
    );
    return ClipRRect(
      borderRadius: BorderRadius.circular(radius),
      child: SizedBox.square(
        dimension: size,
        child: path == null
            ? placeholder
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
                errorBuilder: (_, _, _) => placeholder,
              ),
      ),
    );
  }
}
