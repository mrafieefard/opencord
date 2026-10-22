import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/lavalink.dart';
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

/// What was typed as the address: a host, or a pasted URL like
/// `https://lava.example.com:443`, which also gives the port and TLS.
({String host, int? port, bool? secure}) parseNodeAddress(String text) {
  final trimmed = text.trim();
  final uri = trimmed.contains('://') ? Uri.tryParse(trimmed) : null;
  if (uri == null || uri.host.isEmpty) {
    return (host: trimmed.toLowerCase(), port: null, secure: null);
  }
  final secure = uri.scheme == 'https' || uri.scheme == 'wss';
  return (
    host: uri.host.toLowerCase(),
    port: uri.hasPort ? uri.port : null,
    secure: secure,
  );
}

/// Why the address cannot be a node's host, or null.
String? nodeHostProblem(String host) {
  if (host.isEmpty) return null;
  final valid = RegExp(r'^[a-z0-9.\-_:\[\]]+$').hasMatch(host);
  return valid && !host.startsWith('.') && !host.startsWith('-')
      ? null
      : 'Enter a name like lavalink.example.com or an IP address.';
}

/// Adds a Lavalink node, or edits one (Phase 3 plan §12.1). The password
/// is never shown again: editing leaves it as it is unless replaced.
class LavalinkNodeDialog extends ConsumerStatefulWidget {
  const LavalinkNodeDialog({super.key, required this.serverKey, this.node});

  final String serverKey;

  /// The node edited; null adds one.
  final LavalinkNode? node;

  @override
  ConsumerState<LavalinkNodeDialog> createState() => _NodeDialogState();
}

class _NodeDialogState extends ConsumerState<LavalinkNodeDialog> {
  late final LavalinkNodeSettings? _old = widget.node?.settings;
  late final _name = TextEditingController(text: _old?.name ?? '');
  late final _host = TextEditingController(text: _old?.host ?? '');
  late final _port = TextEditingController(text: '${_old?.port ?? 2333}');
  final _password = TextEditingController();
  late final _maxPlayers = TextEditingController(
    text: switch (_old?.maxPlayers) {
      null || 0 => '',
      final max => '$max',
    },
  );
  late var _secure = _old?.secure ?? false;
  late var _priority = _old?.priority ?? LavalinkPriority.normal;
  late var _strict = _old?.strictAddress ?? true;
  var _busy = false;
  String? _error;

  bool get _editing => widget.node != null;

  @override
  void dispose() {
    _name.dispose();
    _host.dispose();
    _port.dispose();
    _password.dispose();
    _maxPlayers.dispose();
    super.dispose();
  }

  /// A pasted URL fills the port and TLS too.
  void _hostChanged(String text) {
    final parsed = parseNodeAddress(text);
    if (parsed.secure != null) {
      _host.text = parsed.host;
      _secure = parsed.secure!;
      _port.text = '${parsed.port ?? (_secure ? 443 : 80)}';
    }
    setState(() => _error = null);
  }

  int? get _portValue {
    final port = int.tryParse(_port.text.trim());
    return port == null || port < 1 || port > 65535 ? null : port;
  }

  String? get _problem {
    if (nodeHostProblem(_host.text.trim()) case final problem?) return problem;
    if (_port.text.trim().isNotEmpty && _portValue == null) {
      return 'A port is a number from 1 to 65535.';
    }
    return null;
  }

  bool get _ready =>
      _name.text.trim().isNotEmpty &&
      _host.text.trim().isNotEmpty &&
      _portValue != null &&
      _problem == null &&
      (_editing || _password.text.isNotEmpty);

  Future<void> _save() async {
    final settings = LavalinkNodeSettings(
      name: _name.text.trim(),
      host: _host.text.trim(),
      port: _portValue!,
      secure: _secure,
      priority: _priority,
      maxPlayers: int.tryParse(_maxPlayers.text.trim()) ?? 0,
      strictAddress: _strict,
      enabled: _old?.enabled ?? true,
    );
    setState(() {
      _busy = true;
      _error = null;
    });
    final password = _password.text;
    try {
      await ref
          .read(lavalinkProvider(widget.serverKey).notifier)
          .change(
            (repository, server) => switch (widget.node) {
              final node? => repository.updateLavalinkNode(
                server,
                node.id,
                LavalinkNodeChanges(
                  settings: settings,
                  password: password.isEmpty ? null : password,
                ),
              ),
              null => repository.createLavalinkNode(server, settings, password),
            },
          );
      if (!mounted) return;
      showOcToast(
        context,
        _editing ? '${settings.name} saved' : '${settings.name} added',
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

  Widget _switchRow(
    String title,
    String subtitle,
    bool value,
    ValueChanged<bool> onChanged,
  ) {
    final colors = context.oc;
    return Row(
      children: [
        Semantics(
          label: title,
          toggled: value,
          child: OcSwitch(value: value, onChanged: onChanged),
        ),
        const SizedBox(width: OcSpace.s12),
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                title,
                style: OcText.bodyStrong.copyWith(color: colors.text),
              ),
              Text(
                subtitle,
                style: OcText.small.copyWith(color: colors.textSecondary),
              ),
            ],
          ),
        ),
      ],
    );
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final muted = OcText.small.copyWith(color: colors.textSecondary);
    void changed(String _) => setState(() => _error = null);
    final problem = _problem;
    return OcDialog(
      title: _editing ? 'Edit ${_old!.name}' : 'Add a Lavalink node',
      width: 520,
      actions: [
        OcButton(label: 'Cancel', onPressed: () => Navigator.pop(context)),
        OcButton.primary(
          label: _editing ? 'Save' : 'Add node',
          busy: _busy,
          onPressed: _ready && !_busy ? _save : null,
        ),
      ],
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          if (!_editing) ...[
            Text(
              'Your own Lavalink or a public one, version 4 or newer. Opencord '
              'connects to it; people never do, and its password stays on the '
              'server.',
              style: muted,
            ),
            const SizedBox(height: OcSpace.s16),
          ],
          const SectionLabel('Name'),
          const SizedBox(height: OcSpace.s8),
          OcTextField(
            controller: _name,
            hint: 'Home server',
            semanticLabel: 'Name',
            maxLength: 64,
            autofocus: !_editing,
            onChanged: changed,
          ),
          const SizedBox(height: OcSpace.s12),
          const SectionLabel('Address'),
          const SizedBox(height: OcSpace.s8),
          Row(
            children: [
              Expanded(
                child: OcTextField(
                  controller: _host,
                  mono: true,
                  hint: 'lavalink.example.com',
                  semanticLabel: 'Host',
                  onChanged: _hostChanged,
                ),
              ),
              const SizedBox(width: OcSpace.s8),
              SizedBox(
                width: 96,
                child: OcTextField(
                  controller: _port,
                  mono: true,
                  hint: '2333',
                  semanticLabel: 'Port',
                  inputFormatters: [FilteringTextInputFormatter.digitsOnly],
                  onChanged: changed,
                ),
              ),
            ],
          ),
          const SizedBox(height: OcSpace.s12),
          _switchRow(
            'TLS',
            'The node is reached over https and wss',
            _secure,
            (on) => setState(() => _secure = on),
          ),
          const SizedBox(height: OcSpace.s16),
          const SectionLabel('Password'),
          const SizedBox(height: OcSpace.s8),
          OcTextField(
            controller: _password,
            obscure: true,
            hint: _editing
                ? 'Saved: leave empty to keep it'
                : 'Lavalink\'s server.password',
            semanticLabel: 'Password',
            onChanged: changed,
          ),
          const SizedBox(height: OcSpace.s16),
          const SectionLabel('Priority'),
          const SizedBox(height: OcSpace.s8),
          ChoiceChips<LavalinkPriority>(
            options: [
              for (final priority in LavalinkPriority.values)
                (priority, priority.label),
            ],
            value: _priority,
            onChanged: (priority) => setState(() => _priority = priority),
          ),
          const SizedBox(height: OcSpace.s4),
          Text(
            'Backup nodes play only when no Preferred or Normal node can.',
            style: muted,
          ),
          const SizedBox(height: OcSpace.s16),
          const SectionLabel('Max players'),
          const SizedBox(height: OcSpace.s8),
          OcTextField(
            controller: _maxPlayers,
            mono: true,
            hint: 'No limit',
            semanticLabel: 'Max players',
            inputFormatters: [FilteringTextInputFormatter.digitsOnly],
            onChanged: changed,
          ),
          const SizedBox(height: OcSpace.s4),
          Text('How many voice channels it plays for at once.', style: muted),
          const SizedBox(height: OcSpace.s16),
          _switchRow(
            'Strict address check',
            'Accept audio only from this node\'s addresses. Turn off for a '
                'public node whose address changes.',
            _strict,
            (on) => setState(() => _strict = on),
          ),
          if (_error ?? problem case final error?) ...[
            const SizedBox(height: OcSpace.s12),
            InlineError(error),
          ],
        ],
      ),
    );
  }
}
