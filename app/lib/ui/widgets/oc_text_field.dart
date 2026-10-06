import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';

/// Decoration for every input: `hover` fill, no border at rest, a 1 px
/// border while focused. [radius] is 10 for inputs, 18 for the search pill
/// and 20 for the composer (§2.3, §4.6).
InputDecoration ocInputDecoration(
  BuildContext context, {
  String? hint,
  IconData? prefixIcon,
  Widget? suffix,
  double radius = OcRadius.input,
  EdgeInsets? padding,
}) {
  final colors = context.oc;
  final shape = OutlineInputBorder(
    borderRadius: BorderRadius.circular(radius),
    borderSide: BorderSide.none,
  );
  return InputDecoration(
    hintText: hint,
    hintStyle: OcText.body.copyWith(color: colors.textMuted),
    filled: true,
    fillColor: colors.hover,
    isDense: true,
    contentPadding:
        padding ??
        const EdgeInsets.symmetric(
          horizontal: OcSpace.s12,
          vertical: OcSpace.s10,
        ),
    prefixIcon: prefixIcon == null
        ? null
        : Icon(prefixIcon, size: OcSize.iconRow, color: colors.textMuted),
    prefixIconConstraints: const BoxConstraints(minWidth: 36, minHeight: 32),
    suffixIcon: suffix,
    suffixIconConstraints: const BoxConstraints(minWidth: 32, minHeight: 32),
    border: shape,
    enabledBorder: shape,
    disabledBorder: shape,
    focusedBorder: shape.copyWith(borderSide: BorderSide(color: colors.border)),
  );
}

class OcTextField extends StatelessWidget {
  const OcTextField({
    super.key,
    this.controller,
    this.focusNode,
    this.hint,
    this.prefixIcon,
    this.suffix,
    this.radius = OcRadius.input,
    this.mono = false,
    this.autofocus = false,
    this.readOnly = false,
    this.enabled = true,
    this.maxLines = 1,
    this.minLines,
    this.maxLength,
    this.onChanged,
    this.onSubmitted,
    this.textInputAction,
    this.inputFormatters,
    this.semanticLabel,
  });

  final TextEditingController? controller;
  final FocusNode? focusNode;
  final String? hint;
  final IconData? prefixIcon;
  final Widget? suffix;
  final double radius;

  /// JetBrains Mono, for codes, keys and fingerprints.
  final bool mono;
  final bool autofocus;
  final bool readOnly;
  final bool enabled;
  final int? maxLines;
  final int? minLines;
  final int? maxLength;
  final ValueChanged<String>? onChanged;
  final ValueChanged<String>? onSubmitted;
  final TextInputAction? textInputAction;
  final List<TextInputFormatter>? inputFormatters;
  final String? semanticLabel;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final field = TextField(
      controller: controller,
      focusNode: focusNode,
      autofocus: autofocus,
      readOnly: readOnly,
      enabled: enabled,
      maxLines: maxLines,
      minLines: minLines,
      maxLength: maxLength,
      maxLengthEnforcement: MaxLengthEnforcement.enforced,
      onChanged: onChanged,
      onSubmitted: onSubmitted,
      textInputAction: textInputAction,
      inputFormatters: inputFormatters,
      cursorWidth: 1.5,
      cursorColor: colors.text,
      style: (mono ? OcText.mono : OcText.body).copyWith(color: colors.text),
      decoration: ocInputDecoration(
        context,
        hint: hint,
        prefixIcon: prefixIcon,
        suffix: suffix,
        radius: radius,
      ).copyWith(counterText: ''),
    );
    if (semanticLabel == null) return field;
    return Semantics(label: semanticLabel, textField: true, child: field);
  }
}
