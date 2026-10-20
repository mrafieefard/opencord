import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/commands.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/music/music_artwork.dart';
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
  });

  final String serverKey;
  final int channelId;
  final CommandSpec spec;

  /// Back to the plain composer.
  final VoidCallback onClose;

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
  final _anchor = LayerLink();
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
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      final first = widget.spec.options.firstOrNull;
      (first == null ? _chipFocus : _focus[first.name]!).requestFocus();
    });
  }

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
      setState(() => _selected = (_selected + step) % _choices.length);
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
        child: CompositedTransformTarget(
          link: _anchor,
          child: OverlayPortal(
            controller: _popup,
            overlayChildBuilder: (context) => Positioned(
              left: 0,
              top: 0,
              child: CompositedTransformFollower(
                link: _anchor,
                showWhenUnlinked: false,
                targetAnchor: Alignment.topLeft,
                followerAnchor: Alignment.bottomLeft,
                offset: const Offset(0, -OcSpace.s6),
                child: _ChoiceList(
                  serverKey: widget.serverKey,
                  choices: _choices,
                  selected: _selected,
                  onHover: (index) => setState(() => _selected = index),
                  onPick: _pickChoice,
                ),
              ),
            ),
            child: Container(
              constraints: const BoxConstraints(minHeight: 40),
              padding: const EdgeInsets.symmetric(
                horizontal: OcSpace.s8,
                vertical: OcSpace.s6,
              ),
              decoration: BoxDecoration(
                color: _allSelected ? colors.selected : colors.surface,
                border: Border.all(color: colors.border),
                borderRadius: BorderRadius.circular(OcRadius.composer),
              ),
              child: Wrap(
                spacing: OcSpace.s8,
                runSpacing: OcSpace.s6,
                crossAxisAlignment: WrapCrossAlignment.center,
                children: [
                  Focus(
                    focusNode: _chipFocus,
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
                            Text(
                              '/${spec.name}',
                              style: OcText.bodyStrong.copyWith(
                                color: colors.onAccent,
                              ),
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
    );
  }

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

/// `/play`'s suggestions: at most 5, with small artwork (§19.6).
class _ChoiceList extends StatelessWidget {
  const _ChoiceList({
    required this.serverKey,
    required this.choices,
    required this.selected,
    required this.onHover,
    required this.onPick,
  });

  final String serverKey;
  final List<CommandChoice> choices;
  final int selected;
  final ValueChanged<int> onHover;
  final ValueChanged<CommandChoice> onPick;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Material(
      color: colors.elevated,
      borderRadius: BorderRadius.circular(OcRadius.menu),
      child: Container(
        width: 420,
        padding: const EdgeInsets.all(OcSpace.s4),
        decoration: BoxDecoration(
          border: Border.all(color: colors.border),
          borderRadius: BorderRadius.circular(OcRadius.menu),
        ),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            for (final (index, choice) in choices.indexed)
              MouseRegion(
                cursor: SystemMouseCursors.click,
                onEnter: (_) => onHover(index),
                child: GestureDetector(
                  onTap: () => onPick(choice),
                  child: Semantics(
                    button: true,
                    selected: index == selected,
                    label: choice.name,
                    child: Container(
                      height: 40,
                      padding: const EdgeInsets.symmetric(
                        horizontal: OcSpace.s8,
                      ),
                      decoration: BoxDecoration(
                        color: index == selected ? colors.selected : null,
                        borderRadius: BorderRadius.circular(OcRadius.menu - 2),
                      ),
                      child: Row(
                        children: [
                          if (choice.artworkId.isNotEmpty ||
                              choice.name.contains(' · ')) ...[
                            MusicArtwork(
                              serverKey: serverKey,
                              artworkId: choice.artworkId,
                              size: 28,
                              radius: 4,
                            ),
                            const SizedBox(width: OcSpace.s8),
                          ],
                          Expanded(
                            child: Text(
                              choice.name,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: OcText.body.copyWith(color: colors.text),
                            ),
                          ),
                        ],
                      ),
                    ),
                  ),
                ),
              ),
          ],
        ),
      ),
    );
  }
}
