import 'package:flutter/material.dart';

import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_switch.dart';
import 'package:opencord/ui/widgets/section_label.dart';

/// A bordered group of settings rows separated by dividers (§8).
class SettingsSection extends StatelessWidget {
  const SettingsSection({
    super.key,
    required this.children,
    this.title,
    this.footer,
  });

  final String? title;
  final String? footer;
  final List<Widget> children;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      mainAxisSize: MainAxisSize.min,
      children: [
        if (title != null)
          SectionLabel(
            title!,
            padding: const EdgeInsets.only(left: 4, bottom: OcSpace.s8),
          ),
        DecoratedBox(
          decoration: BoxDecoration(
            color: colors.surface,
            borderRadius: BorderRadius.circular(OcRadius.section),
            border: Border.all(color: colors.border),
          ),
          child: ClipRRect(
            borderRadius: BorderRadius.circular(OcRadius.section - 1),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              mainAxisSize: MainAxisSize.min,
              children: [
                for (var i = 0; i < children.length; i++) ...[
                  if (i > 0) Divider(height: 1, color: colors.border),
                  children[i],
                ],
              ],
            ),
          ),
        ),
        if (footer != null)
          Padding(
            padding: const EdgeInsets.fromLTRB(4, OcSpace.s8, 4, 0),
            child: Text(
              footer!,
              style: OcText.small.copyWith(color: colors.textMuted),
            ),
          ),
      ],
    );
  }
}

/// A row with a title, optional subtitle and something on the right.
/// Tappable rows get a chevron unless they bring their own trailing widget.
class SettingsRow extends StatelessWidget {
  const SettingsRow({
    super.key,
    required this.title,
    this.subtitle,
    this.icon,
    this.trailing,
    this.onTap,
    this.mono = false,
    this.toggled,
  });

  final String title;
  final String? subtitle;
  final IconData? icon;
  final Widget? trailing;
  final VoidCallback? onTap;

  /// For a switch row: whether it is on.
  final bool? toggled;

  /// Shows the subtitle in the mono face, for fingerprints and addresses.
  final bool mono;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    // A row that is itself the control speaks for its content, subtitle
    // included; otherwise what is in it (buttons too) is read as it is.
    final control = onTap != null || toggled != null;
    return Hoverable(
      onTap: onTap,
      focusRadius: BorderRadius.zero,
      semanticLabel: control ? [title, ?subtitle].join('\n') : null,
      toggled: toggled,
      cursor: SystemMouseCursors.basic,
      builder: (context, state) => AnimatedContainer(
        duration: OcMotion.of(context).hover,
        constraints: const BoxConstraints(minHeight: 52),
        padding: const EdgeInsets.symmetric(
          horizontal: OcSpace.s16,
          vertical: OcSpace.s10,
        ),
        color: onTap != null && state.active ? colors.hover : null,
        child: Row(
          children: [
            if (icon != null) ...[
              Icon(icon, size: OcSize.iconRow, color: colors.textSecondary),
              const SizedBox(width: OcSpace.s12),
            ],
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                mainAxisSize: MainAxisSize.min,
                children: [
                  Text(title, style: OcText.body.copyWith(color: colors.text)),
                  if (subtitle != null) ...[
                    const SizedBox(height: 2),
                    Text(
                      subtitle!,
                      style: (mono ? OcText.mono : OcText.small).copyWith(
                        color: colors.textMuted,
                      ),
                    ),
                  ],
                ],
              ),
            ),
            if (trailing != null) ...[
              const SizedBox(width: OcSpace.s12),
              // Its own node, so a button there is not lost in the row.
              if (control)
                trailing!
              else
                Semantics(container: true, child: trailing),
            ] else if (onTap != null)
              Icon(
                OcIcons.chevronRight,
                size: OcSize.iconRow,
                color: colors.textMuted,
              ),
          ],
        ),
      ),
    );
  }
}

class SettingsSwitchRow extends StatelessWidget {
  const SettingsSwitchRow({
    super.key,
    required this.title,
    required this.value,
    required this.onChanged,
    this.subtitle,
    this.icon,
  });

  final String title;
  final String? subtitle;
  final IconData? icon;
  final bool value;
  final ValueChanged<bool>? onChanged;

  @override
  Widget build(BuildContext context) {
    return SettingsRow(
      title: title,
      subtitle: subtitle,
      icon: icon,
      onTap: onChanged == null ? null : () => onChanged!(!value),
      toggled: value,
      trailing: ExcludeSemantics(
        child: OcSwitch(value: value, onChanged: onChanged),
      ),
    );
  }
}

@immutable
class ChoiceCardOption<T> {
  const ChoiceCardOption({
    required this.value,
    required this.label,
    this.description,
    this.icon,
    this.preview,
  });

  final T value;
  final String label;
  final String? description;
  final IconData? icon;

  /// A picture of the choice, like a miniature of a theme.
  final Widget? preview;
}

/// Mutually exclusive cards; the chosen one has a 1.5 px text-colored
/// border (§4.11, §8.1).
class SettingsChoiceCards<T> extends StatelessWidget {
  const SettingsChoiceCards({
    super.key,
    required this.options,
    required this.value,
    required this.onChanged,
  });

  final List<ChoiceCardOption<T>> options;
  final T value;
  final ValueChanged<T>? onChanged;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final motion = OcMotion.of(context);
    return Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        for (var i = 0; i < options.length; i++) ...[
          if (i > 0) const SizedBox(width: OcSpace.s12),
          Expanded(
            child: Semantics(
              inMutuallyExclusiveGroup: true,
              checked: options[i].value == value,
              child: Hoverable(
                onTap: onChanged == null
                    ? null
                    : () => onChanged!(options[i].value),
                focusRadius: BorderRadius.circular(OcRadius.section),
                semanticLabel: options[i].label,
                builder: (context, state) {
                  final option = options[i];
                  final chosen = option.value == value;
                  return AnimatedContainer(
                    duration: motion.hover,
                    padding: const EdgeInsets.all(OcSpace.s12),
                    decoration: BoxDecoration(
                      color: state.active ? colors.hover : colors.surface,
                      borderRadius: BorderRadius.circular(OcRadius.section),
                      border: Border.all(
                        color: chosen ? colors.text : colors.border,
                        width: chosen ? 1.5 : 1,
                      ),
                    ),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        if (option.preview != null) ...[
                          option.preview!,
                          const SizedBox(height: OcSpace.s10),
                        ],
                        Row(
                          children: [
                            if (option.icon != null) ...[
                              Icon(
                                option.icon,
                                size: OcSize.iconRow,
                                color: chosen
                                    ? colors.text
                                    : colors.textSecondary,
                              ),
                              const SizedBox(width: OcSpace.s8),
                            ],
                            Expanded(
                              child: Text(
                                option.label,
                                style:
                                    (chosen ? OcText.bodyStrong : OcText.body)
                                        .copyWith(color: colors.text),
                              ),
                            ),
                          ],
                        ),
                        if (option.description != null) ...[
                          const SizedBox(height: 2),
                          Text(
                            option.description!,
                            style: OcText.small.copyWith(
                              color: colors.textMuted,
                            ),
                          ),
                        ],
                      ],
                    ),
                  );
                },
              ),
            ),
          ),
        ],
      ],
    );
  }
}
