import 'package:clock/clock.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/network.dart';
import 'package:opencord/features/settings/server/network/network_dialogs.dart';
import 'package:opencord/features/settings/settings_dialog.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/settings.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// Detecting the address, and the addresses set by hand.
typedef _Choice = ({bool autoDetect, String ipv4, String ipv6});

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

  Future<void> _save(_Choice choice) => saveNetwork(
    ref,
    widget.serverKey,
    (repository, server) => repository.setPublicAddress(
      server,
      autoDetect: choice.autoDetect,
      overrideIpv4: choice.ipv4,
      overrideIpv6: choice.ipv6,
    ),
  );

  Future<void> _setByHand(_Choice choice, ValueChanged<_Choice> change) async {
    final chosen = await showOcDialog<({String ipv4, String ipv6})>(
      context: context,
      builder: (_) => AddressDialog(
        address: PublicAddress(
          ipv4: widget.address.ipv4,
          ipv6: widget.address.ipv6,
          overrideIpv4: choice.ipv4,
          overrideIpv6: choice.ipv6,
        ),
      ),
    );
    if (chosen == null || !mounted) return;
    change((
      autoDetect: choice.autoDetect,
      ipv4: chosen.ipv4,
      ipv6: chosen.ipv6,
    ));
  }

  @override
  Widget build(BuildContext context) {
    final address = widget.address;
    // Detecting automatically and the addresses set by hand wait for the
    // save bar.
    return PendingValue<_Choice>(
      saved: (
        autoDetect: address.autoDetect,
        ipv4: address.overrideIpv4,
        ipv6: address.overrideIpv6,
      ),
      save: _save,
      builder: (context, choice, change) => _section(choice, change),
    );
  }

  Widget _section(_Choice choice, ValueChanged<_Choice> change) {
    final address = widget.address;
    final showV6 = address.hasIpv6 || address.ipv6.isNotEmpty;
    final overridden = choice.ipv4.isNotEmpty || choice.ipv6.isNotEmpty;
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
          value: choice.autoDetect,
          onChanged: (on) =>
              change((autoDetect: on, ipv4: choice.ipv4, ipv6: choice.ipv6)),
        ),
        SettingsRow(
          title: 'Set by hand',
          subtitle: overridden
              ? 'Used instead of what is detected'
              : 'For a fixed address, or when detection is off',
          trailing: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              if (overridden) ...[
                OcButton.ghost(
                  label: 'Clear',
                  dense: true,
                  onPressed: () => change((
                    autoDetect: choice.autoDetect,
                    ipv4: '',
                    ipv6: '',
                  )),
                ),
                const SizedBox(width: 8),
              ],
              OcButton(
                label: 'Change',
                dense: true,
                onPressed: () => _setByHand(choice, change),
              ),
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
