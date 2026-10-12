import 'dart:math' as math;

import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/sound.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/chat/emoji_picker.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/inline_error.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/oc_text_field.dart';
import 'package:opencord/ui/widgets/popover.dart';
import 'package:opencord/ui/widgets/section_label.dart';
import 'package:opencord/ui/widgets/settings.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// The files a sound can be made from (Phase 2 plan §11.2).
const soundFileTypes = XTypeGroup(
  label: 'Audio',
  extensions: ['mp3', 'wav', 'flac', 'ogg', 'oga', 'opus', 'm4a', 'aac'],
);

/// Shortest window worth uploading.
const _minLength = Duration(milliseconds: 100);

/// Asks for a file, then opens it in the trim editor (§11.3); the new
/// sound once uploaded to [serverKey], or null.
Future<Sound?> uploadSoundFromFile(
  BuildContext context,
  WidgetRef ref, {
  required String serverKey,
}) async {
  final file = await openFile(acceptedTypeGroups: const [soundFileTypes]);
  if (file == null || !context.mounted) return null;
  final repository = ref.read(repositoryProvider);
  final SoundDraft draft;
  try {
    draft = await repository.openSoundDraft(file.path);
  } on RepoException catch (error) {
    if (context.mounted) {
      showOcToast(context, "That file can't be used. ${error.message}");
    }
    return null;
  }
  if (!context.mounted) {
    repository.closeSoundDraft(draft.id);
    return null;
  }
  final sound = await showOcDialog<Sound>(
    context: context,
    builder: (_) => TrimEditor(
      serverKey: serverKey,
      draft: draft,
      suggestedName: _nameFrom(file.name),
    ),
  );
  // Uploading closes it; a dialog closed without uploading leaves it.
  if (sound == null) repository.closeSoundDraft(draft.id);
  return sound;
}

/// A file's name without its extension, cut to the longest sound name.
String _nameFrom(String fileName) {
  final dot = fileName.lastIndexOf('.');
  final name = (dot > 0 ? fileName.substring(0, dot) : fileName)
      .replaceAll(RegExp('[_-]+'), ' ')
      .trim();
  return name.length > 32 ? name.substring(0, 32).trimRight() : name;
}

/// The trim editor (§11.3): a waveform with a draggable window of up to
/// five seconds, preview, volume, name and emoji, and loudness
/// normalization (on by default).
class TrimEditor extends ConsumerStatefulWidget {
  const TrimEditor({
    super.key,
    required this.serverKey,
    required this.draft,
    this.suggestedName = '',
  });

  final String serverKey;
  final SoundDraft draft;
  final String suggestedName;

  @override
  ConsumerState<TrimEditor> createState() => _TrimEditorState();
}

class _TrimEditorState extends ConsumerState<TrimEditor> {
  late final _name = TextEditingController(text: widget.suggestedName);
  var _start = Duration.zero;
  late var _length = _shorter(widget.draft.duration, maxSoundDuration);
  var _volume = 100;
  var _normalize = true;
  String? _emoji;
  var _uploading = false;
  String? _error;

  static Duration _shorter(Duration a, Duration b) => a < b ? a : b;

  SoundWindow get _window => SoundWindow(
    draftId: widget.draft.id,
    start: _start,
    length: _length,
    normalize: _normalize,
    volume: _volume,
  );

  @override
  void dispose() {
    _name.dispose();
    super.dispose();
  }

  void _select(Duration start, Duration length) => setState(() {
    final total = widget.draft.duration;
    final clampedLength = Duration(
      milliseconds: length.inMilliseconds.clamp(
        math.min(_minLength.inMilliseconds, total.inMilliseconds),
        _shorter(maxSoundDuration, total).inMilliseconds,
      ),
    );
    _length = clampedLength;
    _start = Duration(
      milliseconds: start.inMilliseconds.clamp(
        0,
        (total - clampedLength).inMilliseconds,
      ),
    );
  });

  Future<void> _preview() async {
    try {
      await ref.read(repositoryProvider).previewSoundDraft(_window);
    } on RepoException catch (error) {
      if (mounted) setState(() => _error = error.message);
    }
  }

  Future<void> _upload() async {
    final name = _name.text.trim();
    if (name.length < 2) {
      setState(() => _error = 'Give the sound a name of 2 to 32 characters.');
      return;
    }
    setState(() {
      _uploading = true;
      _error = null;
    });
    try {
      final sound = await ref
          .read(repositoryProvider)
          .uploadSound(widget.serverKey, _window, name: name, emoji: _emoji);
      if (mounted) Navigator.pop(context, sound);
    } on RepoException catch (error) {
      if (!mounted) return;
      setState(() {
        _uploading = false;
        _error = switch (error.kind) {
          RepoErrorKind.soundTooLong => 'Sounds can be at most 5 seconds long.',
          RepoErrorKind.soundInvalid => "The server couldn't use this sound.",
          RepoErrorKind.soundboardFull => error.message,
          RepoErrorKind.forbidden =>
            'You need Manage soundboard to upload sounds.',
          _ => error.message,
        };
      });
    }
  }

  Future<void> _pickEmoji(BuildContext anchor) async {
    final emoji = await showEmojiPicker(
      anchor,
      anchor: globalRectOf(anchor),
      side: PopoverSide.below,
      alignEnd: false,
    );
    if (emoji != null && mounted) setState(() => _emoji = emoji);
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final end = _start + _length;
    return OcDialog(
      title: 'Upload sound',
      width: 560,
      actions: [
        OcButton(label: 'Cancel', onPressed: () => Navigator.pop(context)),
        OcButton.primary(
          label: 'Upload',
          busy: _uploading,
          onPressed: _uploading ? null : _upload,
        ),
      ],
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(
            'Drag the window to choose up to 5 seconds. Drag its edges to '
            'make it shorter.',
            style: OcText.small.copyWith(color: colors.textSecondary),
          ),
          const SizedBox(height: OcSpace.s12),
          SoundWaveform(
            waveform: widget.draft.waveform,
            duration: widget.draft.duration,
            start: _start,
            length: _length,
            onChanged: _select,
          ),
          const SizedBox(height: OcSpace.s6),
          Row(
            children: [
              Text(
                '${_clock(_start)} – ${_clock(end)}',
                style: OcText.small.copyWith(color: colors.textSecondary),
              ),
              const SizedBox(width: OcSpace.s8),
              Text(
                '${(_length.inMilliseconds / 1000).toStringAsFixed(1)} s',
                style: OcText.small.copyWith(color: colors.textMuted),
              ),
              const Spacer(),
              OcButton(
                label: 'Preview',
                icon: OcIcons.playArrow,
                onPressed: _preview,
              ),
            ],
          ),
          const SizedBox(height: OcSpace.s16),
          const SectionLabel('Name'),
          const SizedBox(height: OcSpace.s6),
          Row(
            children: [
              Builder(
                builder: (anchor) => OcIconButton(
                  icon: OcIcons.mood,
                  tooltip: _emoji == null ? 'Choose an emoji' : 'Change emoji',
                  onPressed: () => _pickEmoji(anchor),
                ),
              ),
              if (_emoji case final emoji?) ...[
                Text(emoji, style: const TextStyle(fontSize: 20)),
                OcIconButton(
                  icon: OcIcons.close,
                  tooltip: 'Remove emoji',
                  size: OcIconButtonSize.compact,
                  onPressed: () => setState(() => _emoji = null),
                ),
              ],
              const SizedBox(width: OcSpace.s8),
              Expanded(
                child: OcTextField(
                  controller: _name,
                  hint: 'Sound name',
                  maxLength: 32,
                  semanticLabel: 'Sound name',
                  onSubmitted: (_) => _upload(),
                ),
              ),
            ],
          ),
          const SizedBox(height: OcSpace.s8),
          SettingsSliderRow(
            title: 'Volume',
            value: _volume,
            min: 0,
            max: 100,
            shown: '$_volume %',
            onChanged: (value) => setState(() => _volume = value),
          ),
          SettingsSwitchRow(
            title: 'Normalize loudness',
            subtitle: 'Makes it about as loud as other sounds.',
            value: _normalize,
            onChanged: (value) => setState(() => _normalize = value),
          ),
          if (_error case final error?) ...[
            const SizedBox(height: OcSpace.s8),
            InlineError(error),
          ],
        ],
      ),
    );
  }
}

/// m:ss.s
String _clock(Duration time) {
  final seconds = time.inMilliseconds / 1000;
  final minutes = seconds ~/ 60;
  final rest = (seconds - minutes * 60).toStringAsFixed(1).padLeft(4, '0');
  return '$minutes:$rest';
}

/// A file's waveform with the chosen window over it: drag the window to
/// move it, its edges to resize it; arrow keys move it by a tenth of a
/// second.
class SoundWaveform extends StatefulWidget {
  const SoundWaveform({
    super.key,
    required this.waveform,
    required this.duration,
    required this.start,
    required this.length,
    required this.onChanged,
  });

  final List<double> waveform;
  final Duration duration;
  final Duration start;
  final Duration length;
  final void Function(Duration start, Duration length) onChanged;

  @override
  State<SoundWaveform> createState() => _SoundWaveformState();
}

enum _Drag { move, startEdge, endEdge }

class _SoundWaveformState extends State<SoundWaveform> {
  static const _height = 96.0;
  static const _edge = 10.0;
  static const _step = Duration(milliseconds: 100);
  _Drag? _drag;
  var _width = 1.0;

  double get _msPerPixel => widget.duration.inMilliseconds / _width;

  double _x(Duration time) =>
      time.inMilliseconds /
      math.max(1, widget.duration.inMilliseconds) *
      _width;

  void _begin(double x) {
    final left = _x(widget.start);
    final right = _x(widget.start + widget.length);
    _drag = (x - left).abs() <= _edge
        ? _Drag.startEdge
        : (x - right).abs() <= _edge
        ? _Drag.endEdge
        : _Drag.move;
    if (_drag == _Drag.move && (x < left || x > right)) {
      // A click outside the window centers it there.
      final center = Duration(milliseconds: (x * _msPerPixel).round());
      widget.onChanged(center - widget.length ~/ 2, widget.length);
    }
  }

  void _update(double dx) {
    final delta = Duration(milliseconds: (dx * _msPerPixel).round());
    switch (_drag) {
      case _Drag.move:
        widget.onChanged(widget.start + delta, widget.length);
      case _Drag.startEdge:
        final end = widget.start + widget.length;
        final start = widget.start + delta;
        widget.onChanged(start, end - start);
      case _Drag.endEdge:
        widget.onChanged(widget.start, widget.length + delta);
      case null:
        break;
    }
  }

  KeyEventResult _key(FocusNode node, KeyEvent event) {
    if (event is! KeyDownEvent && event is! KeyRepeatEvent) {
      return KeyEventResult.ignored;
    }
    final step = switch (event.logicalKey) {
      LogicalKeyboardKey.arrowLeft => -_step,
      LogicalKeyboardKey.arrowRight => _step,
      _ => null,
    };
    if (step == null) return KeyEventResult.ignored;
    widget.onChanged(widget.start + step, widget.length);
    return KeyEventResult.handled;
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    String seconds(Duration d) => (d.inMilliseconds / 1000).toStringAsFixed(1);
    String span(Duration start) =>
        '${seconds(start)} to ${seconds(start + widget.length)} seconds';
    return Semantics(
      label: 'Sound window',
      value: span(widget.start),
      increasedValue: span(widget.start + _step),
      decreasedValue: span(widget.start - _step),
      onIncrease: () => widget.onChanged(widget.start + _step, widget.length),
      onDecrease: () => widget.onChanged(widget.start - _step, widget.length),
      child: Focus(
        onKeyEvent: _key,
        child: LayoutBuilder(
          builder: (context, constraints) {
            _width = math.max(1, constraints.maxWidth);
            return GestureDetector(
              behavior: HitTestBehavior.opaque,
              onHorizontalDragStart: (details) =>
                  _begin(details.localPosition.dx),
              onHorizontalDragUpdate: (details) => _update(details.delta.dx),
              onHorizontalDragEnd: (_) => _drag = null,
              onTapDown: (details) {
                _begin(details.localPosition.dx);
                _drag = null;
              },
              child: MouseRegion(
                cursor: SystemMouseCursors.resizeLeftRight,
                child: CustomPaint(
                  size: Size(constraints.maxWidth, _height),
                  painter: _WaveformPainter(
                    waveform: widget.waveform,
                    from: _x(widget.start) / _width,
                    to: _x(widget.start + widget.length) / _width,
                    bars: colors.textMuted,
                    selected: colors.text,
                    window: colors.selected,
                    edge: colors.text,
                  ),
                ),
              ),
            );
          },
        ),
      ),
    );
  }
}

class _WaveformPainter extends CustomPainter {
  _WaveformPainter({
    required this.waveform,
    required this.from,
    required this.to,
    required this.bars,
    required this.selected,
    required this.window,
    required this.edge,
  });

  final List<double> waveform;

  /// The window, as fractions of the width.
  final double from;
  final double to;
  final Color bars;
  final Color selected;
  final Color window;
  final Color edge;

  @override
  void paint(Canvas canvas, Size size) {
    final left = from * size.width;
    final right = to * size.width;
    canvas.drawRRect(
      RRect.fromRectAndRadius(
        Rect.fromLTRB(left, 0, right, size.height),
        const Radius.circular(6),
      ),
      Paint()..color = window,
    );
    if (waveform.isNotEmpty) {
      final step = size.width / waveform.length;
      final paint = Paint()..strokeWidth = math.max(1, step * 0.6);
      for (final (index, peak) in waveform.indexed) {
        final x = (index + 0.5) * step;
        final half = math.max(
          1.0,
          peak.clamp(0.0, 1.0) * size.height / 2 * 0.9,
        );
        paint.color = x >= left && x <= right ? selected : bars;
        canvas.drawLine(
          Offset(x, size.height / 2 - half),
          Offset(x, size.height / 2 + half),
          paint,
        );
      }
    }
    final edges = Paint()
      ..color = edge
      ..strokeWidth = 2;
    for (final x in [left, right]) {
      canvas.drawLine(Offset(x, 0), Offset(x, size.height), edges);
    }
  }

  @override
  bool shouldRepaint(_WaveformPainter old) =>
      old.waveform != waveform ||
      old.from != from ||
      old.to != to ||
      old.bars != bars ||
      old.selected != selected;
}
