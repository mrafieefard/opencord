import 'package:flutter/material.dart';

import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';

/// Above the input while replying or editing (§4.6): the icon, a 2 px line,
/// "Reply to Kai" or "Edit message" over a one-line preview, and a close
/// button. A click on it shows the message.
class ComposerContextBar extends StatelessWidget {
  const ComposerContextBar({
    super.key,
    required this.editing,
    required this.title,
    required this.preview,
    required this.onClose,
    this.onTap,
  });

  final bool editing;
  final String title;
  final String preview;
  final VoidCallback onClose;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Padding(
      padding: const EdgeInsets.only(bottom: OcSpace.s6),
      child: Row(
        children: [
          SizedBox(
            width: OcSize.hitDefault,
            child: Icon(
              editing ? OcIcons.edit : OcIcons.reply,
              size: OcSize.iconRow,
              color: colors.textSecondary,
            ),
          ),
          Expanded(
            child: Hoverable(
              onTap: onTap,
              semanticLabel: '$title: $preview',
              builder: (context, state) => AnimatedContainer(
                duration: OcMotion.of(context).hover,
                padding: const EdgeInsets.symmetric(
                  horizontal: OcSpace.s8,
                  vertical: OcSpace.s2,
                ),
                decoration: BoxDecoration(
                  color: state.active ? colors.hover : null,
                  borderRadius: BorderRadius.circular(OcRadius.quote),
                ),
                child: IntrinsicHeight(
                  child: Row(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      Container(width: 2, color: colors.text),
                      const SizedBox(width: OcSpace.s8),
                      Expanded(
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            Text(
                              title,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: OcText.small.copyWith(
                                fontWeight: FontWeight.w600,
                                color: colors.text,
                              ),
                            ),
                            Text(
                              preview,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: OcText.small.copyWith(
                                color: colors.textSecondary,
                              ),
                            ),
                          ],
                        ),
                      ),
                    ],
                  ),
                ),
              ),
            ),
          ),
          OcIconButton(
            icon: OcIcons.close,
            tooltip: editing ? 'Cancel edit' : 'Cancel reply',
            size: OcIconButtonSize.compact,
            onPressed: onClose,
          ),
        ],
      ),
    );
  }
}

/// The message box's frame (§4.6), as Telegram's: the field runs the whole
/// width of the chat with no box of its own, and its buttons sit inside the
/// bar beside it. Reply, edit and staged files go above; [cover] lies over
/// the row (the voice recording bar).
class ComposerFrame extends StatelessWidget {
  const ComposerFrame({
    super.key,
    required this.field,
    this.above = const [],
    this.leading = const [],
    this.trailing = const [],
    this.cover,
  });

  final Widget field;
  final List<Widget> above;
  final List<Widget> leading;
  final List<Widget> trailing;
  final Widget? cover;

  /// The field's padding: one line of text plus this is as tall as a
  /// button plus [_buttonInset] on each side, so they share a middle.
  static const fieldPadding = EdgeInsets.symmetric(
    horizontal: OcSpace.s8,
    vertical: 15,
  );

  static const double _buttonInset = 7;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    Widget button(Widget child) => Padding(
      padding: const EdgeInsets.only(bottom: _buttonInset),
      child: child,
    );
    return DecoratedBox(
      decoration: BoxDecoration(
        color: colors.sidebar,
        border: Border(top: BorderSide(color: colors.border)),
      ),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          if (above.isNotEmpty)
            Padding(
              padding: const EdgeInsets.fromLTRB(
                OcSpace.s12,
                OcSpace.s8,
                OcSpace.s12,
                0,
              ),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: above,
              ),
            ),
          Stack(
            children: [
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: OcSpace.s8),
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.end,
                  children: [
                    for (final child in leading) button(child),
                    Expanded(child: field),
                    for (final child in trailing) button(child),
                  ],
                ),
              ),
              if (cover case final cover?)
                Positioned.fill(
                  child: ColoredBox(color: colors.sidebar, child: cover),
                ),
            ],
          ),
        ],
      ),
    );
  }
}

/// The composer field's look inside a [ComposerFrame]: no fill and no
/// border, as the whole bar is the box.
InputDecoration composerFieldDecoration(BuildContext context, String hint) =>
    InputDecoration(
      hintText: hint,
      hintStyle: OcText.body.copyWith(color: context.oc.textMuted),
      isDense: true,
      filled: false,
      hoverColor: Colors.transparent,
      contentPadding: ComposerFrame.fieldPadding,
      border: InputBorder.none,
      enabledBorder: InputBorder.none,
      focusedBorder: InputBorder.none,
      disabledBorder: InputBorder.none,
    );
