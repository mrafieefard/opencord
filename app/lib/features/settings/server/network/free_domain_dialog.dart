import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:url_launcher/url_launcher.dart';

import 'package:opencord/core/model/network.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/choice_chips.dart';
import 'package:opencord/ui/widgets/inline_error.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/oc_switch.dart';
import 'package:opencord/ui/widgets/oc_text_field.dart';
import 'package:opencord/ui/widgets/section_label.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// A free subdomain (Phase 3 plan §9.1): pick DuckDNS or deSEC, make an
/// account and a name there, paste the token and the name. The server then
/// points it here, keeps it pointed here, checks it and gets its
/// certificate.
class FreeDomainDialog extends ConsumerStatefulWidget {
  const FreeDomainDialog({super.key, required this.serverKey});

  final String serverKey;

  @override
  ConsumerState<FreeDomainDialog> createState() => _FreeDomainDialogState();
}

class _FreeDomainDialogState extends ConsumerState<FreeDomainDialog> {
  final _token = TextEditingController();
  final _name = TextEditingController();
  final _email = TextEditingController();
  var _provider = DnsProviderKind.duckdns;
  var _agreed = false;
  var _busy = false;
  String? _error;

  static const _sites = {
    DnsProviderKind.duckdns: (
      'https://www.duckdns.org',
      'Sign in at duckdns.org, add a subdomain, and copy the token shown at '
          'the top of the page.',
    ),
    DnsProviderKind.desec: (
      'https://desec.io',
      'Register at desec.io, create a domain under dedyn.io, and make a '
          'token under Token Management.',
    ),
  };

  @override
  void dispose() {
    _token.dispose();
    _name.dispose();
    _email.dispose();
    super.dispose();
  }

  NetworkInfo? get _network =>
      ref.read(networkProvider(widget.serverKey)).value;

  String get _suffix =>
      _provider == DnsProviderKind.duckdns ? '.duckdns.org' : '.dedyn.io';

  Future<void> _add() async {
    setState(() {
      _busy = true;
      _error = null;
    });
    final email = _email.text.trim();
    try {
      await ref
          .read(networkProvider(widget.serverKey).notifier)
          .change(
            (repository, server) => repository.addFreeSubdomain(
              server,
              provider: _provider,
              name: _name.text.trim(),
              token: _token.text.trim(),
              email: email.isEmpty ? null : email,
              agreeTerms: _agreed,
            ),
          );
      if (!mounted) return;
      showOcToast(
        context,
        'Pointing ${_name.text.trim().toLowerCase()}$_suffix here',
      );
      Navigator.pop(context);
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
    final colors = context.oc;
    final network = _network;
    final connected = network?.provider(_provider);
    final needsAgreement = !(network?.acme.exists ?? false);
    final (site, how) = _sites[_provider]!;
    final ready =
        _name.text.trim().isNotEmpty &&
        (connected != null || _token.text.trim().isNotEmpty) &&
        (!needsAgreement || _agreed);
    return OcDialog(
      title: 'Get a free domain',
      width: 520,
      actions: [
        OcButton(label: 'Cancel', onPressed: () => Navigator.pop(context)),
        OcButton.primary(
          label: 'Get domain',
          busy: _busy,
          onPressed: ready && !_busy ? _add : null,
        ),
      ],
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          const Text(
            'A free name for this server. Opencord points it here, keeps it '
            'pointed here when the address changes, and gets its '
            'certificate. The ports still have to be reachable.',
          ),
          const SizedBox(height: OcSpace.s16),
          ChoiceChips<DnsProviderKind>(
            options: const [
              (DnsProviderKind.duckdns, 'DuckDNS'),
              (DnsProviderKind.desec, 'deSEC'),
            ],
            value: _provider,
            onChanged: (provider) => setState(() {
              _provider = provider;
              _error = null;
            }),
          ),
          const SizedBox(height: OcSpace.s12),
          Text.rich(
            TextSpan(
              children: [
                TextSpan(text: '$how '),
                WidgetSpan(
                  alignment: PlaceholderAlignment.baseline,
                  baseline: TextBaseline.alphabetic,
                  child: GestureDetector(
                    onTap: () => launchUrl(
                      Uri.parse(site),
                      mode: LaunchMode.externalApplication,
                    ),
                    child: Text(
                      'Open ${Uri.parse(site).host}',
                      style: TextStyle(
                        color: colors.text,
                        decoration: TextDecoration.underline,
                      ),
                    ),
                  ),
                ),
              ],
            ),
            style: OcText.small.copyWith(color: colors.textSecondary),
          ),
          const SizedBox(height: OcSpace.s16),
          SectionLabel('${_provider.label} token'),
          const SizedBox(height: OcSpace.s8),
          OcTextField(
            controller: _token,
            mono: true,
            hint: connected == null
                ? 'Paste the token'
                : 'Connected (${connected.masked}): leave empty to keep it',
            semanticLabel: 'Token',
            onChanged: (_) => setState(() => _error = null),
          ),
          const SizedBox(height: OcSpace.s16),
          const SectionLabel('Name'),
          const SizedBox(height: OcSpace.s8),
          OcTextField(
            controller: _name,
            mono: true,
            hint: 'mychat',
            semanticLabel: 'Name',
            suffix: Padding(
              padding: const EdgeInsets.only(right: OcSpace.s12),
              child: Text(
                _suffix,
                style: OcText.mono.copyWith(color: colors.textMuted),
              ),
            ),
            onChanged: (_) => setState(() => _error = null),
          ),
          if (needsAgreement) ...[
            const SizedBox(height: OcSpace.s16),
            const SectionLabel('Certificate'),
            const SizedBox(height: OcSpace.s8),
            OcTextField(
              controller: _email,
              hint: 'Email for Let\'s Encrypt (optional)',
              semanticLabel: 'Email',
            ),
            const SizedBox(height: OcSpace.s12),
            Row(
              children: [
                Semantics(
                  label: 'I agree',
                  toggled: _agreed,
                  child: OcSwitch(
                    value: _agreed,
                    onChanged: (on) => setState(() => _agreed = on),
                  ),
                ),
                const SizedBox(width: OcSpace.s12),
                Expanded(
                  child: Text.rich(
                    TextSpan(
                      text: "I agree to Let's Encrypt's ",
                      children: [
                        WidgetSpan(
                          alignment: PlaceholderAlignment.baseline,
                          baseline: TextBaseline.alphabetic,
                          child: GestureDetector(
                            onTap: () => launchUrl(
                              Uri.parse(
                                network?.acme.termsUrl.isNotEmpty ?? false
                                    ? network!.acme.termsUrl
                                    : 'https://letsencrypt.org/repository/',
                              ),
                              mode: LaunchMode.externalApplication,
                            ),
                            child: Text(
                              'subscriber agreement',
                              style: TextStyle(
                                color: colors.text,
                                decoration: TextDecoration.underline,
                              ),
                            ),
                          ),
                        ),
                        const TextSpan(text: '.'),
                      ],
                    ),
                  ),
                ),
              ],
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
