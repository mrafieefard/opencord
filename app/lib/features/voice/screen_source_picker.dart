import 'dart:async';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/stream.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/hoverable.dart';

/// How often the thumbnails are drawn again while the picker is open
/// (docs/plans/windows.md W6, step 8). Often enough that the pictures are
/// recognisably live, rarely enough that redrawing every window on the
/// desktop is not what the PC is mostly doing.
const _refresh = Duration(seconds: 2);

/// Screens and windows to pick from, where Opencord has its own picker
/// (Phase 2 plan §9.1, `RepoCapabilities.ownScreenPicker`) — on Windows
/// that is always, since it has no system picker worth putting in front of
/// this.
///
/// Shown inside the share dialog, which keeps the chosen id and leaves
/// "Go live" disabled until there is one.
class ScreenSourcePicker extends ConsumerStatefulWidget {
  const ScreenSourcePicker({
    super.key,
    required this.chosen,
    required this.onChosen,
  });

  /// The id chosen so far, from [ScreenSource.id].
  final String? chosen;
  final ValueChanged<String> onChosen;

  @override
  ConsumerState<ScreenSourcePicker> createState() => _ScreenSourcePickerState();
}

class _ScreenSourcePickerState extends ConsumerState<ScreenSourcePicker>
    with SingleTickerProviderStateMixin {
  late final TabController _tabs = TabController(length: 2, vsync: this);
  Timer? _timer;
  List<ScreenSource>? _sources;
  String? _problem;

  /// True while a refresh is in flight, so a slow desktop cannot stack them
  /// up behind each other.
  var _asking = false;

  @override
  void initState() {
    super.initState();
    unawaited(_ask());
    _timer = Timer.periodic(_refresh, (_) => unawaited(_ask()));
  }

  @override
  void dispose() {
    _timer?.cancel();
    _tabs.dispose();
    super.dispose();
  }

  Future<void> _ask() async {
    if (_asking || !mounted) return;
    _asking = true;
    try {
      final found = await ref.read(repositoryProvider).screenSources();
      if (!mounted) return;
      setState(() {
        _sources = found;
        _problem = null;
      });
    } on Object catch (error) {
      if (!mounted) return;
      // A refresh that fails leaves what is on screen alone: the list is
      // still true enough to pick from, and blanking it mid-choice would
      // be worse than a stale thumbnail.
      setState(() => _problem = _sources == null ? '$error' : _problem);
    } finally {
      _asking = false;
    }
  }

  List<ScreenSource> _of(StreamSource kind) => [
    for (final source in _sources ?? const <ScreenSource>[])
      if (source.kind == kind) source,
  ];

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final screens = _of(StreamSource.screen);
    final windows = _of(StreamSource.window);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      mainAxisSize: MainAxisSize.min,
      children: [
        TabBar(
          controller: _tabs,
          labelColor: colors.text,
          unselectedLabelColor: colors.textSecondary,
          indicatorColor: colors.accent,
          tabs: [
            Tab(text: 'Screens (${screens.length})'),
            Tab(text: 'Windows (${windows.length})'),
          ],
        ),
        const SizedBox(height: OcSpace.s12),
        SizedBox(
          height: 260,
          child: switch ((_sources, _problem)) {
            (null, final problem?) => _Message('Nothing to share: $problem'),
            (null, _) => const Center(child: CircularProgressIndicator()),
            _ => TabBarView(
              controller: _tabs,
              children: [
                _Sources(
                  sources: screens,
                  chosen: widget.chosen,
                  onChosen: widget.onChosen,
                  empty: 'No screens were found.',
                ),
                _Sources(
                  sources: windows,
                  chosen: widget.chosen,
                  onChosen: widget.onChosen,
                  empty: 'No windows are open to share.',
                ),
              ],
            ),
          },
        ),
      ],
    );
  }
}

class _Message extends StatelessWidget {
  const _Message(this.text);

  final String text;

  @override
  Widget build(BuildContext context) => Center(
    child: Padding(
      padding: const EdgeInsets.all(OcSpace.s16),
      child: Text(
        text,
        textAlign: TextAlign.center,
        style: OcText.body.copyWith(color: context.oc.textSecondary),
      ),
    ),
  );
}

class _Sources extends StatelessWidget {
  const _Sources({
    required this.sources,
    required this.chosen,
    required this.onChosen,
    required this.empty,
  });

  final List<ScreenSource> sources;
  final String? chosen;
  final ValueChanged<String> onChosen;
  final String empty;

  @override
  Widget build(BuildContext context) {
    if (sources.isEmpty) return _Message(empty);
    return GridView.builder(
      padding: const EdgeInsets.symmetric(vertical: OcSpace.s4),
      gridDelegate: const SliverGridDelegateWithFixedCrossAxisCount(
        crossAxisCount: 2,
        crossAxisSpacing: OcSpace.s12,
        mainAxisSpacing: OcSpace.s12,
        childAspectRatio: 4 / 3.4,
      ),
      itemCount: sources.length,
      itemBuilder: (context, index) {
        final source = sources[index];
        return _SourceTile(
          source: source,
          chosen: source.id == chosen,
          onTap: () => onChosen(source.id),
        );
      },
    );
  }
}

class _SourceTile extends StatelessWidget {
  const _SourceTile({
    required this.source,
    required this.chosen,
    required this.onTap,
  });

  final ScreenSource source;
  final bool chosen;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Hoverable(
      onTap: onTap,
      semanticLabel: source.app == null
          ? source.title
          : '${source.title}, ${source.app}',
      focusRadius: BorderRadius.circular(10),
      builder: (context, state) => AnimatedContainer(
        duration: OcMotion.of(context).hover,
        decoration: BoxDecoration(
          color: state.active ? colors.hover : colors.surface,
          borderRadius: BorderRadius.circular(10),
          border: Border.all(
            color: chosen ? colors.accent : colors.border,
            width: chosen ? 2 : 1,
          ),
        ),
        padding: const EdgeInsets.all(OcSpace.s6),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Expanded(
              child: ClipRRect(
                borderRadius: BorderRadius.circular(6),
                child: _Thumbnail(thumbnail: source.thumbnail),
              ),
            ),
            const SizedBox(height: OcSpace.s6),
            Text(
              source.title,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: OcText.small.copyWith(
                color: colors.text,
                fontWeight: chosen ? FontWeight.w600 : FontWeight.w400,
              ),
            ),
            if (source.app case final app?)
              Text(
                app,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: OcText.small.copyWith(color: colors.textMuted),
              ),
          ],
        ),
      ),
    );
  }
}

/// A source's picture, or a plain block where there is none — a window that
/// refused to draw itself, which is not worth an error.
class _Thumbnail extends StatelessWidget {
  const _Thumbnail({required this.thumbnail});

  final ScreenThumbnail? thumbnail;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    if (thumbnail case final picture? when picture.width > 0) {
      return Image.memory(
        _bmp(picture),
        gaplessPlayback: true,
        fit: BoxFit.contain,
        errorBuilder: (context, _, _) => ColoredBox(color: colors.elevated),
      );
    }
    return ColoredBox(
      color: colors.elevated,
      child: Center(
        child: Icon(Icons.desktop_windows_outlined, color: colors.textMuted),
      ),
    );
  }

  /// The raw RGBA rows as a BMP, which `Image.memory` can read. The core
  /// hands over pixels rather than an encoded picture — encoding a PNG
  /// every two seconds for every window would cost more than the capture —
  /// and a 32-bit BMP is a 54-byte header in front of the rows.
  static Uint8List _bmp(ScreenThumbnail picture) {
    const header = 54;
    final rows = picture.rgba.length;
    final bytes = BytesBuilder(copy: false);
    final head = ByteData(header);
    head.setUint8(0, 0x42); // 'B'
    head.setUint8(1, 0x4d); // 'M'
    head.setUint32(2, header + rows, Endian.little);
    head.setUint32(10, header, Endian.little);
    head.setUint32(14, 40, Endian.little);
    head.setInt32(18, picture.width, Endian.little);
    // Negative: the rows are top down, as the core packs them.
    head.setInt32(22, -picture.height, Endian.little);
    head.setUint16(26, 1, Endian.little);
    head.setUint16(28, 32, Endian.little);
    head.setUint32(34, rows, Endian.little);
    bytes.add(head.buffer.asUint8List());
    // BMP wants BGRA where the core packs RGBA.
    final pixels = Uint8List(rows);
    for (var at = 0; at + 3 < rows; at += 4) {
      pixels[at] = picture.rgba[at + 2];
      pixels[at + 1] = picture.rgba[at + 1];
      pixels[at + 2] = picture.rgba[at];
      pixels[at + 3] = picture.rgba[at + 3];
    }
    bytes.add(pixels);
    return bytes.takeBytes();
  }
}
