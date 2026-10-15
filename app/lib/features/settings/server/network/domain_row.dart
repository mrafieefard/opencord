import 'package:clock/clock.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/network.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/settings/server/network/certificate_dialogs.dart';
import 'package:opencord/features/settings/server/network/network_dialogs.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/confirm_dialog.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
import 'package:opencord/ui/widgets/oc_spinner.dart';
import 'package:opencord/ui/widgets/oc_switch.dart';
import 'package:opencord/ui/widgets/popover.dart';
import 'package:opencord/ui/widgets/settings.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// One domain (Phase 3 plan §6.5): its name, kind, status and
/// certificate, the primary star, the Active switch, Check, and a menu.
class DomainRow extends ConsumerWidget {
  const DomainRow({super.key, required this.serverKey, required this.domain});

  final String serverKey;
  final Domain domain;

  Future<void> _update(
    BuildContext context,
    WidgetRef ref,
    DomainUpdate update, {
    String? done,
  }) => changeNetwork(
    context,
    ref,
    serverKey,
    (repository, server) => repository.updateDomain(server, domain.id, update),
    done: done,
  );

  Future<void> _check(BuildContext context, WidgetRef ref) => changeNetwork(
    context,
    ref,
    serverKey,
    (repository, server) => repository.checkDomain(server, domain.id),
  );

  Future<void> _copyInvite(BuildContext context, WidgetRef ref) async {
    try {
      final invite = await ref
          .read(repositoryProvider)
          .createInvite(
            serverKey,
            domainId: domain.id,
            expiresIn: const Duration(days: 7),
          );
      await Clipboard.setData(ClipboardData(text: invite.link));
      if (context.mounted) showOcToast(context, 'Invite link copied');
    } on RepoException catch (error) {
      if (context.mounted) showOcToast(context, error.message);
    }
  }

  Future<void> _delete(BuildContext context, WidgetRef ref) async {
    final confirmed = await confirmAction(
      context,
      title: 'Delete ${domain.hostname}?',
      message:
          'People can no longer connect through it, and invites made for it '
          'stop working.',
      action: 'Delete domain',
    );
    if (!confirmed || !context.mounted) return;
    await changeNetwork(
      context,
      ref,
      serverKey,
      (repository, server) => repository.deleteDomain(server, domain.id),
      done: '${domain.hostname} deleted',
    );
  }

  void _menu(BuildContext anchor, WidgetRef ref) {
    final certificate = domain.certificate;
    final canRequest = canRequestCertificate(domain, clock.now());
    final renewable = certificate.held && !certificate.uploaded;
    final failedProxy =
        domain.kind.behindProxy &&
        !domain.wasVerified &&
        domain.lastCheck?.result == CheckResult.failed;
    showOcMenu(
      context: anchor,
      position: globalRectOf(anchor).bottomLeft.translate(0, 4),
      entries: [
        if (canRequest)
          OcMenuItem(
            label: renewable ? 'Renew now' : 'Get certificate',
            icon: renewable ? OcIcons.refresh : OcIcons.lock,
            onSelected: () => getCertificate(anchor, ref, serverKey, domain),
          ),
        if (!domain.kind.behindProxy)
          OcMenuItem(
            label: 'Upload certificate',
            icon: OcIcons.upload,
            onSelected: () => showOcDialog<void>(
              context: anchor,
              builder: (_) =>
                  UploadCertificateDialog(serverKey: serverKey, domain: domain),
            ),
          ),
        if (domain.wasVerified && !domain.primary)
          OcMenuItem(
            label: 'Set as primary',
            icon: OcIcons.star,
            onSelected: () => _update(
              anchor,
              ref,
              const DomainUpdate(primary: true),
              done: '${domain.hostname} is the primary domain',
            ),
          ),
        OcMenuItem(
          label: 'Copy server address',
          icon: OcIcons.contentCopy,
          onSelected: () {
            Clipboard.setData(ClipboardData(text: domain.address));
            showOcToast(anchor, 'Address copied');
          },
        ),
        if (domain.active)
          OcMenuItem(
            label: 'Copy invite link',
            icon: OcIcons.link,
            onSelected: () => _copyInvite(anchor, ref),
          ),
        OcMenuItem(
          label: 'Check history',
          icon: OcIcons.history,
          onSelected: () => showOcDialog<void>(
            context: anchor,
            builder: (_) =>
                CheckHistoryDialog(serverKey: serverKey, domain: domain),
          ),
        ),
        if (failedProxy)
          OcMenuItem(
            label: 'Confirm it works',
            icon: OcIcons.check,
            onSelected: () => _update(
              anchor,
              ref,
              const DomainUpdate(confirm: true),
              done: '${domain.hostname} confirmed',
            ),
          ),
        const OcMenuDivider(),
        OcMenuItem(
          label: 'Delete',
          icon: OcIcons.delete,
          onSelected: () => _delete(anchor, ref),
        ),
      ],
    );
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final status = !domain.active && domain.wasVerified
        ? 'Inactive'
        : domain.status.label;
    final now = clock.now();
    final certificate = certificateLabel(domain, now);
    final lines = <String>[
      [
        domain.kind.label,
        status,
        certificate,
        if (domain.primary) 'Primary',
      ].join(' · '),
      if (domain.checking)
        'Checking…'
      else if (domain.lastCheck case final check?)
        [check.message, if (check.note.isNotEmpty) check.note].join(' '),
      ?certificateProgress(domain.certificate, now),
      if (domain.status == DomainStatus.dnsProblem &&
          domain.problemSince != null)
        'DNS problem since ${rowTime(domain.problemSince!, now)}',
      if (domain.kind.behindProxy)
        'Voice and music still need UDP to reach this server directly.',
      if (domain.active)
        switch (domain.users24h) {
          1 => '1 person connected through it in the last 24 hours',
          final count =>
            '$count people connected through it in the last '
                '24 hours',
        },
    ];
    final icon = switch ((domain.status, domain.active)) {
      (DomainStatus.dnsProblem, _) => OcIcons.warning,
      (DomainStatus.verified, true) => OcIcons.verifiedUser,
      (DomainStatus.verified, false) => OcIcons.remove,
      (DomainStatus.pending, _) => OcIcons.hourglassEmpty,
    };
    return SettingsRow(
      icon: icon,
      title: domain.publicPort == 443
          ? domain.hostname
          : '${domain.hostname}:${domain.publicPort}',
      subtitle: lines.join('\n'),
      trailing: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (domain.primary) ...[
            Tooltip(
              message: 'Primary domain',
              child: Icon(
                OcIcons.star,
                size: OcSize.iconRow,
                color: colors.text,
              ),
            ),
            const SizedBox(width: OcSpace.s8),
          ],
          if (domain.checking)
            const Padding(
              padding: EdgeInsets.symmetric(horizontal: OcSpace.s12),
              child: OcSpinner(),
            )
          else
            OcButton(
              label: 'Check',
              icon: OcIcons.check,
              dense: true,
              onPressed: () => _check(context, ref),
            ),
          const SizedBox(width: OcSpace.s12),
          Tooltip(
            message: domain.wasVerified
                ? (domain.active ? 'Active' : 'Inactive')
                : 'Check it first',
            child: Semantics(
              label: 'Active',
              toggled: domain.active,
              child: OcSwitch(
                value: domain.active,
                onChanged: domain.wasVerified
                    ? (on) => _update(context, ref, DomainUpdate(active: on))
                    : null,
              ),
            ),
          ),
          const SizedBox(width: OcSpace.s4),
          Builder(
            builder: (anchor) => OcIconButton(
              icon: OcIcons.moreHoriz,
              tooltip: 'More for ${domain.hostname}',
              size: OcIconButtonSize.compact,
              onPressed: () => _menu(anchor, ref),
            ),
          ),
        ],
      ),
    );
  }
}

/// A domain's last checks, newest first (plan §6.4).
class CheckHistoryDialog extends ConsumerWidget {
  const CheckHistoryDialog({
    super.key,
    required this.serverKey,
    required this.domain,
  });

  final String serverKey;
  final Domain domain;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    return OcDialog(
      title: 'Checks of ${domain.hostname}',
      showClose: true,
      width: 520,
      child: FutureBuilder<List<DomainCheck>>(
        future: ref.read(repositoryProvider).domainChecks(serverKey, domain.id),
        builder: (context, snapshot) {
          final checks = snapshot.data;
          if (snapshot.hasError) {
            return const Text('The checks could not be loaded.');
          }
          if (checks == null) {
            return const Center(child: OcSpinner(size: 20));
          }
          if (checks.isEmpty) return const Text('It has not been checked yet.');
          return ConstrainedBox(
            constraints: const BoxConstraints(maxHeight: 360),
            child: ListView.separated(
              shrinkWrap: true,
              itemCount: checks.length,
              separatorBuilder: (_, _) => Divider(color: colors.border),
              itemBuilder: (context, index) {
                final check = checks[index];
                return Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      '${fullTimestamp(check.checkedAt)} · ${check.result.label}',
                      style: OcText.bodyStrong.copyWith(color: colors.text),
                    ),
                    const SizedBox(height: 2),
                    Text(
                      [check.message, check.note].join(' ').trim(),
                      style: OcText.small.copyWith(color: colors.textSecondary),
                    ),
                  ],
                );
              },
            ),
          );
        },
      ),
    );
  }
}
