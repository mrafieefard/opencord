import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/channel_kind.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/widgets/channel_glyph.dart';
import 'package:opencord/ui/widgets/section_label.dart';
import 'package:opencord/ui/widgets/settings.dart';

import '../../support/pump.dart';

void main() {
  testWidgets('choice cards select a value and outline the chosen one', (
    tester,
  ) async {
    String? picked;
    await pumpThemed(
      tester,
      Center(
        child: SizedBox(
          width: 600,
          child: SettingsChoiceCards<String>(
            value: 'dark',
            onChanged: (value) => picked = value,
            options: const [
              ChoiceCardOption(value: 'system', label: 'System'),
              ChoiceCardOption(value: 'dark', label: 'Dark'),
              ChoiceCardOption(value: 'light', label: 'Light'),
            ],
          ),
        ),
      ),
    );

    await tester.tap(find.text('Light'));
    final chosen = tester.widget<AnimatedContainer>(
      find.ancestor(
        of: find.text('Dark'),
        matching: find.byType(AnimatedContainer),
      ),
    );
    final border = (chosen.decoration! as BoxDecoration).border! as Border;

    expect(picked, 'light');
    expect(border.top.width, 1.5);
    expect(border.top.color, OcColors.dark.text);
  });

  testWidgets('a switch row toggles from anywhere on the row', (tester) async {
    bool? changedTo;
    await pumpThemed(
      tester,
      SettingsSection(
        title: 'Appearance',
        children: [
          SettingsSwitchRow(
            title: 'Reduce motion',
            value: false,
            onChanged: (value) => changedTo = value,
          ),
        ],
      ),
    );

    await tester.tap(find.text('Reduce motion'));

    expect(changedTo, isTrue);
    expect(find.text('APPEARANCE'), findsOneWidget);
  });

  testWidgets('section labels are upper case', (tester) async {
    await pumpThemed(tester, const SectionLabel('Text channels'));

    expect(find.text('TEXT CHANNELS'), findsOneWidget);
  });

  testWidgets('channel glyphs pick an icon per kind and invert when selected', (
    tester,
  ) async {
    await pumpThemed(
      tester,
      const Row(
        children: [
          ChannelGlyph(kind: ChannelKind.text),
          ChannelGlyph(kind: ChannelKind.announcement, selected: true),
          ChannelGlyph(kind: ChannelKind.voice, locked: true),
        ],
      ),
    );

    final icons = tester
        .widgetList<Icon>(find.byType(Icon))
        .map((icon) => icon.icon)
        .toList();
    final selected = tester.widget<Icon>(find.byIcon(OcIcons.campaign));

    expect(
      icons,
      containsAll([
        OcIcons.tag,
        OcIcons.campaign,
        OcIcons.volumeUp,
        OcIcons.lock,
      ]),
    );
    expect(selected.color, OcColors.dark.onAccent);
  });
}
