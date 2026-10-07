import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/inline_error.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/oc_switch.dart';
import 'package:opencord/ui/widgets/oc_text_field.dart';
import 'package:opencord/ui/widgets/section_label.dart';

/// "Release Notes!" → "release-notes": lower case, dashes for spaces, only
/// letters, digits, dashes and underscores (§4.11).
String kebabName(String name) => name
    .toLowerCase()
    .replaceAll(RegExp(r'\s+'), '-')
    .replaceAll(RegExp(r'[^\p{L}\p{N}\-_]', unicode: true), '')
    .replaceAll(RegExp('-{2,}'), '-');

/// Turns text channel names into kebab case as they are typed.
class KebabFormatter extends TextInputFormatter {
  const KebabFormatter();

  @override
  TextEditingValue formatEditUpdate(
    TextEditingValue oldValue,
    TextEditingValue newValue,
  ) {
    final text = kebabName(newValue.text);
    final caret =
        (newValue.selection.baseOffset - (newValue.text.length - text.length))
            .clamp(0, text.length);
    return TextEditingValue(
      text: text,
      selection: TextSelection.collapsed(offset: caret),
    );
  }
}

Future<void> showCreateChannel(
  BuildContext context, {
  required String serverKey,
  int? parentId,
}) => showOcDialog<void>(
  context: context,
  builder: (context) =>
      CreateChannelDialog(serverKey: serverKey, parentId: parentId),
);

/// Create channel (§4.11): the kind as three cards, the name (kebab case
/// for text channels) and whether it is private.
class CreateChannelDialog extends ConsumerStatefulWidget {
  const CreateChannelDialog({
    super.key,
    required this.serverKey,
    this.parentId,
  });

  final String serverKey;
  final int? parentId;

  @override
  ConsumerState<CreateChannelDialog> createState() =>
      _CreateChannelDialogState();
}

class _CreateChannelDialogState extends ConsumerState<CreateChannelDialog> {
  final _name = TextEditingController();
  var _kind = ChannelKind.text;
  var _private = false;
  var _busy = false;
  String? _error;

  @override
  void initState() {
    super.initState();
    _name.addListener(() => setState(() {}));
  }

  @override
  void dispose() {
    _name.dispose();
    super.dispose();
  }

  String get _cleanName {
    final name = _name.text.trim();
    return _kind.isTextLike
        ? kebabName(name).replaceAll(RegExp(r'^-+|-+$'), '')
        : name;
  }

  void _choose(ChannelKind kind) {
    setState(() {
      _kind = kind;
      if (kind.isTextLike) _name.text = kebabName(_name.text);
    });
  }

  Future<void> _create() async {
    final name = _cleanName;
    if (name.isEmpty) return;
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final channel = await ref
          .read(repositoryProvider)
          .createChannel(
            widget.serverKey,
            kind: _kind,
            name: name,
            parentId: widget.parentId,
            private: _private,
          );
      if (!mounted) return;
      if (channel.kind.isTextLike) {
        ref
            .read(navigationProvider.notifier)
            .openChannel(widget.serverKey, channel.id);
      }
      Navigator.pop(context);
    } on RepoException catch (error) {
      if (!mounted) return;
      setState(() {
        _error = error.message;
        _busy = false;
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final parent = widget.parentId == null
        ? null
        : ref.watch(
            serverProvider(
              widget.serverKey,
            ).select((s) => s.data?.channels[widget.parentId]),
          );
    final announcements = ref
        .read(repositoryProvider)
        .capabilities
        .announcementChannels;
    final kinds = [
      (ChannelKind.text, OcIcons.tag, 'Text', 'Messages, links and code'),
      (ChannelKind.voice, OcIcons.volumeUp, 'Voice', 'Talk, share your screen'),
      if (announcements)
        (
          ChannelKind.announcement,
          OcIcons.campaign,
          'Announcement',
          'News that only some can post',
        ),
    ];
    return OcDialog(
      title: parent == null
          ? 'Create channel'
          : 'Create channel in ${parent.name}',
      width: 520,
      actions: [
        OcButton(label: 'Cancel', onPressed: () => Navigator.pop(context)),
        OcButton.primary(
          label: 'Create channel',
          busy: _busy,
          onPressed: _cleanName.isEmpty ? null : _create,
        ),
      ],
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          const SectionLabel('Channel type'),
          const SizedBox(height: OcSpace.s8),
          for (final (kind, icon, title, description) in kinds) ...[
            _KindCard(
              icon: icon,
              title: title,
              description: description,
              selected: _kind == kind,
              onTap: () => _choose(kind),
            ),
            const SizedBox(height: OcSpace.s6),
          ],
          const SizedBox(height: OcSpace.s10),
          const SectionLabel('Channel name'),
          const SizedBox(height: OcSpace.s8),
          OcTextField(
            controller: _name,
            autofocus: true,
            hint: _kind.isTextLike ? 'new-channel' : 'New channel',
            prefixIcon: switch (_kind) {
              ChannelKind.voice => OcIcons.volumeUp,
              ChannelKind.announcement => OcIcons.campaign,
              _ => OcIcons.tag,
            },
            semanticLabel: 'Channel name',
            maxLength: 100,
            inputFormatters: _kind.isTextLike
                ? const [KebabFormatter()]
                : const [],
            onSubmitted: (_) => _create(),
          ),
          const SizedBox(height: OcSpace.s16),
          Row(
            children: [
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      'Private channel',
                      style: OcText.body.copyWith(color: colors.text),
                    ),
                    Text(
                      'Only the roles and members you choose can see it.',
                      style: OcText.small.copyWith(color: colors.textMuted),
                    ),
                  ],
                ),
              ),
              OcSwitch(
                value: _private,
                semanticLabel: 'Private channel',
                onChanged: (value) => setState(() => _private = value),
              ),
            ],
          ),
          if (_error case final error?) ...[
            const SizedBox(height: OcSpace.s12),
            InlineError(error),
          ],
        ],
      ),
    );
  }
}

/// A selectable channel kind; the chosen one has a 1.5 px `text` border
/// (§4.11).
class _KindCard extends StatelessWidget {
  const _KindCard({
    required this.icon,
    required this.title,
    required this.description,
    required this.selected,
    required this.onTap,
  });

  final IconData icon;
  final String title;
  final String description;
  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Hoverable(
      onTap: onTap,
      semanticLabel: '$title channel: $description',
      selected: selected,
      focusRadius: BorderRadius.circular(OcRadius.section),
      builder: (context, state) => AnimatedContainer(
        duration: OcMotion.of(context).hover,
        padding: const EdgeInsets.symmetric(
          horizontal: OcSpace.s12,
          vertical: OcSpace.s10,
        ),
        decoration: BoxDecoration(
          color: state.active ? colors.hover : colors.chat,
          borderRadius: BorderRadius.circular(OcRadius.section),
          border: Border.all(
            color: selected ? colors.text : colors.border,
            width: selected ? 1.5 : 1,
          ),
        ),
        child: Row(
          children: [
            Icon(icon, size: OcSize.iconButton, color: colors.text),
            const SizedBox(width: OcSpace.s12),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    title,
                    style: OcText.bodyStrong.copyWith(color: colors.text),
                  ),
                  Text(
                    description,
                    style: OcText.small.copyWith(color: colors.textSecondary),
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

Future<void> showCreateCategory(
  BuildContext context, {
  required String serverKey,
}) => showOcDialog<void>(
  context: context,
  builder: (context) => _CreateCategoryDialog(serverKey: serverKey),
);

class _CreateCategoryDialog extends ConsumerStatefulWidget {
  const _CreateCategoryDialog({required this.serverKey});

  final String serverKey;

  @override
  ConsumerState<_CreateCategoryDialog> createState() =>
      _CreateCategoryDialogState();
}

class _CreateCategoryDialogState extends ConsumerState<_CreateCategoryDialog> {
  final _name = TextEditingController();
  var _busy = false;
  String? _error;

  @override
  void initState() {
    super.initState();
    _name.addListener(() => setState(() {}));
  }

  @override
  void dispose() {
    _name.dispose();
    super.dispose();
  }

  Future<void> _create() async {
    final name = _name.text.trim();
    if (name.isEmpty) return;
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      await ref
          .read(repositoryProvider)
          .createChannel(
            widget.serverKey,
            kind: ChannelKind.category,
            name: name,
          );
      if (mounted) Navigator.pop(context);
    } on RepoException catch (error) {
      if (!mounted) return;
      setState(() {
        _error = error.message;
        _busy = false;
      });
    }
  }

  @override
  Widget build(BuildContext context) => OcDialog(
    title: 'Create category',
    actions: [
      OcButton(label: 'Cancel', onPressed: () => Navigator.pop(context)),
      OcButton.primary(
        label: 'Create category',
        busy: _busy,
        onPressed: _name.text.trim().isEmpty ? null : _create,
      ),
    ],
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      mainAxisSize: MainAxisSize.min,
      children: [
        const Text('Categories group channels in the sidebar.'),
        const SizedBox(height: OcSpace.s12),
        OcTextField(
          controller: _name,
          autofocus: true,
          hint: 'Category name',
          semanticLabel: 'Category name',
          maxLength: 100,
          onSubmitted: (_) => _create(),
        ),
        if (_error case final error?) ...[
          const SizedBox(height: OcSpace.s12),
          InlineError(error),
        ],
      ],
    ),
  );
}
