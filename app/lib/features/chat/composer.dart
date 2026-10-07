import 'dart:async';

import 'package:clock/clock.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/chat/chat_controller.dart';
import 'package:opencord/features/chat/composer_bar.dart';
import 'package:opencord/features/chat/composer_state.dart';
import 'package:opencord/features/chat/emoji_picker.dart';
import 'package:opencord/features/chat/mentions.dart';
import 'package:opencord/features/chat/suggestion_list.dart';
import 'package:opencord/features/chat/suggestions.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/oc_text_field.dart';
import 'package:opencord/ui/widgets/popover.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// How often "typing" is sent while typing (§6).
const typingInterval = Duration(seconds: 8);

/// The message box at the bottom of a text channel (§4.6).
class Composer extends ConsumerStatefulWidget {
  const Composer({super.key, required this.channel, required this.controller});

  final ChannelRef channel;
  final ChatController controller;

  @override
  ConsumerState<Composer> createState() => _ComposerState();
}

class _ComposerState extends ConsumerState<Composer> {
  late final TextEditingController _text;
  final _focus = FocusNode(debugLabel: 'composer');
  final _field = LayerLink();
  final _fieldKey = GlobalKey();
  final _popup = OverlayPortalController();
  DateTime? _typingSent;
  ActiveToken? _token;
  List<Suggestion> _suggestions = const [];
  int _selected = 0;

  /// The token whose suggestions were closed with Escape; they stay closed
  /// until something else is typed.
  int? _dismissedAt;

  ChannelRef get _channel => widget.channel;

  ComposerNotifier get _composer =>
      ref.read(composerProvider(_channel).notifier);

  ChannelMessagesNotifier get _messages =>
      ref.read(channelMessagesProvider(_channel).notifier);

  ServerData? get _data => ref.read(serverProvider(_channel.server)).data;

  Map<int, String> _memberNames(ServerData data) => {
    for (final member in data.members.values) member.id: member.displayName,
  };

  Map<int, String> _channelNames(ServerData data) => {
    for (final channel in data.channels.values) channel.id: channel.name,
  };

  @override
  void initState() {
    super.initState();
    _text = TextEditingController(
      text: ref.read(composerProvider(_channel)).draft,
    );
    _text.addListener(_onChanged);
    ref.listenManual(composerProvider(_channel), _onComposerState);
    widget.controller.attachComposer(_requestFocus);
    HardwareKeyboard.instance.addHandler(_typeToFocus);
  }

  @override
  void didUpdateWidget(Composer oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!identical(oldWidget.controller, widget.controller)) {
      oldWidget.controller.detachComposer(_requestFocus);
      widget.controller.attachComposer(_requestFocus);
    }
  }

  @override
  void dispose() {
    HardwareKeyboard.instance.removeHandler(_typeToFocus);
    widget.controller.detachComposer(_requestFocus);
    _text.dispose();
    _focus.dispose();
    super.dispose();
  }

  void _requestFocus() {
    _focus.requestFocus();
    _text.selection = TextSelection.collapsed(offset: _text.text.length);
  }

  /// Replies and edits started elsewhere (the message menu) arrive here;
  /// the text follows the state when something other than typing changed
  /// it.
  void _onComposerState(ComposerState? previous, ComposerState next) {
    final startedEditing =
        next.editing != null && previous?.editing != next.editing;
    final data = _data;
    final text = startedEditing && data != null
        ? decodeMentions(
            next.draft,
            members: _memberNames(data),
            channels: _channelNames(data),
          )
        : next.draft;
    if (text != _text.text) {
      _text.value = TextEditingValue(
        text: text,
        selection: TextSelection.collapsed(offset: text.length),
      );
    }
    if (previous?.replyTo != next.replyTo ||
        previous?.editing != next.editing) {
      setState(() {});
    }
  }

  void _onChanged() {
    _composer.setDraft(_text.text);
    _sendTyping();
    _updateSuggestions();
    setState(() {});
  }

  void _sendTyping() {
    if (_text.text.trim().isEmpty) return;
    if (ref.read(composerProvider(_channel)).editing != null) return;
    final now = clock.now();
    final last = _typingSent;
    if (last != null && now.difference(last) < typingInterval) return;
    _typingSent = now;
    unawaited(
      ref
          .read(repositoryProvider)
          .startTyping(_channel.server, _channel.channel)
          .catchError((Object _) {}),
    );
  }

  // Autocomplete -------------------------------------------------------------

  void _updateSuggestions() {
    final selection = _text.selection;
    final data = _data;
    final channel = data?.channels[_channel.channel];
    final token = selection.isCollapsed && data != null && channel != null
        ? activeToken(_text.text, selection.baseOffset)
        : null;
    if (token?.start != _dismissedAt) _dismissedAt = null;
    final suggestions = token == null || _dismissedAt != null
        ? const <Suggestion>[]
        : suggest(data!, channel!, token);
    _token = token;
    if (suggestions.length != _suggestions.length ||
        !_sameSuggestions(suggestions)) {
      _selected = 0;
    }
    _suggestions = suggestions;
    if (suggestions.isEmpty) {
      if (_popup.isShowing) _popup.hide();
    } else if (!_popup.isShowing) {
      _popup.show();
    }
  }

  bool _sameSuggestions(List<Suggestion> other) {
    for (var i = 0; i < other.length; i++) {
      if (other[i].insert != _suggestions[i].insert) return false;
    }
    return true;
  }

  void _pick(Suggestion suggestion) {
    final token = _token;
    if (token == null) return;
    final caret = _text.selection.baseOffset;
    final text = _text.text.replaceRange(token.start, caret, suggestion.insert);
    final offset = token.start + suggestion.insert.length;
    _text.value = TextEditingValue(
      text: text,
      selection: TextSelection.collapsed(offset: offset),
    );
    _focus.requestFocus();
  }

  void _insert(String insert) {
    final selection = _text.selection.isValid
        ? _text.selection
        : TextSelection.collapsed(offset: _text.text.length);
    final text = _text.text.replaceRange(
      selection.start,
      selection.end,
      insert,
    );
    _text.value = TextEditingValue(
      text: text,
      selection: TextSelection.collapsed(
        offset: selection.start + insert.length,
      ),
    );
  }

  // Keys ----------------------------------------------------------------------

  bool get _composing =>
      _text.value.composing.isValid && !_text.value.composing.isCollapsed;

  KeyEventResult _onKey(FocusNode node, KeyEvent event) {
    if (event is! KeyDownEvent && event is! KeyRepeatEvent) {
      return KeyEventResult.ignored;
    }
    // Keys belong to the input method while it composes (§4.6).
    if (_composing) return KeyEventResult.ignored;
    final key = event.logicalKey;
    final keyboard = HardwareKeyboard.instance;
    final shift = keyboard.isShiftPressed;
    final suggesting = _popup.isShowing && _suggestions.isNotEmpty;

    if (suggesting) {
      if (key == LogicalKeyboardKey.arrowDown ||
          key == LogicalKeyboardKey.arrowUp) {
        final step = key == LogicalKeyboardKey.arrowDown ? 1 : -1;
        setState(() => _selected = (_selected + step) % _suggestions.length);
        return KeyEventResult.handled;
      }
      if (key == LogicalKeyboardKey.enter ||
          key == LogicalKeyboardKey.numpadEnter ||
          key == LogicalKeyboardKey.tab) {
        _pick(_suggestions[_selected]);
        return KeyEventResult.handled;
      }
      if (key == LogicalKeyboardKey.escape) {
        _dismissedAt = _token?.start;
        _updateSuggestions();
        setState(() {});
        return KeyEventResult.handled;
      }
    }

    if ((key == LogicalKeyboardKey.enter ||
            key == LogicalKeyboardKey.numpadEnter) &&
        !shift) {
      _submit();
      return KeyEventResult.handled;
    }
    if (key == LogicalKeyboardKey.escape) {
      final state = ref.read(composerProvider(_channel));
      if (state.replyTo == null && state.editing == null) {
        return KeyEventResult.ignored;
      }
      _composer.cancel();
      return KeyEventResult.handled;
    }
    if (key == LogicalKeyboardKey.arrowUp && _text.text.isEmpty) {
      return _editLast() ? KeyEventResult.handled : KeyEventResult.ignored;
    }
    if (key == LogicalKeyboardKey.pageUp ||
        key == LogicalKeyboardKey.pageDown) {
      widget.controller.scrollPage(key == LogicalKeyboardKey.pageUp ? -1 : 1);
      return KeyEventResult.handled;
    }
    if (key == LogicalKeyboardKey.keyV &&
        (keyboard.isControlPressed || keyboard.isMetaPressed)) {
      unawaited(_checkPaste());
    }
    return KeyEventResult.ignored;
  }

  /// Images cannot be sent yet: when what is pasted is not text, say so.
  Future<void> _checkPaste() async {
    final hasText = await Clipboard.hasStrings();
    if (!hasText && mounted) {
      showOcToast(context, 'Pasting images is coming in a later version.');
    }
  }

  /// ↑ in an empty composer edits your last message (§4.6, §7).
  bool _editLast() {
    final self = _data?.self.id;
    final mine = ref
        .read(channelMessagesProvider(_channel))
        .messages
        .lastWhere(
          (message) =>
              message.authorId == self &&
              message.sendState == SendState.sent &&
              !message.isSystem,
          orElse: () => _none,
        );
    if (identical(mine, _none)) return false;
    _composer.edit(mine);
    return true;
  }

  static final _none = Message(
    id: 0,
    channelId: 0,
    authorId: 0,
    content: '',
    createdAt: DateTime(0),
  );

  // Sending -----------------------------------------------------------------

  bool get _coolingDown => ref.read(sendCooldownProvider(_channel)) != null;

  Future<void> _submit() async {
    final data = _data;
    if (data == null) return;
    final text = _text.text.trim();
    final state = ref.read(composerProvider(_channel));
    final encoded = encodeMentions(
      text,
      members: _memberNames(data),
      channels: _channelNames(data),
    );
    final editing = state.editing;
    if (editing != null) {
      if (text.isEmpty) return;
      _composer.sent();
      if (encoded == editing.content) return;
      try {
        await _messages.edit(editing.id, encoded);
      } on RepoException catch (error) {
        if (!mounted) return;
        // Nothing written is lost: back to editing, with the new text.
        _composer
          ..edit(editing)
          ..setDraft(text);
        showOcToast(context, error.message);
      }
      return;
    }
    if (text.isEmpty || _coolingDown) return;
    _composer.sent();
    unawaited(_messages.send(encoded, replyToId: state.replyTo?.id));
    unawaited(widget.controller.scrollToEnd());
  }

  // Type to focus -------------------------------------------------------------

  /// Typing a character anywhere in the window focuses the composer and
  /// types it there (§16). Not while another text field, a menu or a
  /// dialog has the keyboard, and not for spaces, which press buttons.
  bool _typeToFocus(KeyEvent event) {
    if (event is! KeyDownEvent || _focus.hasFocus || !mounted) return false;
    final character = event.character;
    if (character == null ||
        character.isEmpty ||
        character.trim().isEmpty ||
        character.runes.any((rune) => rune < 0x20 || rune == 0x7F)) {
      return false;
    }
    final keyboard = HardwareKeyboard.instance;
    if (keyboard.isControlPressed ||
        keyboard.isAltPressed ||
        keyboard.isMetaPressed) {
      return false;
    }
    if (!(ModalRoute.of(context)?.isCurrent ?? true)) return false;
    final focused = FocusManager.instance.primaryFocus?.context;
    if (focused != null &&
        (focused.widget is EditableText ||
            focused.findAncestorWidgetOfExactType<EditableText>() != null)) {
      return false;
    }
    if (!_canSend()) return false;
    _requestFocus();
    _insert(character);
    return true;
  }

  bool _canSend() =>
      _data?.permissionsIn(_channel.channel).has(Permissions.sendMessages) ??
      false;

  // Building ------------------------------------------------------------------

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final data = ref.watch(
      serverProvider(_channel.server).select((state) => state.data),
    );
    final channel = data?.channels[_channel.channel];
    final canSend =
        data?.permissionsIn(_channel.channel).has(Permissions.sendMessages) ??
        false;
    final state = ref.watch(composerProvider(_channel));
    final cooldown = ref.watch(sendCooldownProvider(_channel));
    final decoration = BoxDecoration(
      color: colors.sidebar,
      border: Border(top: BorderSide(color: colors.border)),
    );
    if (data == null || channel == null) return const SizedBox.shrink();
    if (!canSend) {
      return Container(
        decoration: decoration,
        padding: const EdgeInsets.all(OcSpace.s16),
        alignment: Alignment.center,
        child: Text(
          'You do not have permission to send messages in this channel.',
          textAlign: TextAlign.center,
          style: OcText.body.copyWith(color: colors.textMuted),
        ),
      );
    }
    final hasText = _text.text.trim().isNotEmpty;
    final editing = state.editing;
    final replyTo = state.replyTo;
    final about = editing ?? replyTo;
    return Container(
      decoration: decoration,
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
            // Suggestions open above all of this, reply bar included.
            child: CompositedTransformTarget(
              link: _field,
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  if (about != null)
                    ComposerContextBar(
                      editing: editing != null,
                      title: editing != null
                          ? 'Edit message'
                          : 'Reply to ${data.members[about.authorId]?.displayName ?? 'Unknown user'}',
                      preview: previewText(
                        about.content,
                        user: (id) => data.members[id]?.displayName,
                        channel: (id) => data.channels[id]?.name,
                      ),
                      onTap: () => widget.controller.jumpToMessage(about.id),
                      onClose: _composer.cancel,
                    ),
                  Row(
                    crossAxisAlignment: CrossAxisAlignment.end,
                    children: [
                      const Padding(
                        padding: EdgeInsets.only(bottom: 2),
                        child: OcIconButton(
                          icon: OcIcons.attachFile,
                          tooltip: 'Attachments are coming in a later version',
                        ),
                      ),
                      const SizedBox(width: OcSpace.s4),
                      Expanded(child: _input(context, channel.name)),
                      const SizedBox(width: OcSpace.s6),
                      if (cooldown != null)
                        Padding(
                          padding: const EdgeInsets.only(
                            right: OcSpace.s6,
                            bottom: 11,
                          ),
                          child: _Countdown(until: cooldown),
                        ),
                      Padding(
                        padding: const EdgeInsets.only(bottom: 2),
                        child: hasText || editing != null
                            ? OcIconButton(
                                icon: editing != null
                                    ? OcIcons.check
                                    : OcIcons.send,
                                tooltip: editing != null ? 'Save' : 'Send',
                                active: true,
                                activeStyle: OcActiveStyle.inverted,
                                onPressed: cooldown != null && editing == null
                                    ? null
                                    : _submit,
                              )
                            : OcIconButton(
                                icon: OcIcons.mic,
                                tooltip: 'Voice message',
                                color: colors.textMuted,
                                onPressed: () => showOcToast(
                                  context,
                                  'Voice messages are coming in a later version.',
                                ),
                              ),
                      ),
                    ],
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }

  Widget _input(BuildContext context, String channelName) {
    return OverlayPortal(
      controller: _popup,
      overlayChildBuilder: (context) => Positioned(
        left: 0,
        top: 0,
        child: CompositedTransformFollower(
          link: _field,
          showWhenUnlinked: false,
          targetAnchor: Alignment.topLeft,
          followerAnchor: Alignment.bottomLeft,
          offset: const Offset(OcSize.hitDefault + OcSpace.s4, -OcSpace.s6),
          child: SuggestionList(
            width: _fieldWidth(),
            suggestions: _suggestions,
            selected: _selected,
            onHover: (index) => setState(() => _selected = index),
            onPick: _pick,
          ),
        ),
      ),
      child: KeyedSubtree(
        key: _fieldKey,
        child: Focus(
          canRequestFocus: false,
          skipTraversal: true,
          onKeyEvent: _onKey,
          child: TextField(
            controller: _text,
            focusNode: _focus,
            minLines: 1,
            maxLines: 8,
            keyboardType: TextInputType.multiline,
            textInputAction: TextInputAction.newline,
            style: OcText.body.copyWith(color: context.oc.text),
            cursorColor: context.oc.text,
            decoration: ocInputDecoration(
              context,
              hint: 'Message #$channelName',
              radius: OcRadius.composer,
              padding: const EdgeInsets.fromLTRB(
                OcSpace.s16,
                OcSpace.s10,
                OcSpace.s8,
                OcSpace.s10,
              ),
              suffix: Builder(
                builder: (context) => OcIconButton(
                  icon: OcIcons.mood,
                  tooltip: 'Emoji',
                  size: OcIconButtonSize.compact,
                  onPressed: () async {
                    final emoji = await showEmojiPicker(
                      context,
                      anchor: globalRectOf(context),
                    );
                    if (emoji == null || !mounted) return;
                    _focus.requestFocus();
                    _insert(emoji);
                  },
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }

  double _fieldWidth() {
    final box = _fieldKey.currentContext?.findRenderObject();
    if (box is! RenderBox || !box.hasSize) return 320;
    return box.size.width;
  }
}

/// "4 s" until sending works again (§4.6).
class _Countdown extends StatefulWidget {
  const _Countdown({required this.until});

  final DateTime until;

  @override
  State<_Countdown> createState() => _CountdownState();
}

class _CountdownState extends State<_Countdown> {
  late final Timer _tick;

  @override
  void initState() {
    super.initState();
    // Started here: a lazy field first read in dispose never ran.
    _tick = Timer.periodic(
      const Duration(milliseconds: 250),
      (_) => setState(() {}),
    );
  }

  @override
  void dispose() {
    _tick.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final left = widget.until.difference(clock.now());
    final seconds = (left.inMilliseconds / 1000).ceil().clamp(0, 9999);
    return Tooltip(
      message: 'Sending too fast: you can send again in a moment',
      child: Text(
        '$seconds s',
        style: OcText.small.copyWith(color: context.oc.textMuted),
      ),
    );
  }
}
