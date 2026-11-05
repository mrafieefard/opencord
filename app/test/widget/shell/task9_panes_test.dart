import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/features/members/member_avatar.dart';
import 'package:opencord/features/members/member_panel.dart';
import 'package:opencord/features/music/music_artwork.dart';
import 'package:opencord/features/music/music_controls.dart';
import 'package:opencord/features/music/music_panel.dart';
import 'package:opencord/features/shell/desktop_shell.dart';
import 'package:opencord/features/shell/pane_switch.dart';
import 'package:opencord/features/shell/shell_keys.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/widgets/oc_text_field.dart';
import 'package:opencord/ui/widgets/section_label.dart';

import '../../support/app.dart';

Future<void> _pumpFor(WidgetTester tester, Duration total) async {
  const step = Duration(milliseconds: 50);
  for (var waited = Duration.zero; waited < total; waited += step) {
    await tester.pump(step);
  }
}

Future<void> _settle(WidgetTester tester) =>
    _pumpFor(tester, const Duration(milliseconds: 600));

Finder _inSidebar(String text) => find.descendant(
  of: find.byKey(DesktopShell.sidebarKey),
  matching: find.text(text),
);

Finder _paneSwitchButton(String label) => find.descendant(
  of: find.byType(PaneSwitch),
  matching: find.byWidgetPredicate(
    (w) => w is PaneSwitchButton && w.label == label,
  ),
);

void main() {
  testWidgets('switch_replaces_the_buttons', (tester) async {
    final app = await MockApp.pump(tester);

    // In a text channel without voice/music: header shows Members and count.
    await tester.tap(_inSidebar('general'));
    await _settle(tester);

    expect(find.byType(PaneSwitch), findsNothing);
    expect(find.textContaining('Members'), findsOneWidget);

    // In General (voice channel), music is offered (§5.13): switch is present.
    await tester.tap(_inSidebar('General'));
    await _settle(tester);

    expect(find.byType(PaneSwitch), findsOneWidget);
    final peopleBtnFinder = _paneSwitchButton('People');
    final musicBtnFinder = _paneSwitchButton('Music');
    expect(peopleBtnFinder, findsOneWidget);
    expect(musicBtnFinder, findsOneWidget);

    final colors = tester.element(find.byType(DesktopShell)).oc;

    // Switch container: padding 2, radius 10, background hover.
    final switchContainer = tester.widget<Container>(
      find.descendant(
        of: find.byType(PaneSwitch),
        matching: find.byType(Container),
      ).first,
    );
    expect(switchContainer.padding, const EdgeInsets.all(2));
    final switchDeco = switchContainer.decoration as BoxDecoration;
    expect(switchDeco.borderRadius, BorderRadius.circular(10));
    expect(switchDeco.color, colors.hover);

    // People button is active: background accent, text onAccent.
    final peopleBtn = tester.widget<PaneSwitchButton>(peopleBtnFinder);
    expect(peopleBtn.selected, isTrue);
    final peopleAnimatedContainer = tester.widget<AnimatedContainer>(
      find.descendant(
        of: peopleBtnFinder,
        matching: find.byType(AnimatedContainer),
      ),
    );
    expect(peopleAnimatedContainer.constraints?.minHeight ?? 28, 28);
    expect(
      peopleAnimatedContainer.padding,
      const EdgeInsets.symmetric(horizontal: 12),
    );
    final peopleDeco = peopleAnimatedContainer.decoration as BoxDecoration;
    expect(peopleDeco.color, colors.accent);
    expect(peopleDeco.borderRadius, BorderRadius.circular(8));

    final peopleText = tester.widget<Text>(
      find.descendant(of: peopleBtnFinder, matching: find.byType(Text)),
    );
    expect(peopleText.style?.fontSize, 13);
    expect(peopleText.style?.fontWeight, FontWeight.w500);
    expect(peopleText.style?.color, colors.onAccent);

    // Music button is inactive: background transparent/null, text textSecondary.
    final musicBtn = tester.widget<PaneSwitchButton>(musicBtnFinder);
    expect(musicBtn.selected, isFalse);
    final musicAnimatedContainer = tester.widget<AnimatedContainer>(
      find.descendant(
        of: musicBtnFinder,
        matching: find.byType(AnimatedContainer),
      ),
    );
    final musicDeco = musicAnimatedContainer.decoration as BoxDecoration;
    expect(musicDeco.color, isNull);

    final musicText = tester.widget<Text>(
      find.descendant(of: musicBtnFinder, matching: find.byType(Text)),
    );
    expect(musicText.style?.fontSize, 13);
    expect(musicText.style?.fontWeight, FontWeight.w500);
    expect(musicText.style?.color, colors.textSecondary);

    await app.dispose(tester);
  });

  testWidgets('switch_changes_the_pane_twice', (tester) async {
    final app = await MockApp.pump(tester);
    await tester.tap(_inSidebar('General'));
    await _settle(tester);

    expect(find.byType(MemberPanel), findsOneWidget);
    expect(find.byType(MusicPanel), findsNothing);

    // First change: to Music.
    await tester.tap(_paneSwitchButton('Music'));
    await _settle(tester);
    expect(find.byType(MusicPanel), findsOneWidget);
    expect(find.byType(MemberPanel), findsNothing);

    // Second change: back to People.
    await tester.tap(_paneSwitchButton('People'));
    await _settle(tester);
    expect(find.byType(MemberPanel), findsOneWidget);
    expect(find.byType(MusicPanel), findsNothing);

    // Third change: to Music again.
    await tester.tap(_paneSwitchButton('Music'));
    await _settle(tester);
    expect(find.byType(MusicPanel), findsOneWidget);
    expect(find.byType(MemberPanel), findsNothing);

    await app.dispose(tester);
  });

  testWidgets('member_row_numbers', (tester) async {
    final app = await MockApp.pump(tester);
    await tester.tap(_inSidebar('general'));
    await _settle(tester);

    final rowFinder = find.byType(MemberRow).first;
    expect(rowFinder, findsOneWidget);

    final container = tester.widget<AnimatedContainer>(
      find.descendant(of: rowFinder, matching: find.byType(AnimatedContainer)),
    );
    expect(container.constraints?.minHeight, 44);
    expect(
      container.margin,
      const EdgeInsets.symmetric(horizontal: 8),
    );
    expect(
      container.padding,
      const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
    );
    final deco = container.decoration as BoxDecoration;
    expect(deco.borderRadius, BorderRadius.circular(10));

    // Avatar is size 34.
    final avatar = tester.widget<MemberAvatar>(
      find.descendant(of: rowFinder, matching: find.byType(MemberAvatar)),
    );
    expect(avatar.size, OcSize.memberAvatar); // 34

    // Gap between avatar and name is 10.
    final sizedBox = tester.widget<SizedBox>(
      find.descendant(
        of: rowFinder,
        matching: find.byWidgetPredicate(
          (w) => w is SizedBox && w.width == 10,
        ),
      ),
    );
    expect(sizedBox.width, 10);

    // Check offline row has opacity 0.55.
    final opacities = tester.widgetList<Opacity>(
      find.descendant(
        of: find.byType(MemberRow),
        matching: find.byType(Opacity),
      ),
    );
    expect(opacities.any((o) => o.opacity == 0.55), isTrue);

    await app.dispose(tester);
  });

  testWidgets('section_padding, search_is_the_spec_pill', (tester) async {
    final app = await MockApp.pump(tester);
    await tester.tap(_inSidebar('general'));
    await _settle(tester);

    // Section padding: 10 16 4.
    final sectionFinder = find.descendant(
      of: find.byType(MemberPanel),
      matching: find.byType(SectionLabel),
    ).first;
    expect(sectionFinder, findsOneWidget);
    final section = tester.widget<SectionLabel>(sectionFinder);
    expect(section.padding, const EdgeInsets.fromLTRB(16, 10, 16, 4));

    // Search field: height 36, margin 8, radius 18.
    final searchBox = find.byKey(ShellKeys.memSearch);
    expect(searchBox, findsOneWidget);
    final sizedBox = tester.widget<SizedBox>(searchBox);
    expect(sizedBox.height, 36);

    final searchField = tester.widget<OcTextField>(
      find.descendant(of: searchBox, matching: find.byType(OcTextField)),
    );
    expect(searchField.radius, OcRadius.searchPill); // 18

    final searchPadding = tester.widget<Padding>(
      find.ancestor(of: searchBox, matching: find.byType(Padding)).first,
    );
    expect(searchPadding.padding, const EdgeInsets.all(8));

    await app.dispose(tester);
  });

  testWidgets('music_numbers', (tester) async {
    final app = await MockApp.pump(tester);
    await tester.tap(_inSidebar('General'));
    await _settle(tester);
    await tester.tap(_paneSwitchButton('Music'));
    await _settle(tester);

    expect(find.byType(MusicPanel), findsOneWidget);

    final colors = tester.element(find.byType(DesktopShell)).oc;

    // Art: height 172, radius 14.
    final art = tester.widget<MusicArtwork>(
      find.descendant(
        of: find.byType(NowPlayingView),
        matching: find.byType(MusicArtwork),
      ).first,
    );
    expect(art.height, 172);
    expect(art.radius, 14);
    expect(art.placeholderColor, colors.surface);
    expect(art.placeholderIconSize, 48);

    // Title: 15/600.
    final title = tester.widget<Text>(
      find.descendant(
        of: find.byType(NowPlayingView),
        matching: find.text('Nothing playing'),
      ),
    );
    expect(title.style?.fontSize, 15);
    expect(title.style?.fontWeight, FontWeight.w600);

    // Progress: height 4, radius 2.
    final sliderTheme = tester.widget<SliderTheme>(
      find.descendant(
        of: find.byType(MusicProgress),
        matching: find.byType(SliderTheme),
      ),
    );
    expect(sliderTheme.data.trackHeight, 4);
    expect(sliderTheme.data.inactiveTrackColor, colors.selected);
    expect(sliderTheme.data.activeTrackColor, colors.text);

    // Play button: 48x48.
    final playBox = tester.widget<AnimatedContainer>(
      find.descendant(
        of: find.byType(MusicTransport),
        matching: find.byWidgetPredicate(
          (w) => w is AnimatedContainer && w.constraints?.minWidth == 48,
        ),
      ),
    );
    expect(playBox.constraints?.minWidth, 48);
    expect(playBox.constraints?.minHeight, 48);

    // Transport gaps: 10 px apart.
    final transportRow = tester.widget<Row>(
      find.descendant(
        of: find.byType(MusicTransport),
        matching: find.byType(Row),
      ),
    );
    final gapWidths = transportRow.children
        .whereType<SizedBox>()
        .map((b) => b.width)
        .toList();
    expect(gapWidths.every((w) => w == 10), isTrue);

    await app.dispose(tester);
  });
}
