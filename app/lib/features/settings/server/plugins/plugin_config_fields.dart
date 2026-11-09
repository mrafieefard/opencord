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
    dynamic serverData,
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
            TextField(
              controller: TextEditingController.fromValue(
                TextEditingValue(
                  text: val,
                  selection: TextSelection.collapsed(offset: val.length),
                ),
              ),
              keyboardType: TextInputType.number,
              onChanged: (newVal) => onChanged(field.key, newVal),
              decoration: ocInputDecoration(
                context,
                hint: field.required ? 'Required number' : 'Number',
              ),
            ),
            if (hasError) ...[
              const SizedBox(height: 4),
              const InlineError('Must be a valid number'),
            ],
          ],
        );

      case PluginConfigKind.channel:
        final channels = serverData?.channels.values.toList() ?? [];
        final selectedId = int.tryParse(val);
        final selectedChannel = selectedId != null
            ? serverData?.channels[selectedId]
            : null;
        final displayText = selectedChannel != null
            ? '#${selectedChannel.name}'
            : (val.isNotEmpty ? '#$val' : 'Select channel...');

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
                      child: ListView(
                        shrinkWrap: true,
                        children: [
                          for (final ch in channels)
                            Hoverable(
                              onTap: () => Navigator.of(popoverCtx).pop(ch.id),
                              builder: (ctx, state) => Container(
                                padding: const EdgeInsets.symmetric(
                                  horizontal: OcSpace.s12,
                                  vertical: OcSpace.s8,
                                ),
                                color: state.hovered ? colors.hover : null,
                                child: Row(
                                  children: [
                                    Icon(
                                      ch.kind == ChannelKind.voice
                                          ? OcIcons.volumeUp
                                          : OcIcons.tag,
                                      size: 16,
                                      color: colors.textSecondary,
                                    ),
                                    const SizedBox(width: OcSpace.s8),
                                    Expanded(
                                      child: Text(
                                        ch.name,
                                        style: OcText.body.copyWith(
                                          color: colors.text,
                                        ),
                                      ),
                                    ),
                                  ],
                                ),
                              ),
                            ),
                        ],
                      ),
                    ),
                  );
                  if (picked != null) {
                    onChanged(field.key, picked.toString());
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
                      Icon(OcIcons.tag, size: 16, color: colors.textSecondary),
                      const SizedBox(width: OcSpace.s8),
                      Expanded(
                        child: Text(
                          displayText,
                          style: OcText.body.copyWith(
                            color: selectedChannel != null
                                ? colors.text
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

      case PluginConfigKind.role:
        final roles = serverData?.rolesDescending ?? [];
        final selectedId = int.tryParse(val);
        final selectedRole = selectedId != null
            ? serverData?.roles[selectedId]
            : null;
        final displayText = selectedRole != null
            ? selectedRole.name
            : (val.isNotEmpty ? val : 'Select role...');

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
                      child: ListView(
                        shrinkWrap: true,
                        children: [
                          for (final role in roles)
                            Hoverable(
                              onTap: () =>
                                  Navigator.of(popoverCtx).pop(role.id),
                              builder: (ctx, state) => Container(
                                padding: const EdgeInsets.symmetric(
                                  horizontal: OcSpace.s12,
                                  vertical: OcSpace.s8,
                                ),
                                color: state.hovered ? colors.hover : null,
                                child: Row(
                                  children: [
                                    Icon(
                                      OcIcons.shieldPerson,
                                      size: 16,
                                      color: colors.textSecondary,
                                    ),
                                    const SizedBox(width: OcSpace.s8),
                                    Expanded(
                                      child: Text(
                                        role.name,
                                        style: OcText.body.copyWith(
                                          color: colors.text,
                                        ),
                                      ),
                                    ),
                                  ],
                                ),
                              ),
                            ),
                        ],
                      ),
                    ),
                  );
                  if (picked != null) {
                    onChanged(field.key, picked.toString());
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
                        OcIcons.shieldPerson,
                        size: 16,
                        color: colors.textSecondary,
                      ),
                      const SizedBox(width: OcSpace.s8),
                      Expanded(
                        child: Text(
                          displayText,
                          style: OcText.body.copyWith(
                            color: selectedRole != null
                                ? colors.text
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

      case PluginConfigKind.user:
        final members = serverData?.members.values.toList() ?? [];
        final selectedId = int.tryParse(val);
        final selectedName = selectedId != null
            ? (serverData?.authorOf(selectedId).name ?? val)
            : (val.isNotEmpty ? val : 'Select user...');

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
                      child: ListView(
                        shrinkWrap: true,
                        children: [
                          for (final m in members)
                            Hoverable(
                              onTap: () => Navigator.of(popoverCtx).pop(m.id),
                              builder: (ctx, state) => Container(
                                padding: const EdgeInsets.symmetric(
                                  horizontal: OcSpace.s12,
                                  vertical: OcSpace.s8,
                                ),
                                color: state.hovered ? colors.hover : null,
                                child: Row(
                                  children: [
                                    Icon(
                                      OcIcons.person,
                                      size: 16,
                                      color: colors.textSecondary,
                                    ),
                                    const SizedBox(width: OcSpace.s8),
                                    Expanded(
                                      child: Text(
                                        m.displayName,
                                        style: OcText.body.copyWith(
                                          color: colors.text,
                                        ),
                                      ),
                                    ),
                                  ],
                                ),
                              ),
                            ),
                        ],
                      ),
                    ),
                  );
                  if (picked != null) {
                    onChanged(field.key, picked.toString());
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
                        OcIcons.person,
                        size: 16,
                        color: colors.textSecondary,
                      ),
                      const SizedBox(width: OcSpace.s8),
                      Expanded(
                        child: Text(
                          selectedName,
                          style: OcText.body.copyWith(
                            color: selectedId != null
                                ? colors.text
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

      case PluginConfigKind.secret:
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
            TextField(
              controller: TextEditingController.fromValue(
                TextEditingValue(
                  text: val,
                  selection: TextSelection.collapsed(offset: val.length),
                ),
              ),
              obscureText: true,
              onChanged: (newVal) => onChanged(field.key, newVal),
              decoration: ocInputDecoration(
                context,
                hint: secretHint ?? (field.required ? 'Required secret' : null),
              ),
            ),
          ],
        );

      case PluginConfigKind.text:
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
            TextField(
              controller: TextEditingController.fromValue(
                TextEditingValue(
                  text: val,
                  selection: TextSelection.collapsed(offset: val.length),
                ),
              ),
              onChanged: (newVal) => onChanged(field.key, newVal),
              decoration: ocInputDecoration(
                context,
                hint: field.required ? 'Required' : null,
              ),
            ),
          ],
        );
    }
  }
}
