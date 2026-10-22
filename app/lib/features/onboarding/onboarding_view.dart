import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/window/header_bar.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/inline_error.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_text_field.dart';
import 'package:opencord/ui/widgets/toast.dart';

enum _Step { choose, create, restore }

/// First launch (Phase 1 §9.1): make an identity, or bring one from a
/// backup. Once there is one, the app opens on its own.
class OnboardingView extends ConsumerStatefulWidget {
  const OnboardingView({super.key});

  @override
  ConsumerState<OnboardingView> createState() => _OnboardingViewState();
}

class _OnboardingViewState extends ConsumerState<OnboardingView> {
  var _step = _Step.choose;
  NewIdentity? _draft;
  final _name = TextEditingController();
  final _backup = TextEditingController();
  var _busy = false;
  String? _error;

  OpencordRepository get _repository => ref.read(repositoryProvider);

  @override
  void dispose() {
    _name.dispose();
    _backup.dispose();
    super.dispose();
  }

  Future<void> _create() async {
    setState(() => _busy = true);
    try {
      final draft = await _repository.generateIdentity();
      if (!mounted) return;
      setState(() {
        _draft = draft;
        _step = _Step.create;
      });
    } on RepoException catch (error) {
      if (mounted) setState(() => _error = error.message);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _finish() async {
    final backup = _step == _Step.create ? _draft!.backup : _backup.text;
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      await _repository.adoptIdentity(backup, displayName: _name.text);
    } on RepoException catch (error) {
      if (mounted) setState(() => _error = error.message);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  void _back() => setState(() {
    _step = _Step.choose;
    _error = null;
  });

  bool get _ready =>
      _name.text.trim().isNotEmpty &&
      (_step == _Step.create || _backup.text.trim().isNotEmpty);

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Material(
      color: colors.chat,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          const HeaderBar(
            leadingControls: true,
            trailingControls: true,
            child: SizedBox(),
          ),
          Expanded(
            child: Center(
              child: SingleChildScrollView(
                padding: const EdgeInsets.all(OcSpace.s24),
                child: ConstrainedBox(
                  constraints: const BoxConstraints(maxWidth: 440),
                  child: switch (_step) {
                    _Step.choose => _choose(colors),
                    _Step.create => _form(colors, creating: true),
                    _Step.restore => _form(colors, creating: false),
                  },
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }

  Widget _choose(OcColors colors) => Column(
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      Icon(OcIcons.forum, size: 48, color: colors.text),
      const SizedBox(height: OcSpace.s16),
      Text(
        'Welcome to Opencord',
        textAlign: TextAlign.center,
        style: OcText.title.copyWith(color: colors.text),
      ),
      const SizedBox(height: OcSpace.s8),
      Text(
        'Chat on servers you or your friends run. You are a key on this '
        'device: no account, no password.',
        textAlign: TextAlign.center,
        style: OcText.body.copyWith(color: colors.textSecondary),
      ),
      if (_error case final error?) ...[
        const SizedBox(height: OcSpace.s16),
        InlineError(error),
      ],
      const SizedBox(height: OcSpace.s24),
      OcButton.primary(
        label: 'Create a new identity',
        expand: true,
        busy: _busy,
        onPressed: _busy ? null : _create,
      ),
      const SizedBox(height: OcSpace.s8),
      OcButton(
        label: 'I have a backup',
        expand: true,
        onPressed: _busy
            ? null
            : () => setState(() {
                _step = _Step.restore;
                _error = null;
              }),
      ),
    ],
  );

  Widget _form(OcColors colors, {required bool creating}) {
    Widget label(String text) => Padding(
      padding: const EdgeInsets.only(bottom: OcSpace.s8),
      child: Text(text, style: OcText.label.copyWith(color: colors.textMuted)),
    );
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text(
          creating ? 'Your identity' : 'Bring your identity',
          style: OcText.title.copyWith(color: colors.text),
        ),
        const SizedBox(height: OcSpace.s6),
        Text(
          creating
              ? 'This key is you on every server. Show its fingerprint when '
                    'someone asks who you are.'
              : 'Paste the backup you saved, then choose your name.',
          style: OcText.body.copyWith(color: colors.textSecondary),
        ),
        const SizedBox(height: OcSpace.s20),
        if (creating) ...[
          label('FINGERPRINT'),
          Container(
            padding: const EdgeInsets.all(OcSpace.s12),
            decoration: BoxDecoration(
              color: colors.hover,
              borderRadius: BorderRadius.circular(OcRadius.input),
            ),
            child: SelectableText(
              _draft!.fingerprint.replaceAll('-', ' '),
              style: OcText.mono.copyWith(fontSize: 16, color: colors.text),
            ),
          ),
          const SizedBox(height: OcSpace.s16),
        ] else ...[
          label('IDENTITY BACKUP'),
          OcTextField(
            controller: _backup,
            semanticLabel: 'Identity backup',
            hint: 'opencord-identity-…',
            mono: true,
            minLines: 2,
            maxLines: 3,
            onChanged: (_) => setState(() {}),
          ),
          const SizedBox(height: OcSpace.s16),
        ],
        label('DISPLAY NAME'),
        OcTextField(
          controller: _name,
          semanticLabel: 'Display name',
          hint: 'What should people call you?',
          autofocus: creating,
          maxLength: 32,
          onChanged: (_) => setState(() {}),
          onSubmitted: (_) {
            if (_ready && !_busy) _finish();
          },
        ),
        if (creating) ...[
          const SizedBox(height: OcSpace.s20),
          label('BACKUP'),
          Text(
            'Anyone with the backup can be you. Keep it somewhere private, '
            'like a password; without it this identity cannot be recovered.',
            style: OcText.small.copyWith(color: colors.textSecondary),
          ),
          const SizedBox(height: OcSpace.s8),
          Align(
            alignment: Alignment.centerLeft,
            child: OcButton(
              label: 'Copy backup',
              icon: OcIcons.contentCopy,
              dense: true,
              onPressed: () {
                Clipboard.setData(ClipboardData(text: _draft!.backup));
                showOcToast(context, 'Backup copied');
              },
            ),
          ),
        ],
        if (_error case final error?) ...[
          const SizedBox(height: OcSpace.s16),
          InlineError(error),
        ],
        const SizedBox(height: OcSpace.s24),
        Row(
          children: [
            OcButton(label: 'Back', onPressed: _busy ? null : _back),
            const Spacer(),
            OcButton.primary(
              label: 'Continue',
              busy: _busy,
              onPressed: _ready && !_busy ? _finish : null,
            ),
          ],
        ),
      ],
    );
  }
}
