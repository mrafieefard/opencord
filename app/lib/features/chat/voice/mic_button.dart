import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/chat/voice/voice_recorder_state.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/widgets/toast.dart';

class MicButton extends ConsumerStatefulWidget {
  const MicButton({required this.channel, super.key});

  final ChannelRef channel;

  @override
  ConsumerState<MicButton> createState() => _MicButtonState();
}

class _MicButtonState extends ConsumerState<MicButton> {
  Offset? _downPos;
  bool _spaceDown = false;
  late VoiceRecorderNotifier _notifier;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _notifier = ref.read(voiceRecorderProvider(widget.channel).notifier);
  }

  void _onPointerDown(PointerDownEvent event) {
    _downPos = event.position;
    _notifier.press();
  }

  void _onPointerMove(PointerMoveEvent event) {
    if (_downPos case final down?) {
      _notifier.move(event.position - down);
    }
  }

  void _onPointerUp(PointerUpEvent event) {
    _downPos = null;
    final sent = _notifier.release();
    if (!sent && mounted) {
      showOcToast(context, 'Hold to record');
    }
  }

  void _onPointerCancel(PointerCancelEvent event) {
    _downPos = null;
    _notifier.cancel();
  }

  KeyEventResult _onKeyEvent(FocusNode node, KeyEvent event) {
    if (event is KeyDownEvent) {
      if (event.logicalKey == LogicalKeyboardKey.space && !_spaceDown) {
        _spaceDown = true;
        _notifier.press();
        return KeyEventResult.handled;
      } else if (event.logicalKey == LogicalKeyboardKey.escape) {
        _notifier.cancel();
        return KeyEventResult.handled;
      }
    } else if (event is KeyUpEvent) {
      if (event.logicalKey == LogicalKeyboardKey.space && _spaceDown) {
        _spaceDown = false;
        final sent = _notifier.release();
        if (!sent && mounted) {
          showOcToast(context, 'Hold to record');
        }
        return KeyEventResult.handled;
      }
    }
    return KeyEventResult.ignored;
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;

    return Focus(
      onKeyEvent: _onKeyEvent,
      child: Tooltip(
        message: 'Voice message',
        excludeFromSemantics: true,
        child: Listener(
          onPointerDown: _onPointerDown,
          onPointerMove: _onPointerMove,
          onPointerUp: _onPointerUp,
          onPointerCancel: _onPointerCancel,
          behavior: HitTestBehavior.opaque,
          child: Padding(
            padding: const EdgeInsets.all(6),
            child: Icon(OcIcons.mic, size: 20, color: colors.textMuted),
          ),
        ),
      ),
    );
  }
}
