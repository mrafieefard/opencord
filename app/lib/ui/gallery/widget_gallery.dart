import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'package:opencord/core/model/channel_kind.dart';
import 'package:opencord/core/model/presence.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/theme/oc_theme.dart';
import 'package:opencord/ui/widgets/avatar.dart';
import 'package:opencord/ui/widgets/badges.dart';
import 'package:opencord/ui/widgets/channel_glyph.dart';
import 'package:opencord/ui/widgets/inline_error.dart';
import 'package:opencord/ui/widgets/key_hint.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
import 'package:opencord/ui/widgets/oc_spinner.dart';
import 'package:opencord/ui/widgets/oc_switch.dart';
import 'package:opencord/ui/widgets/oc_text_field.dart';
import 'package:opencord/ui/widgets/popover.dart';
import 'package:opencord/ui/widgets/section_label.dart';
import 'package:opencord/ui/widgets/settings.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// Every shared widget in both themes, side by side (desktop UI plan §12,
/// step 2). Reached from the debug menu.
class WidgetGallery extends StatelessWidget {
  const WidgetGallery({super.key});

  @override
  Widget build(BuildContext context) {
    return Row(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        for (final colors in const [OcColors.dark, OcColors.light])
          Expanded(
            child: Theme(
              data: buildTheme(colors),
              child: const GalleryContent(),
            ),
          ),
      ],
    );
  }
}

/// One theme's column of the gallery.
class GalleryContent extends StatelessWidget {
  const GalleryContent({super.key, this.scrollable = true});

  /// Off in tests that capture the whole column at once.
  final bool scrollable;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final sections = [
      _Section('Typography', [const _Typography()]),
      _Section('Tokens', [const _Swatches()]),
      _Section('Buttons', [const _Buttons()]),
      _Section('Icon buttons', [const _IconButtons()]),
      _Section('Inputs', [const _Inputs()]),
      _Section('Avatars and presence', [const _Avatars()]),
      _Section('Badges', [const _Badges()]),
      _Section('Channel glyphs', [const _Glyphs()]),
      _Section('Switches and key hints', [const _Switches()]),
      _Section('Settings', [const _Settings()]),
      _Section('Overlays', [const _Overlays()]),
    ];
    final content = Padding(
      padding: const EdgeInsets.all(OcSpace.s24),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          for (final section in sections) ...[
            SectionLabel(section.title),
            const SizedBox(height: OcSpace.s10),
            ...section.children,
            const SizedBox(height: OcSpace.s24),
          ],
        ],
      ),
    );
    return Material(
      color: colors.chat,
      child: scrollable ? SingleChildScrollView(child: content) : content,
    );
  }
}

class _Section {
  const _Section(this.title, this.children);

  final String title;
  final List<Widget> children;
}

Widget _wrap(List<Widget> children) => Wrap(
  spacing: OcSpace.s12,
  runSpacing: OcSpace.s12,
  crossAxisAlignment: WrapCrossAlignment.center,
  children: children,
);

class _Typography extends StatelessWidget {
  const _Typography();

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          'Title — 17 / 600',
          style: OcText.title.copyWith(color: colors.text),
        ),
        Text(
          'Header — 15 / 600',
          style: OcText.header.copyWith(color: colors.text),
        ),
        Text(
          'Body — 14 / 400 for messages and rows',
          style: OcText.body.copyWith(color: colors.text),
        ),
        Text(
          'Body strong — 14 / 600',
          style: OcText.bodyStrong.copyWith(color: colors.text),
        ),
        Text(
          'Small — previews and subtitles',
          style: OcText.small.copyWith(color: colors.textSecondary),
        ),
        Text(
          'Meta — 12:04 · edited',
          style: OcText.meta.copyWith(color: colors.textMuted),
        ),
        Text(
          'LABEL — CATEGORY',
          style: OcText.label.copyWith(color: colors.textMuted),
        ),
        Text(
          'mono — ABCD-EFGH-IJKL-MNOP',
          style: OcText.mono.copyWith(color: colors.text),
        ),
      ],
    );
  }
}

class _Swatches extends StatelessWidget {
  const _Swatches();

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return _wrap([
      for (final MapEntry(:key, :value) in colors.all.entries)
        Column(
          children: [
            Container(
              width: 44,
              height: 28,
              decoration: BoxDecoration(
                color: value,
                borderRadius: BorderRadius.circular(6),
                border: Border.all(color: colors.border),
              ),
            ),
            const SizedBox(height: 2),
            Text(key, style: OcText.meta.copyWith(color: colors.textMuted)),
          ],
        ),
    ]);
  }
}

class _Buttons extends StatelessWidget {
  const _Buttons();

  @override
  Widget build(BuildContext context) {
    void noop() {}
    return _wrap([
      OcButton.primary(label: 'Trust and connect', onPressed: noop),
      OcButton(label: 'Cancel', onPressed: noop),
      OcButton.ghost(label: 'Ghost', onPressed: noop),
      OcButton.primary(label: 'Dense', dense: true, onPressed: noop),
      OcButton(label: 'Dense', dense: true, onPressed: noop),
      OcButton(label: 'With icon', icon: OcIcons.personAdd, onPressed: noop),
      OcButton.primary(label: 'Connecting', busy: true, onPressed: noop),
      const OcButton.primary(label: 'Disabled'),
    ]);
  }
}

class _IconButtons extends StatelessWidget {
  const _IconButtons();

  @override
  Widget build(BuildContext context) {
    void noop() {}
    return _wrap([
      OcIconButton(icon: OcIcons.search, tooltip: 'Search', onPressed: noop),
      OcIconButton(
        icon: OcIcons.mic,
        activeIcon: OcIcons.micOff,
        tooltip: 'Unmute',
        active: true,
        onPressed: noop,
      ),
      OcIconButton(
        icon: OcIcons.group,
        tooltip: 'Member list',
        active: true,
        activeStyle: OcActiveStyle.inverted,
        onPressed: noop,
      ),
      OcIconButton(
        icon: OcIcons.settings,
        tooltip: 'Settings',
        size: OcIconButtonSize.compact,
        onPressed: noop,
      ),
      OcIconButton(
        icon: OcIcons.videocam,
        tooltip: 'Camera',
        size: OcIconButtonSize.voice,
        active: true,
        activeStyle: OcActiveStyle.inverted,
        onPressed: noop,
      ),
      const OcIconButton(icon: OcIcons.pushPin, tooltip: 'Disabled'),
    ]);
  }
}

class _Inputs extends StatelessWidget {
  const _Inputs();

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        const OcTextField(hint: 'Invite link or host:port'),
        const SizedBox(height: OcSpace.s10),
        OcTextField(
          hint: 'Search',
          prefixIcon: OcIcons.search,
          radius: OcRadius.searchPill,
          readOnly: true,
          suffix: Padding(
            padding: const EdgeInsets.only(right: 8),
            child: KeyHint(
              appShortcut(LogicalKeyboardKey.keyK, Theme.of(context).platform),
            ),
          ),
        ),
        const SizedBox(height: OcSpace.s10),
        const OcTextField(
          hint: 'Message #general',
          radius: OcRadius.composer,
          maxLines: null,
        ),
        const SizedBox(height: OcSpace.s10),
        OcTextField(
          controller: TextEditingController(text: 'ABCD-EFGH-IJKL-MNOP'),
          mono: true,
          readOnly: true,
        ),
        const SizedBox(height: OcSpace.s10),
        const InlineError('Could not connect: the server did not answer.'),
      ],
    );
  }
}

class _Avatars extends StatelessWidget {
  const _Avatars();

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return _wrap([
      for (final presence in Presence.values)
        OcAvatar(
          id: presence.index,
          name: 'Kai ${presence.label}',
          size: OcSize.memberAvatar,
          presence: presence,
          ringColor: colors.chat,
        ),
      const OcAvatar(id: 'mira', name: 'Mira', speaking: true),
      const OcAvatar(id: 'voice', name: 'Jo', size: OcSize.voiceAvatar),
      OcAvatar(
        id: 'profile',
        name: 'Lena Fischer',
        size: 64,
        presence: Presence.idle,
        ringColor: colors.chat,
      ),
      const OcAvatar(
        id: 'server-1',
        name: 'Opencord Dev',
        size: OcSize.serverIcon,
        borderRadius: OcRadius.serverIcon,
      ),
      const OcAvatar(
        id: 'server-2',
        name: 'Rust Berlin',
        size: OcSize.serverIcon,
        borderRadius: OcRadius.serverIconActive,
        inverted: true,
      ),
    ]);
  }
}

class _Badges extends StatelessWidget {
  const _Badges();

  @override
  Widget build(BuildContext context) {
    return _wrap(const [
      UnreadBadge(count: 3),
      UnreadBadge(count: 128),
      UnreadBadge(count: 1250),
      UnreadBadge(count: 12, muted: true),
      MentionBadge(),
      LivePill(),
      ConnectionDot(connected: true),
      ConnectionDot(connected: false),
      OcSpinner(),
    ]);
  }
}

class _Glyphs extends StatelessWidget {
  const _Glyphs();

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return _wrap([
      const ChannelGlyph(kind: ChannelKind.text),
      const ChannelGlyph(kind: ChannelKind.text, selected: true),
      ChannelGlyph(
        kind: ChannelKind.announcement,
        locked: true,
        ringColor: colors.chat,
      ),
      const ChannelGlyph(kind: ChannelKind.voice),
      ChannelGlyph(
        kind: ChannelKind.text,
        size: 64,
        locked: true,
        ringColor: colors.chat,
      ),
    ]);
  }
}

class _Switches extends StatefulWidget {
  const _Switches();

  @override
  State<_Switches> createState() => _SwitchesState();
}

class _SwitchesState extends State<_Switches> {
  bool _on = true;

  @override
  Widget build(BuildContext context) {
    final platform = Theme.of(context).platform;
    return _wrap([
      OcSwitch(value: _on, onChanged: (value) => setState(() => _on = value)),
      OcSwitch(value: !_on, onChanged: (value) => setState(() => _on = !value)),
      const OcSwitch(value: true, onChanged: null),
      KeyHint(appShortcut(LogicalKeyboardKey.keyK, platform)),
      KeyHint(appShortcut(LogicalKeyboardKey.keyM, platform, shift: true)),
      const KeyHint(SingleActivator(LogicalKeyboardKey.arrowUp, alt: true)),
    ]);
  }
}

class _Settings extends StatefulWidget {
  const _Settings();

  @override
  State<_Settings> createState() => _SettingsState();
}

class _SettingsState extends State<_Settings> {
  String _theme = 'system';
  bool _reduceMotion = false;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        SettingsSection(
          title: 'Appearance',
          footer: 'Changes apply right away.',
          children: [
            SettingsSwitchRow(
              title: 'Reduce motion',
              subtitle: 'Turns off non-essential animations',
              value: _reduceMotion,
              onChanged: (value) => setState(() => _reduceMotion = value),
            ),
            SettingsRow(
              title: 'Identity key',
              subtitle: 'ABCD-EFGH-IJKL-MNOP',
              mono: true,
              icon: OcIcons.key,
              onTap: () {},
            ),
            const SettingsRow(title: 'Version', trailing: Text('0.1.0')),
          ],
        ),
        const SizedBox(height: OcSpace.s16),
        SettingsChoiceCards<String>(
          value: _theme,
          onChanged: (value) => setState(() => _theme = value),
          options: const [
            ChoiceCardOption(
              value: 'system',
              label: 'System',
              icon: OcIcons.contrast,
            ),
            ChoiceCardOption(
              value: 'dark',
              label: 'Dark',
              icon: OcIcons.darkMode,
            ),
            ChoiceCardOption(
              value: 'light',
              label: 'Light',
              icon: OcIcons.lightMode,
            ),
          ],
        ),
      ],
    );
  }
}

class _Overlays extends StatelessWidget {
  const _Overlays();

  @override
  Widget build(BuildContext context) {
    return Builder(
      builder: (context) => _wrap([
        OcButton(
          label: 'Dialog',
          onPressed: () => showOcDialog<void>(
            context: context,
            builder: (context) => OcDialog(
              title: 'Delete channel',
              actions: [
                OcButton(
                  label: 'Cancel',
                  onPressed: () => Navigator.pop(context),
                ),
                OcButton.primary(
                  label: 'Delete channel',
                  onPressed: () => Navigator.pop(context),
                ),
              ],
              child: const Text(
                'This deletes #general and its messages for everyone.',
              ),
            ),
          ),
        ),
        Builder(
          builder: (context) => OcButton(
            label: 'Menu',
            onPressed: () => showOcMenu(
              context: context,
              position: globalRectOf(context).bottomLeft,
              entries: [
                OcMenuItem(
                  label: 'Reply',
                  icon: OcIcons.reply,
                  onSelected: () {},
                ),
                OcMenuItem(
                  label: 'Copy text',
                  icon: OcIcons.contentCopy,
                  shortcut: const SingleActivator(
                    LogicalKeyboardKey.keyC,
                    control: true,
                  ),
                  onSelected: () {},
                ),
                OcMenuItem(
                  label: 'Roles',
                  icon: OcIcons.shieldPerson,
                  submenu: [
                    OcMenuItem(
                      label: 'Maintainer',
                      checked: true,
                      onSelected: () {},
                    ),
                    OcMenuItem(
                      label: 'Contributor',
                      checked: false,
                      onSelected: () {},
                    ),
                  ],
                ),
                const OcMenuDivider(),
                OcMenuItem(
                  label: 'Delete',
                  icon: OcIcons.delete,
                  onSelected: () {},
                ),
              ],
            ),
          ),
        ),
        Builder(
          builder: (context) => OcButton(
            label: 'Popover',
            onPressed: () => showPopover<void>(
              context: context,
              anchor: globalRectOf(context),
              builder: (context) => const SizedBox(
                width: 220,
                child: Text(
                  'An anchored popover, closed by Escape or a click outside.',
                ),
              ),
            ),
          ),
        ),
        OcButton(
          label: 'Toast',
          onPressed: () => showOcToast(context, 'Invite link copied'),
        ),
        OcButton(
          label: 'Undo toast',
          onPressed: () => showOcToast(
            context,
            'Reaction removed',
            actionLabel: 'Undo',
            onAction: () {},
          ),
        ),
      ]),
    );
  }
}
