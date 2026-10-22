import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/features/chat/emoji_picker.dart';
import 'package:opencord/features/chat/reaction_pill.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_theme.dart';
import 'package:opencord/ui/widgets/popover.dart';

/// Pumps [child] with providers above the app, as in the real app, so
/// popovers on the root overlay can read them.
Future<ProviderContainer> _pump(WidgetTester tester, Widget child) async {
  final container = ProviderContainer(
    overrides: [keyValueStoreProvider.overrideWithValue(MemoryKeyValueStore())],
  );
  addTearDown(container.dispose);
  tester.view.physicalSize = const Size(600, 700);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  await tester.pumpWidget(
    UncontrolledProviderScope(
      container: container,
      child: MaterialApp(
        debugShowCheckedModeBanner: false,
        theme: buildTheme(OcColors.dark),
        // Popovers draw on Material; the picker is pumped bare here.
        home: Material(
          color: OcColors.dark.chat,
          child: Center(child: child),
        ),
      ),
    ),
  );
  return container;
}

void main() {
  testWidgets('searching narrows the grid and Enter picks the first match', (
    tester,
  ) async {
    final picked = <String>[];
    final container = await _pump(tester, EmojiPicker(onPicked: picked.add));

    await tester.enterText(find.byType(TextField), 'rocket');
    await tester.pump();
    expect(find.text('🚀'), findsWidgets);
    expect(find.text('😀'), findsNothing);

    await tester.sendKeyEvent(LogicalKeyboardKey.enter);

    expect(picked, ['🚀']);
    expect(container.read(emojiUsageProvider), {'🚀': 1});
  });

  testWidgets('arrow keys move the selection and the footer names it', (
    tester,
  ) async {
    await _pump(tester, EmojiPicker(onPicked: (_) {}));
    await tester.enterText(find.byType(TextField), 'heart');
    await tester.pump();
    expect(find.text(':heart:'), findsOneWidget);

    await tester.sendKeyEvent(LogicalKeyboardKey.arrowRight);
    await tester.pump();

    expect(find.text(':heart:'), findsNothing);
    expect(find.textContaining(RegExp(r'^:\w+:$')), findsOneWidget);
  });

  testWidgets('nothing found says so', (tester) async {
    await _pump(tester, EmojiPicker(onPicked: (_) {}));

    await tester.enterText(find.byType(TextField), 'zzzzqx');
    await tester.pump();

    expect(find.text('No emoji match "zzzzqx"'), findsOneWidget);
  });

  testWidgets('used emoji lead the frequently used section', (tester) async {
    final container = await _pump(tester, EmojiPicker(onPicked: (_) {}));
    container.read(emojiUsageProvider.notifier)
      ..use('🚀')
      ..use('🚀');
    await tester.pump();

    // The first cell of the grid.
    final first = tester.widget<Text>(
      find
          .descendant(of: find.byType(SliverGrid), matching: find.byType(Text))
          .first,
    );
    expect(first.data, '🚀');
  });

  testWidgets('the quick pill offers eight, and more opens the picker', (
    tester,
  ) async {
    String? result;
    await _pump(
      tester,
      Builder(
        builder: (context) => TextButton(
          onPressed: () async => result = await pickReaction(
            context,
            anchor: globalRectOf(context),
          ),
          child: const Text('react'),
        ),
      ),
    );

    await tester.tap(find.text('react'));
    await tester.pumpAndSettle();
    expect(find.byType(QuickReactions), findsOneWidget);
    for (final emoji in defaultReactions) {
      expect(find.text(emoji), findsOneWidget);
    }
    await tester.tap(find.text('🎉'));
    await tester.pumpAndSettle();
    expect(result, '🎉');

    await tester.tap(find.text('react'));
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('More reactions'));
    await tester.pumpAndSettle();
    expect(find.byType(EmojiPicker), findsOneWidget);
    await tester.enterText(find.byType(TextField), 'tada');
    await tester.pump();
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pumpAndSettle();
    expect(result, '🎉');
    expect(find.byType(EmojiPicker), findsNothing);
  });
}
