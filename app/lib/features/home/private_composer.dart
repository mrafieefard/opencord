import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/chat/composer_bar.dart';
import 'package:opencord/features/home/private_errors.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/oc_text_field.dart';

/// The text composer for private conversations (§6.3).
class PrivateComposer extends ConsumerStatefulWidget {
  const PrivateComposer({super.key, required this.peer, this.onEditMessage});

  final String peer;
  final void Function(PrivateMessage message)? onEditMessage;

  @override
  ConsumerState<PrivateComposer> createState() => PrivateComposerState();
}

class PrivateComposerState extends ConsumerState<PrivateComposer> {
  final _controller = TextEditingController();
  final _focusNode = FocusNode();
  PrivateMessage? _editing;
  DateTime? _lastTypingSent;

  @override
  void initState() {
    super.initState();
    _controller.addListener(_onTextChanged);
  }

  @override
  void dispose() {
    _controller.removeListener(_onTextChanged);
    _controller.dispose();
    _focusNode.dispose();
    super.dispose();
  }

  void startEditing(PrivateMessage message) {
    setState(() {
      _editing = message;
      _controller.text = message.content;
      _controller.selection = TextSelection.fromPosition(
        TextPosition(offset: _controller.text.length),
      );
    });
    _focusNode.requestFocus();
  }

  void _cancelEditing() {
    setState(() {
      _editing = null;
      _controller.clear();
    });
  }

  void _onTextChanged() {
    final text = _controller.text.trim();
    if (text.isEmpty) return;

    final now = DateTime.now();
    if (_lastTypingSent == null ||
        now.difference(_lastTypingSent!) >= const Duration(seconds: 8)) {
      _lastTypingSent = now;
      ref.read(repositoryProvider).sendPrivateTyping(widget.peer);
    }
  }

  bool _editLast() {
    final messages = ref
        .read(privateConversationProvider(widget.peer))
        .messages;
    final lastMine = messages.reversed
        .where((m) => m.outgoing && m.kind == PrivateMessageKind.text)
        .firstOrNull;
    if (lastMine == null) return false;
    startEditing(lastMine);
    return true;
  }

  Future<void> _submit() async {
    final text = _controller.text.trim();
    if (text.isEmpty) return;

    final editing = _editing;
    if (editing != null) {
      _cancelEditing();
      if (text == editing.content) return;
      try {
        await ref
            .read(repositoryProvider)
            .editPrivateMessage(widget.peer, editing.id, text);
      } on RepoException catch (e) {
        if (!mounted) return;
        startEditing(editing);
        _controller.text = text;
        final name = ref.read(contactNameProvider(widget.peer));
        await showPrivateFailure(context, e, name: name);
      }
      return;
    }

    _controller.clear();
    try {
      await ref.read(repositoryProvider).sendPrivateMessage(widget.peer, text);
    } on RepoException catch (e) {
      if (!mounted) return;
      final name = ref.read(contactNameProvider(widget.peer));
      await showPrivateFailure(context, e, name: name);
    }
  }

  KeyEventResult _handleKey(FocusNode node, KeyEvent event) {
    if (event is! KeyDownEvent) return KeyEventResult.ignored;

    final key = event.logicalKey;
    final shift = HardwareKeyboard.instance.isShiftPressed;

    if ((key == LogicalKeyboardKey.enter ||
            key == LogicalKeyboardKey.numpadEnter) &&
        !shift) {
      if (_controller.value.isComposingRangeValid) {
        return KeyEventResult.ignored;
      }
      unawaited(_submit());
      return KeyEventResult.handled;
    }

    if (key == LogicalKeyboardKey.escape) {
      if (_editing != null) {
        _cancelEditing();
        return KeyEventResult.handled;
      }
    }

    if (key == LogicalKeyboardKey.arrowUp && _controller.text.isEmpty) {
      if (_editLast()) return KeyEventResult.handled;
    }

    return KeyEventResult.ignored;
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final runeCount = _controller.text.runes.length;
    final showCounter = runeCount >= 3500;
    final overLimit = runeCount > 4000;

    final name = ref.watch(contactNameProvider(widget.peer));
    // The channel composer's frame and field (§6.3).
    return Container(
      decoration: BoxDecoration(
        color: colors.sidebar,
        border: Border(top: BorderSide(color: colors.border)),
      ),
      child: Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: OcSize.messageColumn),
          child: Padding(
            padding: const EdgeInsets.fromLTRB(
              OcSpace.s12,
              OcSpace.s10,
              OcSpace.s12,
              OcSpace.s12,
            ),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                if (_editing case final editing?)
                  ComposerContextBar(
                    editing: true,
                    title: 'Edit message',
                    preview: editing.content,
                    onClose: _cancelEditing,
                  ),
                Row(
                  crossAxisAlignment: CrossAxisAlignment.end,
                  children: [
                    Expanded(
                      child: Focus(
                        canRequestFocus: false,
                        skipTraversal: true,
                        onKeyEvent: _handleKey,
                        child: TextField(
                          controller: _controller,
                          focusNode: _focusNode,
                          maxLines: 8,
                          minLines: 1,
                          textInputAction: TextInputAction.newline,
                          style: OcText.body.copyWith(color: colors.text),
                          cursorColor: colors.text,
                          decoration: ocInputDecoration(
                            context,
                            hint: 'Message $name',
                            radius: OcRadius.composer,
                            padding: const EdgeInsets.fromLTRB(
                              OcSpace.s16,
                              OcSpace.s10,
                              OcSpace.s8,
                              OcSpace.s10,
                            ),
                          ),
                        ),
                      ),
                    ),
                    if (showCounter)
                      Padding(
                        padding: const EdgeInsets.only(
                          bottom: OcSpace.s8,
                          right: OcSpace.s8,
                        ),
                        child: Text(
                          '${4000 - runeCount}',
                          style: OcText.meta.copyWith(
                            color: overLimit
                                ? colors.recording
                                : colors.textMuted,
                          ),
                        ),
                      ),
                    OcIconButton(
                      icon: OcIcons.send,
                      tooltip: 'Send',
                      onPressed: overLimit ? null : _submit,
                    ),
                  ],
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
