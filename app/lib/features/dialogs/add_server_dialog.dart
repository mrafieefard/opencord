import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/format.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/inline_error.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/oc_spinner.dart';
import 'package:opencord/ui/widgets/oc_text_field.dart';

/// Opens the add server dialog (§4.11).
/// [link] fills in the address, for invite links opened from outside.
Future<void> showAddServer(BuildContext context, {String? link}) =>
    showOcDialog<void>(
      context: context,
      builder: (context) => AddServerDialog(link: link),
    );

enum _Step { input, verify, connecting }

/// One dialog, three steps (§4.11): the link or address (with the owner's
/// claim token tucked under "I'm the owner"), verifying the fingerprint
/// when the link has none, and connecting. It closes on the new server.
class AddServerDialog extends ConsumerStatefulWidget {
  const AddServerDialog({super.key, this.link});

  static const double width = 480;

  /// Filled in at the start.
  final String? link;

  @override
  ConsumerState<AddServerDialog> createState() => _AddServerDialogState();
}

class _AddServerDialogState extends ConsumerState<AddServerDialog> {
  late final _link = TextEditingController(text: widget.link);
  final _claim = TextEditingController();
  var _step = _Step.input;
  var _owner = false;
  var _busy = false;
  String? _error;
  ServerNeedsTrust? _trust;

  @override
  void dispose() {
    _link.dispose();
    _claim.dispose();
    super.dispose();
  }

  OpencordRepository get _repository => ref.read(repositoryProvider);

  String? get _claimToken {
    final token = _claim.text.trim();
    return _owner && token.isNotEmpty ? token : null;
  }

  /// The host the input names, for "Connecting to host…".
  String get _host {
    final address = _trust?.address ?? _link.text.trim();
    final match = RegExp(r'^(?:opencord://)?([^/:#\s]+)').firstMatch(address);
    return match?.group(1) ?? address;
  }

  Future<void> _join() async {
    final input = _link.text.trim();
    if (input.isEmpty) {
      setState(() => _error = 'Paste an invite link or an address.');
      return;
    }
    setState(() {
      _busy = true;
      _error = null;
    });
    await _add(input, onFail: _Step.input);
  }

  Future<void> _trustAndConnect() async {
    final trust = _trust;
    if (trust == null) return;
    setState(() {
      _step = _Step.connecting;
      _error = null;
    });
    try {
      await _repository.trustFingerprint(trust.address, trust.fingerprint);
    } on RepoException catch (error) {
      _fail(error.message, _Step.verify);
      return;
    }
    await _add(_link.text.trim(), onFail: _Step.verify);
  }

  Future<void> _add(String input, {required _Step onFail}) async {
    try {
      final result = await _repository.addServer(
        input,
        claimToken: _claimToken,
      );
      if (!mounted) return;
      switch (result) {
        case ServerAdded(:final server):
          ref.read(navigationProvider.notifier).openServer(server.key);
          Navigator.pop(context);
        case ServerNeedsTrust():
          setState(() {
            _trust = result;
            _step = _Step.verify;
            _busy = false;
          });
      }
    } on RepoException catch (error) {
      _fail(error.message, onFail);
    }
  }

  void _fail(String message, _Step step) {
    if (!mounted) return;
    setState(() {
      _error = message;
      _step = step;
      _busy = false;
    });
  }

  @override
  Widget build(BuildContext context) => switch (_step) {
    _Step.input => _input(context),
    _Step.verify => _verify(context),
    _Step.connecting => OcDialog(
      title: 'Add a server',
      width: AddServerDialog.width,
      child: Row(
        children: [
          const OcSpinner(),
          const SizedBox(width: OcSpace.s12),
          Expanded(child: Text('Connecting to $_host…')),
        ],
      ),
    ),
  };

  Widget _input(BuildContext context) {
    final colors = context.oc;
    return OcDialog(
      title: 'Add a server',
      width: AddServerDialog.width,
      actions: [
        OcButton(label: 'Cancel', onPressed: () => Navigator.pop(context)),
        OcButton.primary(label: 'Join', busy: _busy, onPressed: _join),
      ],
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          const Text(
            'Paste an invite link, or the address of a server you host.',
          ),
          const SizedBox(height: OcSpace.s12),
          OcTextField(
            controller: _link,
            autofocus: true,
            hint: 'opencord://host:port/invite/…  or  host:port',
            semanticLabel: 'Invite link or address',
            onSubmitted: (_) => _join(),
          ),
          if (_error case final error?) ...[
            const SizedBox(height: OcSpace.s10),
            InlineError(error),
          ],
          const SizedBox(height: OcSpace.s12),
          Align(
            alignment: Alignment.centerLeft,
            child: Hoverable(
              onTap: () => setState(() => _owner = !_owner),
              semanticLabel: "I'm the owner",
              selected: _owner,
              focusRadius: BorderRadius.circular(OcRadius.quote),
              builder: (context, state) => Padding(
                padding: const EdgeInsets.symmetric(vertical: OcSpace.s4),
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Icon(
                      _owner ? OcIcons.expandLess : OcIcons.expandMore,
                      size: OcSize.iconRow,
                      color: colors.textSecondary,
                    ),
                    const SizedBox(width: OcSpace.s4),
                    Text(
                      "I'm the owner",
                      style: OcText.body.copyWith(
                        color: state.active
                            ? colors.text
                            : colors.textSecondary,
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ),
          if (_owner) ...[
            const SizedBox(height: OcSpace.s8),
            OcTextField(
              controller: _claim,
              mono: true,
              hint: 'Owner claim token',
              semanticLabel: 'Owner claim token',
              onSubmitted: (_) => _join(),
            ),
            const SizedBox(height: OcSpace.s6),
            Text(
              'The token your server printed the first time it started. '
              'It makes you the owner; you only need it once.',
              style: OcText.small.copyWith(color: colors.textMuted),
            ),
          ],
        ],
      ),
    );
  }

  Widget _verify(BuildContext context) {
    final colors = context.oc;
    final trust = _trust!;
    return OcDialog(
      title: 'Verify server',
      width: AddServerDialog.width,
      actions: [
        OcButton(label: 'Cancel', onPressed: () => Navigator.pop(context)),
        OcButton.primary(
          label: 'Trust and connect',
          autofocus: true,
          onPressed: _trustAndConnect,
        ),
      ],
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(
            '${trust.address} has no fingerprint in its address. Compare '
            'this with the fingerprint the server owner shared with you.',
          ),
          const SizedBox(height: OcSpace.s12),
          Container(
            padding: const EdgeInsets.all(OcSpace.s12),
            decoration: BoxDecoration(
              color: colors.chat,
              borderRadius: BorderRadius.circular(OcRadius.input),
              border: Border.all(color: colors.border),
            ),
            child: SelectableText(
              groupedFingerprint(trust.fingerprint),
              style: OcText.mono.copyWith(color: colors.text),
            ),
          ),
          if (_error case final error?) ...[
            const SizedBox(height: OcSpace.s10),
            InlineError(error),
          ],
        ],
      ),
    );
  }
}
