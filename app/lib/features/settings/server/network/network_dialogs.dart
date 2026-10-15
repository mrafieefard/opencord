import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/network.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/widgets/inline_error.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/oc_text_field.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// Sends a network change and shows what went wrong, if anything. Returns
/// whether it worked.
Future<bool> changeNetwork(
  BuildContext context,
  WidgetRef ref,
  String serverKey,
  Future<NetworkInfo> Function(OpencordRepository repository, String server)
  action, {
  String? done,
}) async {
  try {
    await ref.read(networkProvider(serverKey).notifier).change(action);
    if (done != null && context.mounted) showOcToast(context, done);
    return true;
  } on RepoException catch (error) {
    if (context.mounted) showOcToast(context, error.message);
    return false;
  }
}

/// Asks for a port, 1 to 65535.
Future<int?> askPort(
  BuildContext context, {
  required String title,
  required String body,
  required int current,
}) async {
  final text = await showOcDialog<String>(
    context: context,
    builder: (_) => ValueDialog(
      title: title,
      body: body,
      initial: current == 0 ? '' : '$current',
      hint: 'Port',
      action: 'Save',
      digitsOnly: true,
      check: (value) {
        final port = int.tryParse(value);
        return port == null || port < 1 || port > 65535
            ? 'A port is a number from 1 to 65535.'
            : null;
      },
    ),
  );
  return text == null ? null : int.parse(text);
}

/// Asks for a list, one entry per line.
Future<List<String>?> askList(
  BuildContext context, {
  required String title,
  required String body,
  required List<String> current,
  required String hint,
  bool allowEmpty = true,
}) async {
  final text = await showOcDialog<String>(
    context: context,
    builder: (_) => ValueDialog(
      title: title,
      body: body,
      initial: current.join('\n'),
      hint: hint,
      action: 'Save',
      lines: 5,
      check: (value) => !allowEmpty && _lines(value).isEmpty
          ? 'At least one is needed.'
          : null,
    ),
  );
  return text == null ? null : _lines(text);
}

List<String> _lines(String text) => [
  for (final line in text.split(RegExp(r'[\n,]')))
    if (line.trim().isNotEmpty) line.trim(),
];

/// One value to edit: a text field, checked before it is saved.
class ValueDialog extends StatefulWidget {
  const ValueDialog({
    super.key,
    required this.title,
    required this.body,
    required this.initial,
    required this.hint,
    required this.action,
    this.check,
    this.lines = 1,
    this.digitsOnly = false,
  });

  final String title;
  final String body;
  final String initial;
  final String hint;
  final String action;

  /// Why the value cannot be saved, or null when it can.
  final String? Function(String value)? check;
  final int lines;
  final bool digitsOnly;

  @override
  State<ValueDialog> createState() => _ValueDialogState();
}

class _ValueDialogState extends State<ValueDialog> {
  late final _value = TextEditingController(text: widget.initial);
  String? _problem;

  @override
  void dispose() {
    _value.dispose();
    super.dispose();
  }

  void _save() {
    final value = _value.text.trim();
    final problem = widget.check?.call(value);
    if (problem != null) {
      setState(() => _problem = problem);
      return;
    }
    Navigator.pop(context, value);
  }

  @override
  Widget build(BuildContext context) {
    return OcDialog(
      title: widget.title,
      actions: [
        OcButton(label: 'Cancel', onPressed: () => Navigator.pop(context)),
        OcButton.primary(label: widget.action, onPressed: _save),
      ],
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(widget.body),
          const SizedBox(height: OcSpace.s16),
          OcTextField(
            controller: _value,
            autofocus: true,
            mono: true,
            hint: widget.hint,
            semanticLabel: widget.hint,
            maxLines: widget.lines,
            minLines: widget.lines,
            inputFormatters: widget.digitsOnly
                ? [FilteringTextInputFormatter.digitsOnly]
                : null,
            onChanged: (_) {
              if (_problem != null) setState(() => _problem = null);
            },
            onSubmitted: widget.lines == 1 ? (_) => _save() : null,
          ),
          if (_problem case final problem?) ...[
            const SizedBox(height: OcSpace.s8),
            InlineError(problem),
          ],
        ],
      ),
    );
  }
}

/// The public address, set by hand: IPv4 and IPv6, either may be empty.
class AddressDialog extends StatefulWidget {
  const AddressDialog({super.key, required this.address});

  final PublicAddress address;

  @override
  State<AddressDialog> createState() => _AddressDialogState();
}

class _AddressDialogState extends State<AddressDialog> {
  late final _v4 = TextEditingController(
    text: widget.address.overrideIpv4.isNotEmpty
        ? widget.address.overrideIpv4
        : widget.address.ipv4,
  );
  late final _v6 = TextEditingController(
    text: widget.address.overrideIpv6.isNotEmpty
        ? widget.address.overrideIpv6
        : widget.address.ipv6,
  );

  @override
  void dispose() {
    _v4.dispose();
    _v6.dispose();
    super.dispose();
  }

  void _save() =>
      Navigator.pop(context, (ipv4: _v4.text.trim(), ipv6: _v6.text.trim()));

  @override
  Widget build(BuildContext context) => OcDialog(
    title: 'Set the public address',
    actions: [
      OcButton(label: 'Cancel', onPressed: () => Navigator.pop(context)),
      OcButton.primary(label: 'Save', onPressed: _save),
    ],
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      mainAxisSize: MainAxisSize.min,
      children: [
        const Text(
          'The addresses people and Lavalink nodes reach this server at. '
          'Leave one empty to have it detected.',
        ),
        const SizedBox(height: OcSpace.s16),
        OcTextField(
          controller: _v4,
          autofocus: true,
          mono: true,
          hint: 'IPv4, for example 203.0.113.10',
          semanticLabel: 'IPv4 address',
          onSubmitted: (_) => _save(),
        ),
        const SizedBox(height: OcSpace.s8),
        OcTextField(
          controller: _v6,
          mono: true,
          hint: 'IPv6, for example 2001:db8::10',
          semanticLabel: 'IPv6 address',
          onSubmitted: (_) => _save(),
        ),
      ],
    ),
  );
}
