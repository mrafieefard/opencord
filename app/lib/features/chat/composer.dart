import 'dart:async';

import 'package:clock/clock.dart';
import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/model/commands.dart';
import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/chat/attachments/staged_tiles.dart';
import 'package:opencord/features/chat/attachments/summary.dart';
import 'package:opencord/features/chat/chat_controller.dart';
import 'package:opencord/features/chat/composer_bar.dart';
import 'package:opencord/features/chat/composer_state.dart';
import 'package:opencord/features/chat/command_input.dart';
import 'package:opencord/features/chat/emoji_picker.dart';
import 'package:opencord/features/chat/mentions.dart';
import 'package:opencord/features/chat/suggestion_list.dart';
import 'package:opencord/features/chat/suggestions.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/measured_height.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/oc_text_field.dart';
import 'package:opencord/ui/widgets/popover.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// How often "typing" is sent while typing (§6).
const typingInterval = Duration(seconds: 8);

/// The message box at the bottom of a text channel (§4.6).
/// The composer's height at rest: empty, one line, nothing above it. The
/// user panel and the call's control bar take it, so their top lines meet
/// the composer's at any text size; null until a composer has shown.
class RestingComposerHeight extends Notifier<double?> {
  @override
  double? build() => null;

  void set(double height) => state = height;
}

final restingComposerHeightProvider =
    NotifierProvider<RestingComposerHeight, double?>(RestingComposerHeight.new);

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

  /// A chat command being written (Phase 3 §19.6), in place of the text.
  CommandSpec? _command;

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

  /// "/" and a word at the start: the commands it could be.
  List<Suggestion>? _commandSuggestions(ServerData data) {
    final text = _text.text;
    final match = RegExp(r'^/(\S*)$').firstMatch(text);
    if (match == null ||
        _text.selection.baseOffset != text.length ||
        ref.read(composerProvider(_channel)).editing != null) {
      return null;
    }
    final voice = ref.read(voiceSessionProvider);
    return [
      for (final spec in matchingCommands(
        data.commands,
        match[1]!,
      ).take(maxSuggestions))
        CommandSuggestion(
          spec,
          unavailable: commandUnavailable(
            spec,
            data: data,
            channelId: _channel.channel,
            voice: voice,
            serverKey: _channel.server,
          ),
        ),
    ];
  }

  void _updateSuggestions() {
    final selection = _text.selection;
    final data = _data;
    final channel = data?.channels[_channel.channel];
    final commands = data == null ? null : _commandSuggestions(data);
    final token = commands != null
        ? (trigger: '/', query: _text.text.substring(1), start: 0)
        : selection.isCollapsed && data != null && channel != null
        ? activeToken(_text.text, selection.baseOffset)
        : null;
    if (token?.start != _dismissedAt) _dismissedAt = null;
    final suggestions = token == null || _dismissedAt != null
        ? const <Suggestion>[]
        : commands ?? suggest(data!, channel!, token);
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
    if (suggestion case CommandSuggestion(:final spec, :final unavailable)) {
      if (unavailable != null) {
        showOcToast(context, unavailable);
        return;
      }
      _text.clear();
      _popup.hide();
      setState(() => _command = spec);
      return;
    }
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

  Future<void> _checkPaste() async {
    final hasText = await Clipboard.hasStrings();
    if (!hasText) {
      final bytes = await ref.read(repositoryProvider).clipboardImage();
      if (bytes != null) {
        final now = ref.read(clockProvider)().toLocal();
        final y = now.year.toString().padLeft(4, '0');
        final m = now.month.toString().padLeft(2, '0');
        final d = now.day.toString().padLeft(2, '0');
        final h = now.hour.toString().padLeft(2, '0');
        final min = now.minute.toString().padLeft(2, '0');
        final s = now.second.toString().padLeft(2, '0');
        final filename = 'pasted-image-$y$m$d-$h$min$s.png';
        final id = '${now.microsecondsSinceEpoch}_${filename.hashCode}';
        _composer.stageFile(
          StagedFile(
            id: id,
            name: filename,
            size: bytes.length,
            bytes: bytes,
            preview: bytes.length <= 20 * 1024 * 1024 ? bytes : null,
          ),
        );
      }
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
    if ((text.isEmpty && state.staged.isEmpty) ||
        _coolingDown ||
        state.hasProblem) {
      return;
    }
    final staged = List<StagedFile>.of(state.staged);
    _composer.sent();
    _composer.clearStaged();
    if (staged.isNotEmpty) {
      unawaited(
        _messages.sendWithFiles(encoded, staged, replyToId: state.replyTo?.id),
      );
    } else {
      unawaited(_messages.send(encoded, replyToId: state.replyTo?.id));
    }
    unawaited(widget.controller.scrollToEnd());
  }

  // Type to focus -------------------------------------------------------------

  /// Typing a character anywhere in the window focuses the composer and
  /// types it there (§16). Not while another text field, a menu or a
  /// dialog has the keyboard, and not for spaces, which press buttons.
  bool _typeToFocus(KeyEvent event) {
    if (event is! KeyDownEvent ||
        _focus.hasFocus ||
        !mounted ||
        _command != null) {
      return false;
    }
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
    final composer = Container(
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
                        messageSummary(about),
                        user: (id) => data.members[id]?.displayName,
                        channel: (id) => data.channels[id]?.name,
                      ),
                      onTap: () => widget.controller.jumpToMessage(about.id),
                      onClose: _composer.cancel,
                    ),
                  if (state.staged.isNotEmpty)
                    Padding(
                      padding: const EdgeInsets.only(bottom: OcSpace.s8),
                      child: StagedTiles(
                        files: state.staged,
                        limit: data.myUploadLimit,
                        onRemove: _composer.removeStaged,
                      ),
                    ),
                  Row(
                    crossAxisAlignment: CrossAxisAlignment.end,
                    children: [
                      Padding(
                        padding: const EdgeInsets.only(bottom: 2),
                        child: OcIconButton(
                          icon: OcIcons.attachFile,
                          tooltip:
                              data
                                  .permissionsIn(_channel.channel)
                                  .has(Permissions.attachFiles)
                              ? 'Attach files · up to ${formatFileSize(data.myUploadLimit)} each'
                              : "You can't attach files here",
                          onPressed:
                              data
                                  .permissionsIn(_channel.channel)
                                  .has(Permissions.attachFiles)
                              ? () async {
                                  final files = await openFiles();
                                  if (files.isNotEmpty) {
                                    await _composer.stagePaths(
                                      files.map((f) => f.path).toList(),
                                    );
                                  }
                                }
                              : null,
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
                        child:
                            hasText ||
                                editing != null ||
                                state.staged.isNotEmpty
                            ? OcIconButton(
                                icon: editing != null
                                    ? OcIcons.check
                                    : OcIcons.send,
                                tooltip: editing != null ? 'Save' : 'Send',
                                active: true,
                                activeStyle: OcActiveStyle.inverted,
                                onPressed:
                                    (cooldown != null && editing == null) ||
                                        state.hasProblem
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
    // Always wrapped, or the field would be rebuilt as typing starts.
    return MeasuredHeight(onChanged: _measured, child: composer);
  }

  /// Keeps the height only at rest: empty, and nothing above the field.
  void _measured(double height) {
    if (!mounted || _text.text.isNotEmpty) return;
    final state = ref.read(composerProvider(_channel));
    if (state.editing != null ||
        state.replyTo != null ||
        state.staged.isNotEmpty) {
      return;
    }
    ref.read(restingComposerHeightProvider.notifier).set(height);
  }

  Widget _input(BuildContext context, String channelName) {
    // One portal either way: swapping it for the command's own leaves its
    // semantics behind.
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
      child: switch (_command) {
        final spec? => CommandInput(
          key: ValueKey(spec.name),
          serverKey: _channel.server,
          channelId: _channel.channel,
          spec: spec,
          onClose: () {
            setState(() => _command = null);
            WidgetsBinding.instance.addPostFrameCallback((_) {
              if (mounted) _focus.requestFocus();
            });
          },
        ),
        null => KeyedSubtree(
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
      },
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
