import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/network.dart';
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
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/oc_text_field.dart';
import 'package:opencord/ui/widgets/section_label.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// Adding a domain (Phase 3 plan §6.1), in two steps: its name and kind,
/// then what to set up for it: the DNS records, or the proxy.
class AddDomainDialog extends ConsumerStatefulWidget {
  const AddDomainDialog({super.key, required this.serverKey});

  final String serverKey;

  @override
  ConsumerState<AddDomainDialog> createState() => _AddDomainDialogState();
}

class _AddDomainDialogState extends ConsumerState<AddDomainDialog> {
  final _hostname = TextEditingController();
  final _port = TextEditingController();
  var _kind = DomainKind.own;
  var _busy = false;
  String? _error;

  /// Set once added: the second step.
  Domain? _added;

  @override
  void dispose() {
    _hostname.dispose();
    _port.dispose();
    super.dispose();
  }

  NetworkInfo? get _network =>
      ref.read(networkProvider(widget.serverKey)).value;

  Future<void> _add() async {
    final port = _port.text.trim().isEmpty ? null : int.tryParse(_port.text);
    if (_port.text.trim().isNotEmpty &&
        (port == null || port < 1 || port > 65535)) {
      setState(() => _error = 'A port is a number from 1 to 65535.');
      return;
    }
    setState(() {
      _busy = true;
      _error = null;
    });
    final hostname = _hostname.text.trim();
    try {
      await ref
          .read(networkProvider(widget.serverKey).notifier)
          .change(
            (repository, server) => repository.addDomain(
              server,
              hostname: hostname,
              kind: _kind,
              publicPort: port,
            ),
          );
      final domains = _network?.domains ?? const <Domain>[];
      if (!mounted) return;
      setState(() {
        _busy = false;
        _added = domains.isEmpty ? null : domains.last;
      });
      if (_added == null) Navigator.pop(context);
    } on RepoException catch (error) {
      if (!mounted) return;
      setState(() {
        _busy = false;
        _error = error.message;
      });
    }
  }

  Future<void> _checkNow() async {
    final added = _added;
    if (added == null) return;
    setState(() => _busy = true);
    try {
      await ref
          .read(networkProvider(widget.serverKey).notifier)
          .change(
            (repository, server) => repository.checkDomain(server, added.id),
          );
      if (mounted) Navigator.pop(context);
    } on RepoException catch (error) {
      if (!mounted) return;
      setState(() {
        _busy = false;
        _error = error.message;
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final added = _added;
    return added == null ? _first(context) : _second(context, added);
  }

  Widget _first(BuildContext context) {
    final colors = context.oc;
    final main = _network?.listeningPort ?? 7710;
    return OcDialog(
      title: 'Add a domain',
      width: 520,
      actions: [
        OcButton(label: 'Cancel', onPressed: () => Navigator.pop(context)),
        OcButton.primary(
          label: 'Next',
          busy: _busy,
          onPressed: _busy ? null : _add,
        ),
      ],
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          const Text(
            'People connect and are invited through the domain, and a '
            'certificate makes it trusted without asking them.',
          ),
          const SizedBox(height: OcSpace.s16),
          OcTextField(
            controller: _hostname,
            autofocus: true,
            mono: true,
            hint: 'chat.example.com',
            semanticLabel: 'Domain name',
            onSubmitted: (_) => _add(),
          ),
          const SizedBox(height: OcSpace.s16),
          const SectionLabel('How it reaches this server'),
          const SizedBox(height: OcSpace.s8),
          ChoiceChips<DomainKind>(
            options: const [
              (DomainKind.own, 'Its DNS points here'),
              (DomainKind.reverseProxy, 'Through a reverse proxy'),
            ],
            value: _kind,
            onChanged: (kind) => setState(() => _kind = kind),
          ),
          const SizedBox(height: OcSpace.s8),
          Text(
            _kind == DomainKind.own
                ? 'You create A and AAAA records pointing to this server; '
                      'Opencord checks them.'
                : 'Caddy, Nginx, Traefik or a tunnel such as ngrok handles '
                      'the certificate and forwards to Opencord.',
            style: OcText.small.copyWith(color: colors.textMuted),
          ),
          const SizedBox(height: OcSpace.s16),
          const SectionLabel('Port people connect to'),
          const SizedBox(height: OcSpace.s8),
          OcTextField(
            controller: _port,
            mono: true,
            hint: _kind == DomainKind.own ? '$main' : '443',
            semanticLabel: 'Port',
            inputFormatters: [FilteringTextInputFormatter.digitsOnly],
            onSubmitted: (_) => _add(),
          ),
          if (_error case final error?) ...[
            const SizedBox(height: OcSpace.s12),
            InlineError(error),
          ],
        ],
      ),
    );
  }

  Widget _second(BuildContext context, Domain domain) {
    final colors = context.oc;
    final network = _network;
    final address = network?.publicAddress ?? const PublicAddress();
    final local = network?.settings.localListenerPort ?? 7719;
    final records = <(String, String)>[
      if (address.ipv4.isNotEmpty) ('A', address.ipv4),
      if (address.ipv6.isNotEmpty) ('AAAA', address.ipv6),
    ];
    final own = !domain.kind.behindProxy;
    return OcDialog(
      title: own ? 'Create the DNS records' : 'Point the proxy here',
      width: 560,
      actions: [
        OcButton(label: 'Later', onPressed: () => Navigator.pop(context)),
        OcButton.primary(
          label: 'Check now',
          busy: _busy,
          onPressed: _busy ? null : _checkNow,
        ),
      ],
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          if (own) ...[
            Text(
              'At your DNS provider, create these records for '
              '${domain.hostname}, then check it.',
            ),
            const SizedBox(height: OcSpace.s12),
            if (records.isEmpty)
              const InlineError(
                "This server's public address is not known yet: detect it "
                'or set it under Public address first.',
              ),
            for (final (type, value) in records)
              _CopyLine(
                label: '$type  ${domain.hostname}  →  $value',
                copy: value,
              ),
            const SizedBox(height: OcSpace.s12),
            Text(
              'Using Cloudflare? Set the records to DNS only (grey cloud): '
              'its proxy forwards only web traffic on a few ports, and no '
              'UDP for voice.',
              style: OcText.small.copyWith(color: colors.textMuted),
            ),
          ] else ...[
            Text(
              'Have the proxy serve ${domain.hostname} with its certificate '
              'and forward to Opencord on this machine, WebSockets included. '
              'For Caddy:',
            ),
            const SizedBox(height: OcSpace.s12),
            _CopyLine(
              label:
                  '${domain.hostname} {\n    reverse_proxy 127.0.0.1:$local\n}',
              copy:
                  '${domain.hostname} {\n    reverse_proxy 127.0.0.1:$local\n}\n',
            ),
            const SizedBox(height: OcSpace.s12),
            Text(
              'Voice and music still need UDP 7711 and 7712 to reach this '
              'server directly: proxies carry no UDP.',
              style: OcText.small.copyWith(color: colors.textMuted),
            ),
          ],
          if (_error case final error?) ...[
            const SizedBox(height: OcSpace.s12),
            InlineError(error),
          ],
        ],
      ),
    );
  }
}

/// A value in the mono face, with a button that copies it.
class _CopyLine extends StatelessWidget {
  const _CopyLine({required this.label, required this.copy});

  final String label;
  final String copy;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Container(
      margin: const EdgeInsets.only(bottom: OcSpace.s8),
      padding: const EdgeInsets.fromLTRB(OcSpace.s12, 6, 4, 6),
      decoration: BoxDecoration(
        color: colors.chat,
        borderRadius: BorderRadius.circular(OcRadius.input),
        border: Border.all(color: colors.border),
      ),
      child: Row(
        children: [
          Expanded(
            child: SelectableText(
              label,
              style: OcText.mono.copyWith(color: colors.text),
            ),
          ),
          OcIconButton(
            icon: OcIcons.contentCopy,
            tooltip: 'Copy',
            size: OcIconButtonSize.compact,
            onPressed: () {
              Clipboard.setData(ClipboardData(text: copy));
              showOcToast(context, 'Copied');
            },
          ),
        ],
      ),
    );
  }
}
