import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/channel_kind.dart';
import 'package:opencord/core/model/presence.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/widgets/avatar.dart';
import 'package:opencord/ui/widgets/badges.dart';
import 'package:opencord/ui/widgets/channel_glyph.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/settings.dart';

import '../support/pump.dart';

Widget _padded(Widget child) =>
    Padding(padding: const EdgeInsets.all(16), child: child);

void main() {
  for (final (name, colors) in themes) {
    testWidgets('presence shapes ($name)', (tester) async {
      await pumpThemed(
        tester,
        colors: colors,
        background: (c) => c.sidebar,
        surface: const Size(360, 100),
        _padded(
          Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              for (final presence in Presence.values)
                OcAvatar(
                  id: presence.name,
                  name: presence.label,
                  size: 34,
                  presence: presence,
                  ringColor: colors.sidebar,
                ),
              for (final presence in Presence.values)
                PresenceBadge(
                  presence: presence,
                  size: 16,
                  ringColor: colors.sidebar,
                ),
            ],
          ),
        ),
      );

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/presence_shapes_$name.png'),
      );
    });

    testWidgets('buttons, badges and glyphs ($name)', (tester) async {
      await pumpThemed(
        tester,
        colors: colors,
        surface: const Size(520, 170),
        _padded(
          Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  OcButton.primary(label: 'Join', onPressed: () {}),
                  const SizedBox(width: 8),
                  OcButton(label: 'Cancel', onPressed: () {}),
                  const SizedBox(width: 8),
                  OcButton.ghost(label: 'Ghost', onPressed: () {}),
                  const SizedBox(width: 8),
                  OcIconButton(
                    icon: OcIcons.mic,
                    activeIcon: OcIcons.micOff,
                    tooltip: 'Unmute',
                    active: true,
                    onPressed: () {},
                  ),
                  OcIconButton(
                    icon: OcIcons.videocam,
                    tooltip: 'Camera',
                    active: true,
                    activeStyle: OcActiveStyle.inverted,
                    onPressed: () {},
                  ),
                ],
              ),
              const SizedBox(height: 16),
              const Row(
                children: [
                  UnreadBadge(count: 3),
                  SizedBox(width: 8),
                  UnreadBadge(count: 1250),
                  SizedBox(width: 8),
                  UnreadBadge(count: 12, muted: true),
                  SizedBox(width: 8),
                  MentionBadge(),
                  SizedBox(width: 8),
                  LivePill(),
                  SizedBox(width: 8),
                  ConnectionDot(connected: true),
                  SizedBox(width: 8),
                  ConnectionDot(connected: false),
                ],
              ),
              const SizedBox(height: 16),
              Row(
                children: [
                  const ChannelGlyph(kind: ChannelKind.text),
                  const SizedBox(width: 12),
                  const ChannelGlyph(kind: ChannelKind.text, selected: true),
                  const SizedBox(width: 12),
                  ChannelGlyph(
                    kind: ChannelKind.announcement,
                    locked: true,
                    ringColor: colors.chat,
                  ),
                  const SizedBox(width: 12),
                  const ChannelGlyph(kind: ChannelKind.voice),
                ],
              ),
            ],
          ),
        ),
      );

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/controls_$name.png'),
      );
    });

    testWidgets('settings section ($name)', (tester) async {
      await pumpThemed(
        tester,
        colors: colors,
        background: (c) => c.elevated,
        surface: const Size(520, 330),
        _padded(
          Column(
            children: [
              SettingsSection(
                title: 'Appearance',
                footer: 'Changes apply right away.',
                children: [
                  SettingsSwitchRow(
                    title: 'Reduce motion',
                    subtitle: 'Turns off non-essential animations',
                    value: true,
                    onChanged: (_) {},
                  ),
                  SettingsRow(
                    title: 'Identity key',
                    subtitle: 'ABCD-EFGH-IJKL-MNOP',
                    mono: true,
                    icon: OcIcons.key,
                    onTap: () {},
                  ),
                ],
              ),
              const SizedBox(height: 16),
              SettingsChoiceCards<ThemeMode>(
                value: ThemeMode.dark,
                onChanged: (_) {},
                options: const [
                  ChoiceCardOption(
                    value: ThemeMode.system,
                    label: 'System',
                    icon: OcIcons.contrast,
                  ),
                  ChoiceCardOption(
                    value: ThemeMode.dark,
                    label: 'Dark',
                    icon: OcIcons.darkMode,
                  ),
                  ChoiceCardOption(
                    value: ThemeMode.light,
                    label: 'Light',
                    icon: OcIcons.lightMode,
                  ),
                ],
              ),
            ],
          ),
        ),
      );

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/settings_section_$name.png'),
      );
    });
  }

  test('OcColors stay importable from goldens', () {
    expect(OcColors.dark, isNot(OcColors.light));
  });
}
