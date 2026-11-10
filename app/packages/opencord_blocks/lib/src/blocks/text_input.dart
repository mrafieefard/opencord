import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../common_props.dart';
import '../view.dart';

/// `text_input` (design §12.2): single or multi-line text input field;
/// keeps caret while the plugin redraws, throttles `change` events to 100 ms,
/// and submits on Enter (single line) or Ctrl+Enter (multiline).
final textInputBlock = BlockRenderer(_content);

Widget _content(BlockBuild b) {
  return _TextInputWidget(key: ValueKey<int>(b.node.id), build: b);
}

class _TextInputWidget extends StatefulWidget {
  const _TextInputWidget({super.key, required this.build});

  final BlockBuild build;

  @override
  State<_TextInputWidget> createState() => _TextInputWidgetState();
}

class _TextInputWidgetState extends State<_TextInputWidget> {
  late final TextEditingController _controller;
  late final FocusNode _focusNode;
  String _lastSentValue = '';
  DateTime _lastSentTime = DateTime.fromMillisecondsSinceEpoch(0);
  Timer? _throttleTimer;
  String? _pendingText;

  @override
  void initState() {
    super.initState();
    final initial = widget.build.props['value'] as String? ?? '';
    _controller = TextEditingController(text: initial);
    _lastSentValue = initial;
    _focusNode = FocusNode();
    _focusNode.addListener(_onFocusChange);
  }

  @override
  void didUpdateWidget(_TextInputWidget oldWidget) {
    super.didUpdateWidget(oldWidget);
    final nextValue = widget.build.props['value'] as String? ?? '';
    if (_focusNode.hasFocus) {
      if (nextValue != _lastSentValue) {
        _controller.text = nextValue;
        _controller.selection = TextSelection.collapsed(
          offset: nextValue.length,
        );
        _lastSentValue = nextValue;
      }
    } else {
      if (nextValue != _controller.text) {
        _controller.text = nextValue;
        _controller.selection = TextSelection.collapsed(
          offset: nextValue.length,
        );
        _lastSentValue = nextValue;
      }
    }
  }

  @override
  void dispose() {
    _throttleTimer?.cancel();
    _focusNode.removeListener(_onFocusChange);
    _focusNode.dispose();
    _controller.dispose();
    super.dispose();
  }

  void _onFocusChange() {
    final node = widget.build.node;
    if (_focusNode.hasFocus) {
      if (node.handles('focus')) widget.build.send('focus');
    } else {
      if (node.handles('blur')) widget.build.send('blur');
    }
  }

  void _onChanged(String text) {
    _pendingText = text;
    final node = widget.build.node;
    if (!node.handles('change')) return;

    final now = DateTime.now();
    final elapsed = now.difference(_lastSentTime).inMilliseconds;
    if (elapsed >= 100) {
      _lastSentTime = now;
      _throttleTimer?.cancel();
      _pendingText = null;
      _lastSentValue = text;
      widget.build.send('change', {'value': text});
    } else {
      _throttleTimer ??= Timer(Duration(milliseconds: 100 - elapsed), () {
        _throttleTimer = null;
        if (!mounted) return;
        final pending = _pendingText;
        _pendingText = null;
        if (pending != null) {
          _lastSentTime = DateTime.now();
          _lastSentValue = pending;
          widget.build.send('change', {'value': pending});
        }
      });
    }
  }

  void _submit() {
    _throttleTimer?.cancel();
    final pending = _pendingText;
    _pendingText = null;
    if (pending != null) {
      _lastSentValue = pending;
      if (widget.build.node.handles('change')) {
        widget.build.send('change', {'value': pending});
      }
    }
    if (widget.build.node.handles('submit')) {
      widget.build.send('submit', {'value': _controller.text});
    }
  }

  @override
  Widget build(BuildContext context) {
    final props = widget.build.props;
    final placeholder = props['placeholder'] as String? ?? '';
    final password = props['password'] == true;
    final disabled = props['disabled'] == true;
    final lines = (numberProp(props['lines']) ?? 1).toInt();
    final maxLength = (numberProp(props['max_length']) ?? 0).toInt();
    final theme = widget.build.theme;

    final isSingleLine = lines <= 1;

    return Material(
      type: MaterialType.transparency,
      child: Focus(
        onKeyEvent: (node, event) {
          if (event is KeyDownEvent &&
              event.logicalKey == LogicalKeyboardKey.enter) {
            if (isSingleLine) {
              _submit();
              return KeyEventResult.handled;
            } else if (HardwareKeyboard.instance.isControlPressed ||
                HardwareKeyboard.instance.isMetaPressed) {
              _submit();
              return KeyEventResult.handled;
            }
          }
          return KeyEventResult.ignored;
        },
        child: TextField(
          controller: _controller,
          focusNode: _focusNode,
          enabled: !disabled,
          obscureText: password,
          maxLines: isSingleLine ? 1 : lines,
          minLines: isSingleLine ? 1 : lines,
          maxLength: maxLength > 0 ? maxLength : null,
          maxLengthEnforcement: MaxLengthEnforcement.enforced,
          buildCounter:
              (
                _, {
                required currentLength,
                required isFocused,
                required maxLength,
              }) => null,
          onChanged: _onChanged,
          onSubmitted: (_) => _submit(),
          style: TextStyle(
            fontFamily: theme.text.fontFamily ?? 'Inter',
            fontSize: 14,
            color: theme.colors['text']!,
          ),
          decoration: InputDecoration(
            hintText: placeholder,
            hintStyle: TextStyle(
              color: theme.colors['text_muted']!,
              fontSize: 14,
            ),
            filled: true,
            fillColor: theme.colors['surface']!,
            isDense: true,
            contentPadding: const EdgeInsets.symmetric(
              horizontal: 12,
              vertical: 10,
            ),
            border: OutlineInputBorder(
              borderRadius: BorderRadius.circular(8),
              borderSide: BorderSide(color: theme.colors['border']!, width: 1),
            ),
            enabledBorder: OutlineInputBorder(
              borderRadius: BorderRadius.circular(8),
              borderSide: BorderSide(color: theme.colors['border']!, width: 1),
            ),
            focusedBorder: OutlineInputBorder(
              borderRadius: BorderRadius.circular(8),
              borderSide: BorderSide(
                color: theme.colors['accent']!,
                width: 1.5,
              ),
            ),
          ),
        ),
      ),
    );
  }
}
