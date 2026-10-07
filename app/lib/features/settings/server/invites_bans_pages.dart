import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/misc.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/dialogs/invite_dialog.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/avatar.dart';
import 'package:opencord/ui/widgets/confirm_dialog.dart';
import 'package:opencord/ui/widgets/inline_error.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/oc_spinner.dart';
import 'package:opencord/ui/widgets/section_label.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// Loads a list from the repository, shows it, and loads it again after a
/// change.
abstract class _ListPageState<W extends ConsumerStatefulWidget, T>
    extends ConsumerState<W> {
  List<T>? items;
  String? error;

  Future<List<T>> fetch();

  @override
  void initState() {
    super.initState();
    reload();
  }

  Future<void> reload() async {
    try {
      final loaded = await fetch();
      if (mounted) {
        setState(() {
          items = loaded;
          error = null;
        });
      }
    } on RepoException catch (failure) {
      if (mounted) setState(() => error = failure.message);
    }
  }

  Future<void> guard(Future<void> Function() action) async {
    try {
      await action();
    } on RepoException catch (failure) {
      if (mounted) showOcToast(context, failure.message);
    }
    await reload();
  }

  /// The list in a bordered box, or what stands in for it.
  Widget box(
    BuildContext context, {
    required String empty,
    required Widget Function(T item) row,
  }) {
    final colors = context.oc;
    final items = this.items;
    if (error case final error?) return InlineError(error);
    if (items == null) {
      return const Padding(
        padding: EdgeInsets.all(OcSpace.s24),
        child: Center(child: OcSpinner()),
      );
    }
    return DecoratedBox(
      decoration: BoxDecoration(
        color: colors.surface,
        borderRadius: BorderRadius.circular(OcRadius.section),
        border: Border.all(color: colors.border),
      ),
      child: items.isEmpty
          ? Padding(
              padding: const EdgeInsets.all(OcSpace.s24),
              child: Text(
                empty,
                textAlign: TextAlign.center,
                style: OcText.small.copyWith(color: colors.textMuted),
              ),
            )
          : Column(
              children: [
                for (final (index, item) in items.indexed) ...[
                  if (index > 0) Divider(height: 1, color: colors.border),
                  row(item),
                ],
              ],
            ),
    );
  }
}

/// Invites (§8.2): every link with who made it, how often it was used and
/// when it stops working; copy, revoke or make a new one.
class ServerInvitesPage extends ConsumerStatefulWidget {
  const ServerInvitesPage({super.key, required this.serverKey});

  final String serverKey;

  @override
  ConsumerState<ServerInvitesPage> createState() => _ServerInvitesPageState();
}

class _ServerInvitesPageState
    extends _ListPageState<ServerInvitesPage, Invite> {
  @override
  Future<List<Invite>> fetch() =>
      ref.read(repositoryProvider).fetchInvites(widget.serverKey);

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final members = ref.watch(
      serverProvider(widget.serverKey).select((s) => s.data?.members),
    );
    final now = ref.read(clockProvider)();
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Row(
          children: [
            Expanded(
              child: Text(
                'Links stop working when they expire or reach their limit.',
                style: OcText.small.copyWith(color: colors.textSecondary),
              ),
            ),
            OcButton.primary(
              label: 'Create invite',
              icon: OcIcons.add,
              onPressed: () async {
                await showInvitePeople(context, serverKey: widget.serverKey);
                await reload();
              },
            ),
          ],
        ),
        const SizedBox(height: OcSpace.s16),
        const Padding(
          padding: EdgeInsets.symmetric(horizontal: OcSpace.s12),
          child: Row(
            children: [
              Expanded(flex: 3, child: SectionLabel('Code')),
              Expanded(flex: 4, child: SectionLabel('Made by')),
              Expanded(flex: 2, child: SectionLabel('Uses')),
              Expanded(flex: 3, child: SectionLabel('Expires')),
              SizedBox(width: 2 * OcSize.hitCompact),
            ],
          ),
        ),
        const SizedBox(height: OcSpace.s6),
        box(
          context,
          empty: 'No invites yet. Create one to bring people in.',
          row: (invite) => Padding(
            padding: const EdgeInsets.symmetric(
              horizontal: OcSpace.s12,
              vertical: OcSpace.s6,
            ),
            child: Row(
              children: [
                Expanded(
                  flex: 3,
                  child: Text(
                    invite.code,
                    style: OcText.mono.copyWith(color: colors.text),
                  ),
                ),
                Expanded(
                  flex: 4,
                  child: Text(
                    members?[invite.createdBy]?.displayName ?? 'Someone',
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: OcText.body.copyWith(color: colors.text),
                  ),
                ),
                Expanded(
                  flex: 2,
                  child: Text(
                    invite.maxUses == null
                        ? '${invite.uses}'
                        : '${invite.uses} / ${invite.maxUses}',
                    style: OcText.small.copyWith(color: colors.textSecondary),
                  ),
                ),
                Expanded(
                  flex: 3,
                  child: Text(
                    expiryLabel(invite.expiresAt, now),
                    style: OcText.small.copyWith(color: colors.textSecondary),
                  ),
                ),
                OcIconButton(
                  icon: OcIcons.contentCopy,
                  tooltip: 'Copy link',
                  size: OcIconButtonSize.compact,
                  onPressed: () {
                    Clipboard.setData(ClipboardData(text: invite.link));
                    showOcToast(context, 'Invite link copied');
                  },
                ),
                OcIconButton(
                  icon: OcIcons.delete,
                  tooltip: 'Revoke ${invite.code}',
                  size: OcIconButtonSize.compact,
                  onPressed: () async {
                    final revoke = await confirmAction(
                      context,
                      title: 'Revoke this invite?',
                      message:
                          'The link ${invite.code} stops working for everyone '
                          'who has it.',
                      action: 'Revoke invite',
                    );
                    if (!revoke) return;
                    await guard(
                      () => ref
                          .read(repositoryProvider)
                          .revokeInvite(widget.serverKey, invite.code),
                    );
                  },
                ),
              ],
            ),
          ),
        ),
      ],
    );
  }
}

/// Bans (§8.2): who is banned, why and since when, with Unban.
class ServerBansPage extends ConsumerStatefulWidget {
  const ServerBansPage({super.key, required this.serverKey});

  final String serverKey;

  @override
  ConsumerState<ServerBansPage> createState() => _ServerBansPageState();
}

class _ServerBansPageState extends _ListPageState<ServerBansPage, Ban> {
  @override
  Future<List<Ban>> fetch() =>
      ref.read(repositoryProvider).fetchBans(widget.serverKey);

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return box(
      context,
      empty: 'No one is banned.',
      row: (ban) => Padding(
        padding: const EdgeInsets.symmetric(
          horizontal: OcSpace.s12,
          vertical: OcSpace.s10,
        ),
        child: Row(
          children: [
            OcAvatar(
              id: '${ban.user.id}',
              name: ban.user.displayName,
              size: 32,
            ),
            const SizedBox(width: OcSpace.s12),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    ban.user.displayName,
                    style: OcText.body.copyWith(color: colors.text),
                  ),
                  Text(
                    [
                      'Since ${longDate(ban.createdAt)}',
                      if (ban.reason case final reason? when reason.isNotEmpty)
                        reason,
                    ].join(' · '),
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                    style: OcText.small.copyWith(color: colors.textMuted),
                  ),
                ],
              ),
            ),
            OcButton(
              label: 'Unban',
              dense: true,
              onPressed: () async {
                final unban = await confirmAction(
                  context,
                  title: 'Unban ${ban.user.displayName}?',
                  message: 'They can join again with an invite.',
                  action: 'Unban',
                );
                if (!unban) return;
                await guard(
                  () => ref
                      .read(repositoryProvider)
                      .unbanMember(widget.serverKey, ban.user.id),
                );
              },
            ),
          ],
        ),
      ),
    );
  }
}
