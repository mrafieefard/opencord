import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/hotkeys.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/hoverable.dart';

/// Shows a keybind's keys; click it, then press the keys, like Discord's
/// "Record keybind". A key with the modifiers held records at once; a
/// modifier on its own (Right Ctrl, say) records when it comes up. Escape
/// or clicking elsewhere stops without a change.
class KeybindRecorder extends ConsumerStatefulWidget {
  const KeybindRecorder({
    super.key,
    required this.keys,
    required this.onRecorded,
    required this.semanticLabel,
    this.autofocus = false,
  });

  /// As [Accelerator] writes them; empty for none.
  final String keys;
  final ValueChanged<String> onRecorded;
  final String semanticLabel;

  /// Starts recording at once (a keybind just added).
  final bool autofocus;

  @override
  ConsumerState<KeybindRecorder> createState() => _KeybindRecorderState();
}

class _KeybindRecorderState extends ConsumerState<KeybindRecorder> {
  final _focus = FocusNode(debugLabel: 'keybind recorder');
  var _recording = false;

  /// Modifier keys down since recording started, in the order pressed.
  final _modifiers = <LogicalKeyboardKey>[];
  String? _problem;

  @override
  void initState() {
    super.initState();
    _focus.addListener(() {
      if (!_focus.hasFocus && _recording) _stop();
    });
    if (widget.autofocus) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) _start();
      });
    }
  }

  @override
  void dispose() {
    if (_recording) {
      // The provider outlives this page.
      final recording = ref.read(recordingKeybindProvider.notifier);
      WidgetsBinding.instance.addPostFrameCallback((_) => recording.set(false));
    }
    _focus.dispose();
    super.dispose();
  }

  void _start() {
    setState(() {
      _recording = true;
      _modifiers.clear();
      _problem = null;
    });
    ref.read(recordingKeybindProvider.notifier).set(true);
    _focus.requestFocus();
  }

  void _stop() {
    setState(() => _recording = false);
    ref.read(recordingKeybindProvider.notifier).set(false);
  }

  void _finish(Accelerator accelerator) {
    _stop();
    _focus.unfocus();
    widget.onRecorded(accelerator.toString());
  }

  /// [key] with the modifiers held now, other than [key] itself.
  Accelerator _with(LogicalKeyboardKey key) {
    bool held(Set<LogicalKeyboardKey> kind) =>
        _modifiers.any((down) => down != key && kind.contains(down));
    return Accelerator(
      key: key,
      ctrl: held(_ctrl),
      alt: held(_alt),
      shift: held(_shift),
      logo: held(_logo),
    );
  }

  KeyEventResult _onKey(FocusNode node, KeyEvent event) {
    if (!_recording) return KeyEventResult.ignored;
    final key = event.logicalKey;
    switch (event) {
      case KeyDownEvent() when key == LogicalKeyboardKey.escape:
        _stop();
      case KeyDownEvent() when _isModifier(key):
        setState(() {
          if (!_modifiers.contains(key)) _modifiers.add(key);
          _problem = null;
        });
      case KeyDownEvent() when !Accelerator.canUse(key):
        setState(() => _problem = "${keyLabel(key)} can't be a keybind");
      case KeyDownEvent():
        _finish(_with(key));
      case KeyUpEvent() when _modifiers.contains(key):
        // Only modifiers went down: the one let go is the key.
        _finish(_with(key));
      default:
        break;
    }
    return KeyEventResult.handled;
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final mac = Theme.of(context).platform == TargetPlatform.macOS;
    final current = Accelerator.parse(widget.keys)?.label(mac: mac);
    final text = switch (_recording) {
      true when _modifiers.isNotEmpty => [
        for (final key in _modifiers) keyLabel(key, mac: mac),
        '…',
      ].join(' + '),
      true => _problem ?? 'Press keys…',
      false => current ?? 'No keybind',
    };
    return Focus(
      focusNode: _focus,
      onKeyEvent: _onKey,
      child: Hoverable(
        onTap: _recording ? _stop : _start,
        semanticLabel: widget.semanticLabel,
        focusRadius: BorderRadius.circular(8),
        builder: (context, state) => AnimatedContainer(
          duration: OcMotion.of(context).hover,
          height: 36,
          padding: const EdgeInsets.symmetric(horizontal: 12),
          decoration: BoxDecoration(
            color: state.active ? colors.hover : colors.surface,
            borderRadius: BorderRadius.circular(8),
            border: Border.all(
              color: _recording ? colors.text : colors.border,
              width: _recording ? 2 : 1,
            ),
          ),
          child: Row(
            children: [
              Expanded(
                child: Semantics(
                  liveRegion: _recording,
                  child: Text(
                    text,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: OcText.body.copyWith(
                      fontWeight: current != null || _recording
                          ? FontWeight.w600
                          : FontWeight.w400,
                      color: current != null || _recording
                          ? colors.text
                          : colors.textMuted,
                    ),
                  ),
                ),
              ),
              const SizedBox(width: 8),
              Text(
                _recording ? 'Stop' : 'Record',
                style: OcText.small.copyWith(color: colors.textSecondary),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

final _ctrl = {LogicalKeyboardKey.controlLeft, LogicalKeyboardKey.controlRight};
final _alt = {LogicalKeyboardKey.altLeft, LogicalKeyboardKey.altRight};
final _shift = {LogicalKeyboardKey.shiftLeft, LogicalKeyboardKey.shiftRight};
final _logo = {LogicalKeyboardKey.metaLeft, LogicalKeyboardKey.metaRight};

bool _isModifier(LogicalKeyboardKey key) =>
    _ctrl.contains(key) ||
    _alt.contains(key) ||
    _shift.contains(key) ||
    _logo.contains(key);
