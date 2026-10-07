import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/app_start.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

import 'package:opencord/features/window/native_window.dart';

import '../../support/app.dart';

class _Window extends NullNativeWindow {
  final calls = <String>[];

  @override
  Future<void> show() async => calls.add('show');

  @override
  Future<void> quit() async => calls.add('quit');
}

void main() {
  testWidgets('without a usable keyring the app says why and tries again', (
    tester,
  ) async {
    final app = await MockApp.pump(
      tester,
      withIdentity: false,
      identityUnavailable: const RepoException(
        RepoErrorKind.other,
        'Opencord keeps your identity in the system keyring, which could not '
        'be used (no Secret Service).',
      ),
    );

    expect(find.textContaining('no Secret Service'), findsOneWidget);
    expect(find.text('Create a new identity'), findsNothing);

    await tester.tap(find.widgetWithText(OcButton, 'Try again'));
    await tester.pumpAndSettle();

    expect(find.text('Create a new identity'), findsOneWidget);
    await app.dispose(tester);
  });

  testWidgets('a start that fails says why in a shown window, with Quit', (
    tester,
  ) async {
    final window = _Window();
    final app = await startupWidget(
      () async => throw const RepoException(
        RepoErrorKind.other,
        'The data folder could not be opened.',
      ),
      window: () => window,
    );
    await tester.pumpWidget(app);

    expect(window.calls, ['show']);
    expect(find.text('Opencord could not start'), findsOneWidget);
    expect(
      find.textContaining('The data folder could not be opened.'),
      findsOneWidget,
    );
    await tester.tap(find.widgetWithText(OcButton, 'Quit'));
    expect(window.calls, ['show', 'quit']);
  });
}
