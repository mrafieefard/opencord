import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/commands.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/chat/popup/composer_popup.dart';
import 'package:opencord/features/chat/popup/popup_rows.dart';
import 'package:opencord/features/chat/popup/source_icon.dart';
import 'package:opencord/features/chat/suggestions.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_spinner.dart';

/// Why a command can't be used here now, for its dimmed line in the list
/// (§19.6); null when it can. The server checks again.
String? commandUnavailable(
  CommandSpec spec, {
  required ServerData data,
  required int channelId,
  required VoiceSession voice,
  required String serverKey,
}) {
  if (spec.local) return null;
  if (!spec.allowedIn(channelId)) return 'Not in this channel';
  if (spec.feature == 'music') {
    final voiceChannel = voice.serverKey == serverKey ? voice.channelId : null;
    if (voiceChannel == null) return 'Join a voice channel first';
    if (!data.permissionsIn(voiceChannel).has(spec.permissions)) {
      return 'Needs ${spec.permissionSummary}';
    }
  }
  return null;
}

/// Commands whose name or alias starts with [typed], the app's own too.
List<CommandSpec> matchingCommands(CommandList list, String typed) {
  final query = typed.toLowerCase();
  return [
    for (final spec in [...list.commands, ...localCommands])
      if (spec.name.startsWith(query) ||
          spec.aliases.any((alias) => alias.startsWith(query)))
        spec,
  ];
}

/// How long typing must pause before suggestions are asked for (§19.6).
const suggestAfter = Duration(milliseconds: 400);

/// A command being written (§19.6): its name as a chip and its options as
/// inline fields. Tab moves between them, Enter runs, Escape cancels.
class CommandInput extends ConsumerStatefulWidget {
  const CommandInput({
    super.key,
    required this.serverKey,
    required this.channelId,
    required this.spec,
    required this.onClose,
    required this.popupLink,
    required this.popupWidth,
    required this.popupMaxHeight,
  });

  final String serverKey;
  final int channelId;
  final CommandSpec spec;

  /// Back to the plain composer.
  final VoidCallback onClose;

  /// Where the popup of an option's choices hangs: the same spot as the
  /// `/` list.
  final LayerLink popupLink;
  final double Function() popupWidth;
  final double Function() popupMaxHeight;

  @override
  ConsumerState<CommandInput> createState() => _CommandInputState();
}

class _CommandInputState extends ConsumerState<CommandInput> {
  late final _fields = {
    for (final option in widget.spec.options)
      if (option.kind != CommandOptionKind.boolean &&
          option.kind != CommandOptionKind.choice)
        option.name: TextEditingController(),
  };
  late final _focus = {
    for (final option in widget.spec.options) option.name: FocusNode(),
  };
  final _chipFocus = FocusNode();
  final _values = <String, String>{};

  /// A suggestion picked for an option: its value is sent, its name shown.
  final _picked = <String, CommandChoice>{};
  final _popup = OverlayPortalController();
  final _popupKey = GlobalKey<ComposerPopupState>();

  /// The field that last had the focus: a click on the composer returns
  /// there, so Backspace always reaches the command.
  FocusNode? _lastFocus;
  Timer? _debounce;
  String? _suggesting;
  var _choices = const <CommandChoice>[];
  var _selected = 0;
  var _running = false;
  var _asked = 0;

  /// Ctrl+A selected the whole command: Backspace or Delete removes it.
  var _allSelected = false;

  @override
  void initState() {
    super.initState();
    for (final MapEntry(key: name, value: controller) in _fields.entries) {
      controller.addListener(() => _typed(name, controller.text));
    }
    for (final node in [_chipFocus, ..._focus.values]) {
      node.addListener(() {
        if (node.hasFocus) _lastFocus = node;
      });
    }
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      _firstField.requestFocus();
    });
  }

  FocusNode get _firstField {
    final first = widget.spec.options.firstOrNull;
    return first == null ? _chipFocus : _focus[first.name]!;
  }

  void _refocus() => (_lastFocus ?? _firstField).requestFocus();

  @override
  void deactivate() {
    for (final node in _focus.values) {
      if (node.hasFocus) node.unfocus();
    }
    if (_chipFocus.hasFocus) _chipFocus.unfocus();
    super.deactivate();
  }

  @override
  void dispose() {
    _debounce?.cancel();
    for (final controller in _fields.values) {
      controller.dispose();
    }
    for (final node in _focus.values) {
      node.dispose();
    }
    _chipFocus.dispose();
    super.dispose();
  }

  void _close() {
    _hideSuggestions();
    for (final node in _focus.values) {
      if (node.hasFocus) node.unfocus();
    }
    if (_chipFocus.hasFocus) _chipFocus.unfocus();
    widget.onClose();
  }

  CommandOptionSpec? _option(String name) =>
      widget.spec.options.where((option) => option.name == name).firstOrNull;

  void _typed(String name, String text) {
    if (_picked[name]?.name != text) _picked.remove(name);
    if (_option(name)?.autocomplete != true) return;
    _debounce?.cancel();
    if (text.trim().length < 2) {
      _hideSuggestions();
      return;
    }
    _debounce = Timer(suggestAfter, () => _suggest(name, text));
  }

  Future<void> _suggest(String name, String text) async {
    // Outdated answers are dropped (§17.5).
    final asked = ++_asked;
    List<CommandChoice> found;
    try {
      found = await ref
          .read(repositoryProvider)
          .autocompleteCommand(
            widget.serverKey,
            widget.channelId,
            widget.spec.name,
            name,
            text,
            source: widget.spec.source,
          );
    } on RepoException {
      found = const [];
    }
    if (!mounted || asked != _asked) return;
    setState(() {
      _suggesting = name;
      _choices = found.take(5).toList();
      _selected = 0;
    });
    if (_choices.isEmpty) {
      _popup.hide();
    } else {
      _popup.show();
    }
  }

  void _hideSuggestions() {
    _asked++;
    if (_popup.isShowing) _popup.hide();
    if (_choices.isNotEmpty) setState(() => _choices = const []);
  }

  void _pickChoice(CommandChoice choice) {
    final name = _suggesting;
    if (name == null) return;
    _picked[name] = choice;
    _fields[name]?.value = TextEditingValue(
      text: choice.name,
      selection: TextSelection.collapsed(offset: choice.name.length),
    );
    _hideSuggestions();
  }

  /// The option values as sent.
  Map<String, String> _options() => {
    for (final option in widget.spec.options)
      if (switch (option.kind) {
            CommandOptionKind.boolean ||
            CommandOptionKind.choice => _values[option.name],
            _ =>
              _picked[option.name]?.value ?? _fields[option.name]?.text.trim(),
          }
          case final value? when value.isNotEmpty)
        option.name: value,
  };

  void _note(String content, {bool error = false}) {
    ref
        .read(
          ephemeralNotesProvider((
            server: widget.serverKey,
            channel: widget.channelId,
          )).notifier,
        )
        .add(
          EphemeralNote(
            channelId: widget.channelId,
            nonce: '',
            content: content,
            at: DateTime.now(),
            error: error,
          ),
        );
  }

  Future<void> _run() async {
    if (_running) return;
    final options = _options();
    final missing = widget.spec.options.where(
      (option) => option.required && !options.containsKey(option.name),
    );
    if (missing.isNotEmpty) {
      _focus[missing.first.name]?.requestFocus();
      return;
    }
    switch (widget.spec.name) {
      case 'volume' when widget.spec.local:
        final percent = int.tryParse(options['percent'] ?? '');
        if (percent == null || percent < 0 || percent > 200) {
          _note('The volume is from 0 to 200', error: true);
        } else {
          ref
              .read(musicListeningProvider.notifier)
              .update((l) => l.copyWith(volume: percent, muted: false));
          _note('Your music volume is $percent %');
        }
        _close();
        return;
      case 'music' when widget.spec.local:
        ref.read(rightPanelModeProvider.notifier).set(RightPanelMode.music);
        _close();
        return;
    }
    setState(() => _running = true);
    final nonce = '${DateTime.now().microsecondsSinceEpoch}';
    try {
      await ref
          .read(repositoryProvider)
          .executeCommand(
            widget.serverKey,
            widget.channelId,
            widget.spec.name,
            options,
            nonce: nonce,
            source: widget.spec.source,
          );
      _close();
    } on RepoException catch (error) {
      // Errors are only for who ran it (§17.3).
      _note(error.message, error: true);
      if (mounted) setState(() => _running = false);
    }
  }

  /// Backspace from where the focus is, as in Discord: on the chip, or at
  /// the start of the first option, it takes the command away; at the start
  /// of a later option it goes back to the end of the one before. Null when
  /// it is the field's own (there is text before the cursor to delete).
  VoidCallback? _backspace() {
    if (_chipFocus.hasFocus) return _close;
    final options = widget.spec.options;
    final at = options.indexWhere((option) => _focus[option.name]!.hasFocus);
    if (at < 0) return null;
    final field = _fields[options[at].name];
    final selection = field?.selection;
    // A text field's own deletion, unless the cursor is at its start.
    if (selection != null &&
        !(selection.isCollapsed && selection.baseOffset <= 0)) {
      return null;
    }
    if (at == 0) return _close;
    final previous = options[at - 1].name;
    return () {
      _focus[previous]!.requestFocus();
      final text = _fields[previous];
      if (text != null) {
        text.selection = TextSelection.collapsed(offset: text.text.length);
      }
    };
  }

  KeyEventResult _onKey(FocusNode node, KeyEvent event) {
    if (event is! KeyDownEvent && event is! KeyRepeatEvent) {
      return KeyEventResult.ignored;
    }
    final key = event.logicalKey;
    final keyboard = HardwareKeyboard.instance;
    if (key == LogicalKeyboardKey.keyA &&
        (keyboard.isControlPressed || keyboard.isMetaPressed)) {
      setState(() => _allSelected = true);
      return KeyEventResult.handled;
    }
    if (_allSelected) {
      if (key == LogicalKeyboardKey.backspace ||
          key == LogicalKeyboardKey.delete) {
        _close();
        return KeyEventResult.handled;
      }
      if (!_isModifier(key)) setState(() => _allSelected = false);
    }
    if (key == LogicalKeyboardKey.backspace) {
      if (_backspace() case final action?) {
        action();
        return KeyEventResult.handled;
      }
      return KeyEventResult.ignored;
    }
    final suggesting = _popup.isShowing && _choices.isNotEmpty;
    if (suggesting &&
        (key == LogicalKeyboardKey.arrowDown ||
            key == LogicalKeyboardKey.arrowUp)) {
      final step = key == LogicalKeyboardKey.arrowDown ? 1 : -1;
      setState(() => _selected = stepRow(_selected, step, _choices.length));
      _popupKey.currentState?.scrollToRow(_selected);
      return KeyEventResult.handled;
    }
    if (key == LogicalKeyboardKey.enter ||
        key == LogicalKeyboardKey.numpadEnter) {
      if (suggesting) {
        _pickChoice(_choices[_selected]);
      } else {
        unawaited(_run());
      }
      return KeyEventResult.handled;
    }
    if (key == LogicalKeyboardKey.escape) {
      if (suggesting) {
        _hideSuggestions();
      } else {
        _close();
      }
      return KeyEventResult.handled;
    }
    if (key == LogicalKeyboardKey.tab) {
      final order = [
        _chipFocus,
        for (final option in widget.spec.options) _focus[option.name]!,
      ];
      final at = order.indexWhere((each) => each.hasFocus);
      final step = HardwareKeyboard.instance.isShiftPressed ? -1 : 1;
      order[(at + step) % order.length].requestFocus();
      return KeyEventResult.handled;
    }
    return KeyEventResult.ignored;
  }

  Widget _chip(BuildContext context) {
    final colors = context.oc;
    final spec = widget.spec;
    final data = ref.read(serverProvider(widget.serverKey)).data;
    return Focus(
      focusNode: _chipFocus,
      child: GestureDetector(
        onTap: _chipFocus.requestFocus,
        child: Tooltip(
          message: spec.description,
          child: Container(
            padding: const EdgeInsets.symmetric(
              horizontal: OcSpace.s8,
              vertical: 3,
            ),
            decoration: BoxDecoration(
              color: colors.accent,
              borderRadius: BorderRadius.circular(6),
            ),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                if (spec.pluginId != null && data != null) ...[
                  CommandSourceIcon(
                    serverKey: widget.serverKey,
                    source: sourceOfCommand(spec, data),
                    size: 16,
                    radius: 5,
                    color: colors.accent,
                    background: colors.onAccent,
                  ),
                  const SizedBox(width: 5),
                ],
                Text(
                  '/${spec.name}',
                  style: OcText.bodyStrong.copyWith(color: colors.onAccent),
                ),
                if (_running) ...[
                  const SizedBox(width: OcSpace.s6),
                  OcSpinner(size: 12, color: colors.onAccent),
                ],
              ],
            ),
          ),
        ),
      ),
    );
  }

  Widget _choicesPopup(BuildContext context) => Positioned(
    left: 0,
    top: 0,
    child: CompositedTransformFollower(
      link: widget.popupLink,
      showWhenUnlinked: false,
      targetAnchor: Alignment.topLeft,
      followerAnchor: Alignment.bottomLeft,
      offset: const Offset(0, -PopupMetrics.gap),
      child: ComposerPopup(
        key: _popupKey,
        serverKey: widget.serverKey,
        width: widget.popupWidth(),
        maxHeight: widget.popupMaxHeight(),
        rows: [for (final choice in _choices) ChoiceSuggestion(choice)],
        title: _suggesting ?? '',
        rowHeight: PopupMetrics.tallRow,
        selected: _selected,
        onHover: (index) => setState(() => _selected = index),
        onPick: (row) {
          if (row is ChoiceSuggestion) _pickChoice(row.choice);
        },
      ),
    ),
  );

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final spec = widget.spec;
    return Focus(
      onKeyEvent: _onKey,
      child: Listener(
        // A click anywhere in the command ends Ctrl+A's selection.
        onPointerDown: (_) {
          if (_allSelected) setState(() => _allSelected = false);
        },
        child: OverlayPortal(
          controller: _popup,
          overlayChildBuilder: _choicesPopup,
          child: GestureDetector(
            behavior: HitTestBehavior.opaque,
            onTap: _refocus,
            child: Padding(
              padding: _inset,
              child: DecoratedBox(
                decoration: BoxDecoration(
                  color: _allSelected ? colors.selected : null,
                  borderRadius: BorderRadius.circular(8),
                ),
                child: SizedBox(
                  width: double.infinity,
                  child: Wrap(
                    spacing: OcSpace.s8,
                    runSpacing: OcSpace.s6,
                    crossAxisAlignment: WrapCrossAlignment.center,
                    children: [
                      _chip(context),
                      for (final option in spec.options) _field(option),
                      if (spec.options.isEmpty)
                        Text(
                          spec.description,
                          style: OcText.small.copyWith(color: colors.textMuted),
                        ),
                    ],
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }

  /// Keeps the float at its resting height for one line of fields.
  static const _inset = EdgeInsets.symmetric(
    horizontal: OcSpace.s8,
    vertical: 16.5,
  );

  Widget _field(CommandOptionSpec option) {
    final colors = context.oc;
    final label = Text(
      '${option.name}:',
      style: OcText.small.copyWith(color: colors.textSecondary),
    );
    switch (option.kind) {
      case CommandOptionKind.boolean || CommandOptionKind.choice:
        final choices = option.kind == CommandOptionKind.boolean
            ? const [
                CommandChoice(name: 'no', value: 'no'),
                CommandChoice(name: 'yes', value: 'yes'),
              ]
            : option.choices;
        return Focus(
          focusNode: _focus[option.name],
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              label,
              const SizedBox(width: OcSpace.s4),
              for (final choice in choices)
                Padding(
                  padding: const EdgeInsets.only(right: OcSpace.s4),
                  child: _ChoicePill(
                    label: choice.name,
                    selected:
                        (_values[option.name] ??
                            (option.kind == CommandOptionKind.boolean
                                ? 'no'
                                : null)) ==
                        choice.value,
                    onTap: () =>
                        setState(() => _values[option.name] = choice.value),
                  ),
                ),
            ],
          ),
        );
      default:
        return Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            label,
            const SizedBox(width: OcSpace.s4),
            IntrinsicWidth(
              child: ConstrainedBox(
                constraints: const BoxConstraints(minWidth: 60, maxWidth: 300),
                child: TextField(
                  controller: _fields[option.name],
                  focusNode: _focus[option.name],
                  maxLength: 400,
                  keyboardType: option.kind == CommandOptionKind.integer
                      ? TextInputType.number
                      : TextInputType.text,
                  inputFormatters: option.kind == CommandOptionKind.integer
                      ? [FilteringTextInputFormatter.digitsOnly]
                      : null,
                  style: OcText.body.copyWith(color: colors.text),
                  cursorColor: colors.text,
                  decoration: InputDecoration(
                    isDense: true,
                    counterText: '',
                    border: InputBorder.none,
                    hintText: option.description,
                    hintStyle: OcText.body.copyWith(color: colors.textMuted),
                    contentPadding: const EdgeInsets.symmetric(vertical: 4),
                  ),
                  onSubmitted: (_) => unawaited(_run()),
                ),
              ),
            ),
          ],
        );
    }
  }
}

final _modifiers = {
  LogicalKeyboardKey.control,
  LogicalKeyboardKey.shift,
  LogicalKeyboardKey.alt,
  LogicalKeyboardKey.meta,
};

bool _isModifier(LogicalKeyboardKey key) =>
    LogicalKeyboardKey.collapseSynonyms({key}).any(_modifiers.contains);

class _ChoicePill extends StatelessWidget {
  const _ChoicePill({
    required this.label,
    required this.selected,
    required this.onTap,
  });

  final String label;
  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Hoverable(
      onTap: onTap,
      semanticLabel: label,
      selected: selected,
      focusRadius: BorderRadius.circular(6),
      builder: (context, state) => Container(
        padding: const EdgeInsets.symmetric(
          horizontal: OcSpace.s6,
          vertical: 2,
        ),
        decoration: BoxDecoration(
          color: selected
              ? colors.selected
              : state.active
              ? colors.hover
              : null,
          border: Border.all(color: colors.border),
          borderRadius: BorderRadius.circular(6),
        ),
        child: Text(
          label,
          style: OcText.small.copyWith(
            color: selected ? colors.text : colors.textSecondary,
          ),
        ),
      ),
    );
  }
}
