import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/misc.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/choice_chips.dart';
import 'package:opencord/ui/widgets/inline_error.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/section_label.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// Opens the invite people dialog (§4.11).
Future<void> showInvitePeople(
  BuildContext context, {
  required String serverKey,
}) => showOcDialog<void>(
  context: context,
  builder: (context) => InviteDialog(serverKey: serverKey),
);

/// A link ready to copy, made when the dialog opens; changing how long it
/// lasts or how often it works offers a new one.
class InviteDialog extends ConsumerStatefulWidget {
  const InviteDialog({super.key, required this.serverKey});

  final String serverKey;

  static const expiries = <(Duration?, String)>[
    (Duration(hours: 1), '1 hour'),
    (Duration(days: 1), '1 day'),
    (Duration(days: 7), '7 days'),
    (null, 'Never'),
  ];

  static const limits = <(int?, String)>[
    (null, 'No limit'),
    (1, '1 use'),
    (5, '5 uses'),
    (10, '10 uses'),
    (25, '25 uses'),
    (100, '100 uses'),
  ];

  @override
  ConsumerState<InviteDialog> createState() => _InviteDialogState();
}

class _InviteDialogState extends ConsumerState<InviteDialog> {
  Invite? _invite;
  String? _error;
  var _busy = false;
  Duration? _expiry = const Duration(days: 7);
  int? _limit;

  /// The options the shown link was made with.
  (Duration?, int?)? _madeWith;

  @override
  void initState() {
    super.initState();
    _create();
  }

  Future<void> _create() async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final invite = await ref
          .read(repositoryProvider)
          .createInvite(widget.serverKey, expiresIn: _expiry, maxUses: _limit);
      if (!mounted) return;
      setState(() {
        _invite = invite;
        _madeWith = (_expiry, _limit);
        _busy = false;
      });
    } on RepoException catch (error) {
      if (!mounted) return;
      setState(() {
        _error = error.message;
        _busy = false;
      });
    }
  }

  void _copy() {
    final invite = _invite;
    if (invite == null) return;
    Clipboard.setData(ClipboardData(text: invite.link));
    showOcToast(context, 'Invite link copied');
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final name =
        ref.watch(
          serverProvider(widget.serverKey).select((s) => s.data?.info.name),
        ) ??
        'this server';
    final changed = _madeWith != null && _madeWith != (_expiry, _limit);
    return OcDialog(
      title: 'Invite people to $name',
      width: 520,
      actions: [
        OcButton(label: 'Done', onPressed: () => Navigator.pop(context)),
      ],
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          Text('Anyone with this link can join $name.'),
          const SizedBox(height: OcSpace.s12),
          Row(
            children: [
              Expanded(
                child: Container(
                  height: 40,
                  alignment: Alignment.centerLeft,
                  padding: const EdgeInsets.symmetric(horizontal: OcSpace.s12),
                  decoration: BoxDecoration(
                    color: colors.chat,
                    borderRadius: BorderRadius.circular(OcRadius.input),
                    border: Border.all(color: colors.border),
                  ),
                  child: Text(
                    _invite?.link ?? 'Making a link…',
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: OcText.mono.copyWith(
                      color: _invite == null ? colors.textMuted : colors.text,
                    ),
                  ),
                ),
              ),
              const SizedBox(width: OcSpace.s8),
              OcButton.primary(
                label: 'Copy',
                icon: OcIcons.contentCopy,
                busy: _busy,
                autofocus: true,
                onPressed: _invite == null || changed ? null : _copy,
              ),
            ],
          ),
          if (_error case final error?) ...[
            const SizedBox(height: OcSpace.s10),
            InlineError(error),
          ],
          const SizedBox(height: OcSpace.s20),
          const SectionLabel('Expire after'),
          const SizedBox(height: OcSpace.s8),
          ChoiceChips<Duration?>(
            options: InviteDialog.expiries,
            value: _expiry,
            onChanged: (value) => setState(() => _expiry = value),
          ),
          const SizedBox(height: OcSpace.s16),
          const SectionLabel('Max uses'),
          const SizedBox(height: OcSpace.s8),
          ChoiceChips<int?>(
            options: InviteDialog.limits,
            value: _limit,
            onChanged: (value) => setState(() => _limit = value),
          ),
          if (changed) ...[
            const SizedBox(height: OcSpace.s16),
            Align(
              alignment: Alignment.centerLeft,
              child: OcButton(
                label: 'Make a new link',
                icon: OcIcons.refresh,
                busy: _busy,
                onPressed: _create,
              ),
            ),
          ],
          const SizedBox(height: OcSpace.s16),
          Text(
            'The link carries this server’s fingerprint, so whoever '
            'opens it connects to this server and not to an impostor.',
            style: OcText.small.copyWith(color: colors.textMuted),
          ),
        ],
      ),
    );
  }
}
