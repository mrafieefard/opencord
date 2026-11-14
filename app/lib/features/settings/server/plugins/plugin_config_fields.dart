import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/plugins.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/choice_chips.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/inline_error.dart';
import 'package:opencord/ui/widgets/oc_switch.dart';
import 'package:opencord/ui/widgets/oc_text_field.dart';
import 'package:opencord/ui/widgets/popover.dart';

/// Form fields for a plugin's configuration (§17.2, review focus §6).
class PluginConfigFields extends ConsumerWidget {
  const PluginConfigFields({
    super.key,
    required this.serverKey,
    required this.fields,
    required this.values,
    required this.onChanged,
    this.secretHint,
  });

  final String serverKey;
  final List<PluginConfigField> fields;
  final Map<String, String> values;
  final void Function(String key, String value) onChanged;
  final String? secretHint;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final serverData = ref.watch(serverProvider(serverKey)).data;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      mainAxisSize: MainAxisSize.min,
      children: [
        for (final field in fields) ...[
          Padding(
            padding: const EdgeInsets.symmetric(vertical: OcSpace.s6),
            child: _buildField(context, colors, serverData, field),
          ),
        ],
      ],
    );
  }

  Widget _buildField(
    BuildContext context,
    OcColors colors,
    ServerData? serverData,
    PluginConfigField field,
  ) {
    final val = values[field.key] ?? '';
    final labelText = field.required ? '${field.label} *' : field.label;

    switch (field.kind) {
      case PluginConfigKind.boolean:
        final boolVal = val.toLowerCase() == 'true';
        return Row(
          children: [
            Expanded(
              child: Text(
                labelText,
                style: OcText.body.copyWith(color: colors.text),
              ),
            ),
            OcSwitch(
              value: boolVal,
              onChanged: (newVal) =>
                  onChanged(field.key, newVal ? 'true' : 'false'),
            ),
          ],
        );

      case PluginConfigKind.choice:
        final currentChoice = val.isNotEmpty
            ? val
            : (field.choices.isNotEmpty ? field.choices.first : '');
        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(
              labelText,
              style: OcText.small.copyWith(
                color: colors.textSecondary,
                fontWeight: FontWeight.w600,
              ),
            ),
            const SizedBox(height: 6),
            ChoiceChips<String>(
              options: [for (final c in field.choices) (c, c)],
              value: currentChoice,
              onChanged: (newVal) => onChanged(field.key, newVal),
            ),
          ],
        );

      case PluginConfigKind.number:
        final hasError = val.isNotEmpty && num.tryParse(val) == null;
        return _ConfigTextField(
          field: field,
          value: val,
          keyboardType: TextInputType.number,
          hint: field.required ? 'Required number' : 'Number',
          hasError: hasError,
          onChanged: (newVal) => onChanged(field.key, newVal),
        );

      case PluginConfigKind.channel:
        final channels =
            serverData?.channels.values
                .where((c) => c.kind != ChannelKind.category)
                .toList() ??
            [];
        final options = [
          for (final ch in channels)
            _PickerOption(
              id: ch.id,
              name: '#${ch.name}',
              icon: ch.kind == ChannelKind.voice
                  ? OcIcons.volumeUp
                  : OcIcons.tag,
            ),
        ];
        return _ConfigPicker(
          field: field,
          val: val,
          options: options,
          placeholder: 'Select channel...',
          defaultIcon: OcIcons.tag,
          onChanged: (newId) => onChanged(field.key, newId),
        );

      case PluginConfigKind.role:
        final roles = serverData?.roles.values.toList() ?? [];
        final options = [
          for (final r in roles)
            _PickerOption(
              id: r.id,
              name: r.name,
              color: r.color != 0 ? Color(r.color | 0xFF000000) : null,
              icon: OcIcons.badge,
            ),
        ];
        return _ConfigPicker(
          field: field,
          val: val,
          options: options,
          placeholder: 'Select role...',
          defaultIcon: OcIcons.badge,
          onChanged: (newId) => onChanged(field.key, newId),
        );

      case PluginConfigKind.user:
        final members = serverData?.members.values.toList() ?? [];
        final options = [
          for (final m in members)
            _PickerOption(
              id: m.user.id,
              name: m.displayName,
              icon: OcIcons.person,
            ),
        ];
        return _ConfigPicker(
          field: field,
          val: val,
          options: options,
          placeholder: 'Select user...',
          defaultIcon: OcIcons.person,
          onChanged: (newId) => onChanged(field.key, newId),
        );

      case PluginConfigKind.secret:
        return _ConfigTextField(
          field: field,
          value: val,
          obscure: true,
          hint: secretHint ?? (field.required ? 'Required secret' : null),
          onChanged: (newVal) => onChanged(field.key, newVal),
        );

      case PluginConfigKind.text:
        return _ConfigTextField(
          field: field,
          value: val,
          hint: field.required ? 'Required' : null,
          onChanged: (newVal) => onChanged(field.key, newVal),
        );
    }
  }
}

class _PickerOption {
  const _PickerOption({
    required this.id,
    required this.name,
    this.icon,
    this.color,
  });

  final int id;
  final String name;
  final IconData? icon;
  final Color? color;
}

class _ConfigPicker extends StatelessWidget {
  const _ConfigPicker({
    required this.field,
    required this.val,
    required this.options,
    required this.placeholder,
    required this.defaultIcon,
    required this.onChanged,
  });

  final PluginConfigField field;
  final String val;
  final List<_PickerOption> options;
  final String placeholder;
  final IconData defaultIcon;
  final ValueChanged<String> onChanged;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final labelText = field.required ? '${field.label} *' : field.label;
    final selectedId = int.tryParse(val);
    final selected = options.where((o) => o.id == selectedId).firstOrNull;
    final displayText = selected != null
        ? selected.name
        : (val.isNotEmpty ? val : placeholder);

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisSize: MainAxisSize.min,
      children: [
        Text(
          labelText,
          style: OcText.small.copyWith(
            color: colors.textSecondary,
            fontWeight: FontWeight.w600,
          ),
        ),
        const SizedBox(height: 4),
        Builder(
          builder: (triggerContext) => Hoverable(
            onTap: () async {
              final picked = await showPopover<int>(
                context: triggerContext,
                anchor: globalRectOf(triggerContext),
                builder: (popoverCtx) => ConstrainedBox(
                  constraints: const BoxConstraints(
                    maxWidth: 260,
                    maxHeight: 280,
                  ),
                  child: ListView.builder(
                    shrinkWrap: true,
                    itemCount: options.length,
                    itemBuilder: (ctx, i) {
                      final opt = options[i];
                      return Hoverable(
                        onTap: () => Navigator.of(popoverCtx).pop(opt.id),
                        builder: (ctx, state) => Container(
                          padding: const EdgeInsets.symmetric(
                            horizontal: OcSpace.s12,
                            vertical: OcSpace.s8,
                          ),
                          color: state.hovered ? colors.hover : null,
                          child: Row(
                            children: [
                              if (opt.icon != null) ...[
                                Icon(
                                  opt.icon,
                                  size: 16,
                                  color: opt.color ?? colors.textSecondary,
                                ),
                                const SizedBox(width: OcSpace.s8),
                              ],
                              Expanded(
                                child: Text(
                                  opt.name,
                                  style: OcText.body.copyWith(
                                    color: opt.color ?? colors.text,
                                  ),
                                ),
                              ),
                            ],
                          ),
                        ),
                      );
                    },
                  ),
                ),
              );
              if (picked != null) {
                onChanged(picked.toString());
              }
            },
            builder: (ctx, state) => Container(
              padding: const EdgeInsets.symmetric(
                horizontal: OcSpace.s12,
                vertical: OcSpace.s10,
              ),
              decoration: BoxDecoration(
                color: state.hovered ? colors.hover : colors.surface,
                borderRadius: BorderRadius.circular(OcRadius.input),
                border: Border.all(color: colors.border),
              ),
              child: Row(
                children: [
                  Icon(
                    selected?.icon ?? defaultIcon,
                    size: 16,
                    color: selected?.color ?? colors.textSecondary,
                  ),
                  const SizedBox(width: OcSpace.s8),
                  Expanded(
                    child: Text(
                      displayText,
                      style: OcText.body.copyWith(
                        color: selected != null
                            ? (selected.color ?? colors.text)
                            : colors.textMuted,
                      ),
                    ),
                  ),
                  Icon(
                    OcIcons.expandMore,
                    size: 16,
                    color: colors.textSecondary,
                  ),
                ],
              ),
            ),
          ),
        ),
      ],
    );
  }
}

class _ConfigTextField extends StatefulWidget {
  const _ConfigTextField({
    required this.field,
    required this.value,
    required this.onChanged,
    this.obscure = false,
    this.keyboardType,
    this.hint,
    this.hasError = false,
  });

  final PluginConfigField field;
  final String value;
  final ValueChanged<String> onChanged;
  final bool obscure;
  final TextInputType? keyboardType;
  final String? hint;
  final bool hasError;

  @override
  State<_ConfigTextField> createState() => _ConfigTextFieldState();
}

class _ConfigTextFieldState extends State<_ConfigTextField> {
  late final TextEditingController _controller;

  @override
  void initState() {
    super.initState();
    _controller = TextEditingController(text: widget.value);
  }

  @override
  void didUpdateWidget(covariant _ConfigTextField oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.value != _controller.text) {
      _controller.value = TextEditingValue(
        text: widget.value,
        selection: TextSelection.collapsed(offset: widget.value.length),
      );
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final labelText = widget.field.required
        ? '${widget.field.label} *'
        : widget.field.label;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisSize: MainAxisSize.min,
      children: [
        Text(
          labelText,
          style: OcText.small.copyWith(
            color: colors.textSecondary,
            fontWeight: FontWeight.w600,
          ),
        ),
        const SizedBox(height: 4),
        OcTextField(
          controller: _controller,
          obscure: widget.obscure,
          keyboardType: widget.keyboardType,
          hint: widget.hint,
          onChanged: widget.onChanged,
        ),
        if (widget.hasError) ...[
          const SizedBox(height: 4),
          const InlineError('Must be a valid number'),
        ],
      ],
    );
  }
}
