import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:url_launcher/url_launcher.dart';

import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/features/links/app_links.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/oc_switch.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// Opens a web address in the browser; replaced in tests.
final linkLauncherProvider = Provider<Future<bool> Function(Uri uri)>(
  (ref) =>
      (uri) => launchUrl(uri, mode: LaunchMode.externalApplication),
);

/// Opens a link from a message (§4.5): `opencord://` links in the app,
/// web links straight away for trusted domains, otherwise after showing
/// where they go.
Future<void> openMessageLink(
  BuildContext context,
  WidgetRef ref,
  String url,
) async {
  if (parseAppLink(url) case final link?) {
    return openAppLink(context, ref, link);
  }
  final uri = Uri.tryParse(url);
  if (uri == null || !(uri.isScheme('http') || uri.isScheme('https'))) {
    showOcToast(context, 'That link cannot be opened.');
    return;
  }
  final domain = uri.host.toLowerCase();
  if (!ref.read(trustedLinkDomainsProvider).contains(domain)) {
    final choice = await showOcDialog<_LinkChoice>(
      context: context,
      builder: (context) => _LeaveDialog(url: url, domain: domain),
    );
    if (choice == null || !context.mounted) return;
    if (choice.trust) {
      ref.read(trustedLinkDomainsProvider.notifier).trust(domain);
    }
  }
  final opened = await ref.read(linkLauncherProvider)(uri);
  if (!opened && context.mounted) {
    showOcToast(context, 'No browser could open that link.');
  }
}

@immutable
class _LinkChoice {
  const _LinkChoice({required this.trust});

  final bool trust;
}

class _LeaveDialog extends StatefulWidget {
  const _LeaveDialog({required this.url, required this.domain});

  final String url;
  final String domain;

  @override
  State<_LeaveDialog> createState() => _LeaveDialogState();
}

class _LeaveDialogState extends State<_LeaveDialog> {
  var _trust = false;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return OcDialog(
      title: 'Leave Opencord?',
      actions: [
        OcButton(label: 'Cancel', onPressed: () => Navigator.pop(context)),
        OcButton.primary(
          label: 'Open link',
          onPressed: () => Navigator.pop(context, _LinkChoice(trust: _trust)),
        ),
      ],
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          const Text('This link opens in your browser:'),
          const SizedBox(height: OcSpace.s8),
          Container(
            padding: const EdgeInsets.all(OcSpace.s10),
            decoration: BoxDecoration(
              color: colors.chat,
              borderRadius: BorderRadius.circular(OcRadius.input),
              border: Border.all(color: colors.border),
            ),
            child: SelectableText(
              widget.url,
              style: OcText.mono.copyWith(color: colors.text),
            ),
          ),
          const SizedBox(height: OcSpace.s16),
          Row(
            children: [
              Expanded(
                child: Text(
                  'Open ${widget.domain} links without asking',
                  style: OcText.body.copyWith(color: colors.text),
                ),
              ),
              OcSwitch(
                value: _trust,
                semanticLabel: 'Open ${widget.domain} links without asking',
                onChanged: (value) => setState(() => _trust = value),
              ),
            ],
          ),
        ],
      ),
    );
  }
}
