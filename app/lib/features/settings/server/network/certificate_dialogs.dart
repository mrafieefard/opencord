import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:url_launcher/url_launcher.dart';

import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/network.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/settings/server/network/network_dialogs.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/inline_error.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/oc_switch.dart';
import 'package:opencord/ui/widgets/oc_text_field.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// A domain's certificate as its row says it (Phase 3 plan §6.5, §7):
/// "Valid until 12 Jan · renews automatically", "No certificate",
/// "Renewal failed", "Provided by Cloudflare".
String certificateLabel(Domain domain, DateTime now) {
  final certificate = domain.certificate;
  final until = switch (certificate.notAfter) {
    final at? => shortDate(at, now),
    null => '',
  };
  final test = certificate.test ? 'Test certificate, ' : '';
  return switch (certificate.state) {
    CertificateState.provided => switch (domain.kind) {
      DomainKind.tunnel => 'Certificate provided by Cloudflare',
      DomainKind.cloudflareProxy =>
        "Apps see Cloudflare's certificate; no origin certificate uploaded",
      _ => 'Certificate provided by the proxy',
    },
    CertificateState.requesting => 'Getting a certificate',
    CertificateState.valid
        when certificate.uploaded &&
            domain.kind == DomainKind.cloudflareProxy =>
      'Origin certificate, valid until $until',
    CertificateState.valid when certificate.uploaded =>
      'Uploaded certificate, valid until $until',
    CertificateState.valid when certificate.test =>
      '${test}valid until $until (not used)',
    CertificateState.valid => 'Valid until $until · renews automatically',
    CertificateState.renewalFailed => 'Renewal failed, valid until $until',
    CertificateState.expired => 'Certificate expired $until',
    CertificateState.failed || CertificateState.none => 'No certificate',
  };
}

/// What the row shows under it while requesting or after a failure.
String? certificateProgress(CertificateStatus certificate, DateTime now) {
  if (certificate.step case final step?) {
    final number = step.index + 1;
    return 'Certificate: ${step.label}… ($number of '
        '${CertificateStep.values.length})';
  }
  if (certificate.error.isEmpty) return null;
  final retry = switch (certificate.retryAt) {
    final at? when at.isAfter(now) => ' Try again after ${clockTime(at)}.',
    _ => '',
  };
  return '${certificate.error}$retry';
}

/// Whether a certificate can be asked for now: a verified domain that
/// needs one, not already being asked for, and not waiting out failures.
bool canRequestCertificate(Domain domain, DateTime now) =>
    !domain.kind.behindProxy &&
    domain.status == DomainStatus.verified &&
    domain.certificate.state != CertificateState.requesting &&
    !(domain.certificate.retryAt?.isAfter(now) ?? false);

/// Gets a certificate for [domain]: the first time, after asking for an
/// email and agreement to Let's Encrypt's terms (plan §7.1).
Future<void> getCertificate(
  BuildContext context,
  WidgetRef ref,
  String serverKey,
  Domain domain,
) async {
  final network = ref.read(networkProvider(serverKey)).value;
  final acme = network?.acme ?? const AcmeAccount();
  if (!acme.exists) {
    await showOcDialog<void>(
      context: context,
      builder: (_) => GetCertificateDialog(
        serverKey: serverKey,
        domain: domain,
        acme: acme,
        staging: network?.settings.acmeStaging ?? false,
      ),
    );
    return;
  }
  await changeNetwork(
    context,
    ref,
    serverKey,
    (repository, server) => repository.requestCertificate(server, domain.id),
  );
}

/// The first certificate: an optional email and the subscriber agreement.
class GetCertificateDialog extends ConsumerStatefulWidget {
  const GetCertificateDialog({
    super.key,
    required this.serverKey,
    required this.domain,
    required this.acme,
    this.staging = false,
  });

  final String serverKey;
  final Domain domain;
  final AcmeAccount acme;

  /// Let's Encrypt's test environment is in use.
  final bool staging;

  @override
  ConsumerState<GetCertificateDialog> createState() =>
      _GetCertificateDialogState();
}

class _GetCertificateDialogState extends ConsumerState<GetCertificateDialog> {
  late final _email = TextEditingController(text: widget.acme.email);
  var _agreed = false;
  var _busy = false;
  String? _error;

  @override
  void dispose() {
    _email.dispose();
    super.dispose();
  }

  Future<void> _request() async {
    final email = _email.text.trim();
    if (email.isNotEmpty && (!email.contains('@') || email.contains(' '))) {
      setState(() => _error = 'That is not an email address.');
      return;
    }
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      await ref
          .read(networkProvider(widget.serverKey).notifier)
          .change(
            (repository, server) => repository.requestCertificate(
              server,
              widget.domain.id,
              email: email.isEmpty ? null : email,
              agreeTerms: _agreed,
            ),
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
    final colors = context.oc;
    final terms = widget.acme.termsUrl;
    return OcDialog(
      title: 'Get a certificate for ${widget.domain.hostname}',
      width: 480,
      actions: [
        OcButton(label: 'Cancel', onPressed: () => Navigator.pop(context)),
        OcButton.primary(
          label: 'Get certificate',
          busy: _busy,
          onPressed: _agreed && !_busy ? _request : null,
        ),
      ],
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(
            widget.staging
                ? "The certificate comes from Let's Encrypt's test "
                      'environment: people will not trust it. Turn the test '
                      'environment off in Advanced for a real one.'
                : "Let's Encrypt gives the certificate for free, and this "
                      'server renews it by itself.',
          ),
          const SizedBox(height: OcSpace.s16),
          OcTextField(
            controller: _email,
            autofocus: true,
            hint: 'Email (optional)',
            semanticLabel: 'Email',
            onChanged: (_) {
              if (_error != null) setState(() => _error = null);
            },
          ),
          const SizedBox(height: OcSpace.s4),
          Text(
            "Only for recovering the account and Let's Encrypt's important "
            'notices. It can be changed later.',
            style: OcText.small.copyWith(color: colors.textSecondary),
          ),
          const SizedBox(height: OcSpace.s16),
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
                          onTap: terms.isEmpty
                              ? null
                              : () => launchUrl(
                                  Uri.parse(terms),
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
          if (_error case final error?) ...[
            const SizedBox(height: OcSpace.s12),
            InlineError(error),
          ],
        ],
      ),
    );
  }
}

const _pemTypes = XTypeGroup(
  label: 'PEM',
  extensions: ['pem', 'crt', 'cer', 'key'],
);

/// The owner's own certificate (plan §7.4): its chain and key in PEM,
/// pasted or read from files.
class UploadCertificateDialog extends ConsumerStatefulWidget {
  const UploadCertificateDialog({
    super.key,
    required this.serverKey,
    required this.domain,
  });

  final String serverKey;
  final Domain domain;

  @override
  ConsumerState<UploadCertificateDialog> createState() =>
      _UploadCertificateDialogState();
}

class _UploadCertificateDialogState
    extends ConsumerState<UploadCertificateDialog> {
  final _chain = TextEditingController();
  final _key = TextEditingController();
  var _busy = false;
  String? _error;

  /// PEM files are small; anything bigger is not one.
  static const _maxFile = 64 * 1024;

  @override
  void dispose() {
    _chain.dispose();
    _key.dispose();
    super.dispose();
  }

  Future<void> _load(TextEditingController into) async {
    final file = await openFile(acceptedTypeGroups: const [_pemTypes]);
    if (file == null) return;
    if (await file.length() > _maxFile) {
      setState(() => _error = 'That file is too big to be a PEM file.');
      return;
    }
    into.text = await file.readAsString();
    setState(() => _error = null);
  }

  Future<void> _upload() async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      await ref
          .read(networkProvider(widget.serverKey).notifier)
          .change(
            (repository, server) => repository.uploadCertificate(
              server,
              widget.domain.id,
              chainPem: _chain.text.trim(),
              keyPem: _key.text.trim(),
            ),
          );
      if (!mounted) return;
      showOcToast(context, 'Certificate uploaded');
      Navigator.pop(context);
    } on RepoException catch (error) {
      if (!mounted) return;
      setState(() {
        _busy = false;
        _error = error.message;
      });
    }
  }

  Widget _field(String label, String hint, TextEditingController controller) =>
      Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          Row(
            children: [
              Expanded(child: Text(label, style: OcText.bodyStrong)),
              OcButton(
                label: 'Choose file',
                dense: true,
                onPressed: () => _load(controller),
              ),
            ],
          ),
          const SizedBox(height: OcSpace.s8),
          OcTextField(
            controller: controller,
            mono: true,
            hint: hint,
            semanticLabel: label,
            minLines: 4,
            maxLines: 4,
            onChanged: (_) => setState(() => _error = null),
          ),
        ],
      );

  @override
  Widget build(BuildContext context) {
    final ready = _chain.text.trim().isNotEmpty && _key.text.trim().isNotEmpty;
    return OcDialog(
      title: 'Upload a certificate for ${widget.domain.hostname}',
      width: 560,
      actions: [
        OcButton(label: 'Cancel', onPressed: () => Navigator.pop(context)),
        OcButton.primary(
          label: 'Upload',
          busy: _busy,
          onPressed: ready && !_busy ? _upload : null,
        ),
      ],
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          const Text(
            'For a certificate from your own authority, or a wildcard one. '
            'It is not renewed here: you are told 14 days before it '
            'expires.',
          ),
          const SizedBox(height: OcSpace.s16),
          _field('Certificate chain', '-----BEGIN CERTIFICATE-----', _chain),
          const SizedBox(height: OcSpace.s16),
          _field('Private key', '-----BEGIN PRIVATE KEY-----', _key),
          if (_error case final error?) ...[
            const SizedBox(height: OcSpace.s12),
            InlineError(error),
          ],
        ],
      ),
    );
  }
}
