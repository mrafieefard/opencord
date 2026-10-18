import 'package:clock/clock.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/network.dart';
import 'package:opencord/features/settings/server/network/network_dialogs.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/settings.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// Where the server is reached from the internet (Phase 3 plan §5.1, §8):
/// detected with ipify, or set by hand.
class PublicAddressSection extends ConsumerStatefulWidget {
  const PublicAddressSection({
    super.key,
    required this.serverKey,
    required this.address,
  });

  final String serverKey;
  final PublicAddress address;

  @override
  ConsumerState<PublicAddressSection> createState() =>
      _PublicAddressSectionState();
}

class _PublicAddressSectionState extends ConsumerState<PublicAddressSection> {
  var _detecting = false;

  Future<void> _detect() async {
    setState(() => _detecting = true);
    await changeNetwork(
      context,
      ref,
      widget.serverKey,
      (repository, server) => repository.detectPublicAddress(server),
    );
    if (mounted) setState(() => _detecting = false);
  }

  Future<void> _set({
    required bool autoDetect,
    String ipv4 = '',
    String ipv6 = '',
  }) => changeNetwork(
    context,
    ref,
    widget.serverKey,
    (repository, server) => repository.setPublicAddress(
      server,
      autoDetect: autoDetect,
      overrideIpv4: ipv4,
      overrideIpv6: ipv6,
    ),
  );

  Future<void> _setByHand() async {
    final address = widget.address;
    final chosen = await showOcDialog<({String ipv4, String ipv6})>(
      context: context,
      builder: (_) => AddressDialog(address: address),
    );
    if (chosen == null || !mounted) return;
    await _set(
      autoDetect: address.autoDetect,
      ipv4: chosen.ipv4,
      ipv6: chosen.ipv6,
    );
  }

  @override
  Widget build(BuildContext context) {
    final address = widget.address;
    final showV6 = address.hasIpv6 || address.ipv6.isNotEmpty;
    return SettingsSection(
      title: 'Public address',
      footer:
          'Domains you add must point to these addresses. When they change, '
          'domains are checked again and free subdomains follow.',
      children: [
        _AddressRow(
          label: 'IPv4',
          value: address.ipv4,
          source: _source(address, address.overrideIpv4),
        ),
        if (showV6)
          _AddressRow(
            label: 'IPv6',
            value: address.ipv6,
            source: _source(address, address.overrideIpv6),
          ),
        SettingsSwitchRow(
          title: 'Detect automatically',
          subtitle: address.detectError.isNotEmpty && address.autoDetect
              ? 'Last try failed: ${address.detectError}'
              : 'Asks api.ipify.org at start-up and every 6 hours',
          value: address.autoDetect,
          onChanged: (on) => _set(
            autoDetect: on,
            ipv4: address.overrideIpv4,
            ipv6: address.overrideIpv6,
          ),
        ),
        SettingsRow(
          title: 'Set by hand',
          subtitle: address.overridden
              ? 'Used instead of what is detected'
              : 'For a fixed address, or when detection is off',
          trailing: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              if (address.overridden) ...[
                OcButton.ghost(
                  label: 'Clear',
                  dense: true,
                  onPressed: () => _set(autoDetect: address.autoDetect),
                ),
                const SizedBox(width: 8),
              ],
              OcButton(label: 'Change', dense: true, onPressed: _setByHand),
            ],
          ),
        ),
        if (address.autoDetect)
          SettingsRow(
            title: 'Detect again',
            subtitle: 'Asks now instead of waiting',
            trailing: OcButton(
              label: 'Detect',
              icon: OcIcons.refresh,
              dense: true,
              busy: _detecting,
              onPressed: _detecting ? null : _detect,
            ),
          ),
      ],
    );
  }

  /// Where an address in use came from.
  String _source(PublicAddress address, String override) {
    if (override.isNotEmpty) return 'Set by hand';
    final at = address.detectedAt;
    if (at == null) {
      return address.autoDetect ? 'Not detected yet' : 'Detection is off';
    }
    return 'Detected ${rowTime(at, clock.now()) == clockTime(at) ? 'at' : 'on'} '
        '${rowTime(at, clock.now())}';
  }
}

class _AddressRow extends StatelessWidget {
  const _AddressRow({
    required this.label,
    required this.value,
    required this.source,
  });

  final String label;
  final String value;
  final String source;

  @override
  Widget build(BuildContext context) => SettingsRow(
    title: value.isEmpty ? '$label: unknown' : value,
    subtitle: '$label · $source',
    trailing: value.isEmpty
        ? null
        : OcButton.ghost(
            label: 'Copy',
            icon: OcIcons.contentCopy,
            dense: true,
            onPressed: () {
              Clipboard.setData(ClipboardData(text: value));
              showOcToast(context, '$label address copied');
            },
          ),
  );
}
