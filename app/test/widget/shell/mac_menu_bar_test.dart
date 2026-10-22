import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/switcher/quick_switcher.dart';

import '../../support/app.dart';

/// The menus the app last gave the platform, as sent over the channel.
List<Map<Object?, Object?>> _menus = [];

void _captureMenus(WidgetTester tester) {
  _menus = [];
  tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
    SystemChannels.menu,
    (call) async {
      if (call.method == 'Menu.setMenus') {
        final windows = call.arguments as Map<Object?, Object?>;
        _menus = [
          for (final menu in windows['0']! as List<Object?>)
            menu! as Map<Object?, Object?>,
        ];
      }
      return null;
    },
  );
  addTearDown(
    () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      SystemChannels.menu,
      null,
    ),
  );
}

Map<Object?, Object?> _item(String path) {
  var items = _menus;
  Map<Object?, Object?>? found;
  for (final label in path.split(' › ')) {
    found = items.firstWhere((item) => item['label'] == label);
    items = [
      for (final child in (found['children'] as List<Object?>? ?? const []))
        child! as Map<Object?, Object?>,
    ];
  }
  return found!;
}

Future<void> _select(WidgetTester tester, String path) async {
  await tester.binding.defaultBinaryMessenger.handlePlatformMessage(
    SystemChannels.menu.name,
    SystemChannels.menu.codec.encodeMethodCall(
      MethodCall('Menu.selectedCallback', _item(path)['id']),
    ),
    (_) {},
  );
  await tester.pump(const Duration(milliseconds: 300));
}

void main() {
  testWidgets('macOS gets the menu bar of §15', (tester) async {
    _captureMenus(tester);
    final app = await MockApp.pump(tester, platform: TargetPlatform.macOS);

    expect(_menus.map((menu) => menu['label']), [
      'Opencord',
      'Edit',
      'View',
      'Server',
      'Window',
      'Help',
    ]);
    final switcher = _item('View › Quick Switcher');
    expect(switcher['shortcutTrigger'], LogicalKeyboardKey.keyK.keyId);
    expect(_item('Edit › Copy')['enabled'], isTrue);
    await app.dispose(tester);
  });

  testWidgets('menu items do what their shortcuts do', (tester) async {
    _captureMenus(tester);
    final app = await MockApp.pump(tester, platform: TargetPlatform.macOS);

    await _select(tester, 'View › Quick Switcher');
    expect(find.byType(QuickSwitcher), findsOneWidget);
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pump(const Duration(milliseconds: 300));

    await _select(tester, 'Server › Mute');
    expect(app.read(voiceSessionProvider).muted, isTrue);
    expect(_item('Server › Unmute')['enabled'], isTrue);
    await app.dispose(tester);
  });

  testWidgets('other desktops have no menu bar', (tester) async {
    _captureMenus(tester);
    final app = await MockApp.pump(tester, platform: TargetPlatform.linux);

    expect(_menus, isEmpty);
    await app.dispose(tester);
  });
}
