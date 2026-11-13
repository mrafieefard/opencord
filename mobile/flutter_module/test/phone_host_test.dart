import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord_plugin_area/src/area.dart';
import 'package:opencord_plugin_area/src/channel.dart';
import 'package:opencord_plugin_area/src/phone_host.dart';

import 'support.dart';

const _codec = StandardMethodCodec();

/// React Native's end of the host calls: records each, and answers it with
/// what [answers] gives for its method (`{"ok": …}`), `{"error": "…"}` for
/// a method in [errors], and nothing for one in [silent].
class _HostSide {
  _HostSide(this.tester) {
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(const MethodChannel(AreaChannel.name), (call) async {
      if (call.method != 'hostCall') return null;
      final args = call.arguments as Map;
      final method = args['method'] as String;
      final sent = jsonDecode(args['json'] as String);
      calls.add((method, sent));
      if (silent.contains(method)) return null;
      final reply = errors.contains(method) ? {'error': 'failed'} : {'ok': answers[method]?.call(sent)};
      // Answered later, as React Native would.
      scheduleMicrotask(() {
        tester.binding.defaultBinaryMessenger.handlePlatformMessage(
          AreaChannel.name,
          _codec.encodeMethodCall(MethodCall('hostReply', {'id': args['id'], 'json': jsonEncode(reply)})),
          (_) {},
        );
      });
      return null;
    });
    addTearDown(
      () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(const MethodChannel(AreaChannel.name), null),
    );
  }

  final WidgetTester tester;
  final calls = <(String, Object?)>[];
  final answers = <String, Object? Function(Object? sent)>{};
  final errors = <String>{};
  final silent = <String>{};

  List<Object?> sentTo(String method) => [for (final (m, sent) in calls) if (m == method) sent];
}

PhoneBlocksHost _host(AreaChannel channel) =>
    PhoneBlocksHost(channel: channel, theme: blocksThemeOf(AreaSetup.fromJson(jsonDecode(setupJson()) as Map<String, Object?>)));

Future<void> _show(WidgetTester tester, Widget child) => tester.pumpWidget(
  MaterialApp(home: Scaffold(body: Center(child: child))),
);

/// A real file for the image and font answers.
String _tempFile(String name, List<int> bytes) {
  final dir = Directory.systemTemp.createTempSync('phone_host_test');
  addTearDown(() => dir.deleteSync(recursive: true));
  return (File('${dir.path}/$name')..writeAsBytesSync(bytes)).path;
}

void main() {
  // Fails if a known name has no glyph, or an unknown one gets one.
  testWidgets('icon is the plugin glyph, null for an unknown name', (tester) async {
    final host = _host(AreaChannel());
    expect(host.icon('add')?.codePoint, 0xe145);
    expect(host.icon('add')?.fontFamily, 'MaterialSymbolsRounded');
    expect(host.icon('no_such_icon'), isNull);
  });

  // Fails if the image isn't asked for with its source, or the file the
  // answer names isn't the one drawn.
  testWidgets('image asks with its source and draws the answered file', (tester) async {
    final side = _HostSide(tester);
    final path = _tempFile('a.png', [1, 2, 3]);
    side.answers['image'] = (_) => {'path': path};
    final host = _host(AreaChannel());
    final source = {'asset': 'assets/ticket.png', 'version': 1};
    final provider = await host.image(source);
    expect(side.sentTo('image'), [
      {'source': source},
    ]);
    expect(provider, isA<FileImage>());
    expect((provider! as FileImage).file.path, path);
  });

  // Fails if a font isn't asked for under assets/fonts, or its bytes aren't
  // the answered file's.
  testWidgets('font asks for the asset and reads its bytes', (tester) async {
    final side = _HostSide(tester);
    final path = _tempFile('f.ttf', [9, 8, 7, 6]);
    side.answers['asset'] = (_) => {'path': path};
    final host = _host(AreaChannel());
    // A real file read: outside the tester's fake clock.
    final data = await tester.runAsync(() => host.font('display.ttf'));
    expect(side.sentTo('asset'), [
      {'path': 'assets/fonts/display.ttf'},
    ]);
    expect(data!.buffer.asUint8List(), [9, 8, 7, 6]);
  });

  // Fails if the picker's choice or a cancel isn't passed back.
  testWidgets('pickFile sends image and accept, and passes the choice or a cancel', (tester) async {
    final side = _HostSide(tester);
    side.answers['pick'] = (_) => {'path': '/files/photo.jpg'};
    final host = _host(AreaChannel());
    expect(await host.pickFile(image: true, accept: ['jpg', 'png']), '/files/photo.jpg');
    expect(side.sentTo('pick'), [
      {
        'image': true,
        'accept': ['jpg', 'png'],
      },
    ]);
    side.answers['pick'] = (_) => <String, Object?>{};
    expect(await host.pickFile(image: false), isNull);
  });

  // Fails if an upload doesn't carry its node and path.
  testWidgets('upload sends the node and path', (tester) async {
    final side = _HostSide(tester);
    final host = _host(AreaChannel());
    await host.upload(12, '/files/a.pdf');
    expect(side.sentTo('upload'), [
      {'node_id': 12, 'path': '/files/a.pdf'},
    ]);
  });

  // Fails if the viewer's answer to the link dialog isn't the result.
  testWidgets('confirmLink asks with the url and returns whether it opened', (tester) async {
    final side = _HostSide(tester);
    side.answers['link'] = (_) => {'opened': true};
    final host = _host(AreaChannel());
    expect(await host.confirmLink('https://opencord.dev'), isTrue);
    expect(side.sentTo('link'), [
      {'url': 'https://opencord.dev'},
    ]);
    side.answers['link'] = (_) => {'opened': false};
    expect(await host.confirmLink('https://opencord.dev'), isFalse);
  });

  // Fails if copy, open_channel or sound send another shape, or a sound's
  // onEnd doesn't run when React Native says it ended.
  testWidgets('copy, openChannel and playSound send their arguments', (tester) async {
    final side = _HostSide(tester);
    final host = _host(AreaChannel());
    host.copy('ticket #12');
    host.openChannel('114795140719050752');
    var ended = false;
    await host.playSound('assets/ding.ogg', 0.5, () => ended = true);
    await tester.pump();
    expect(side.sentTo('copy'), [
      {'text': 'ticket #12'},
    ]);
    expect(side.sentTo('open_channel'), [
      {'id': '114795140719050752'},
    ]);
    expect(side.sentTo('sound'), [
      {'asset': 'assets/ding.ogg', 'volume': 0.5},
    ]);
    expect(ended, isTrue);
  });

  // Fails if the user block doesn't show the answered name.
  testWidgets('user shows the answered name', (tester) async {
    final side = _HostSide(tester);
    side.answers['user'] = (_) => {'name': 'Ava', 'avatar_path': null};
    final host = _host(AreaChannel());
    await _show(tester, host.user('1001', 32, true));
    await tester.pump();
    await tester.pump();
    expect(side.sentTo('user'), [
      {'id': '1001'},
    ]);
    expect(find.text('Ava'), findsOneWidget);
    expect(find.text('A'), findsOneWidget); // The initial, with no avatar.
  });

  // Fails if mentions in rich text don't show the answered names, or a link
  // press doesn't go through the link dialog.
  testWidgets('richText shows answered names and confirms links', (tester) async {
    final side = _HostSide(tester);
    side.answers['names'] = (sent) => {
      'users': {'42': 'Ava'},
      'channels': {'5': 'general'},
    };
    side.answers['link'] = (_) => {'opened': true};
    final host = _host(AreaChannel());
    await _show(tester, host.richText('hi <@42> in <#5>', const TextStyle(fontSize: 14)));
    await tester.pump();
    await tester.pump();
    expect(side.sentTo('names'), [
      {
        'users': ['42'],
        'channels': ['5'],
      },
    ]);
    expect(find.textContaining('@Ava', findRichText: true), findsOneWidget);
    expect(find.textContaining('#general', findRichText: true), findsOneWidget);
  });

  // Fails if a refused host call throws into the renderer instead of the
  // block's fallback (no image, no font, no file, link not opened).
  testWidgets('a host call that errors draws the block\'s fallback, not an exception', (tester) async {
    final side = _HostSide(tester);
    side.errors.addAll(['image', 'asset', 'pick', 'link', 'user']);
    final host = _host(AreaChannel());
    expect(await host.image({'asset': 'x.png'}), isNull);
    expect(await host.font('x.ttf'), isNull);
    expect(await host.pickFile(image: false), isNull);
    expect(await host.confirmLink('https://x'), isFalse);
    await _show(tester, host.user('7', 32, true));
    await tester.pump();
    await tester.pump();
    expect(find.text('User 7'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  // Fails if an unanswered call waits for ever (a stuck image), or the file
  // picker stops waiting for the viewer.
  testWidgets('a host call with no reply times out after hostCallTimeout, except pick', (tester) async {
    final side = _HostSide(tester);
    side.silent.addAll(['image', 'pick']);
    final host = _host(AreaChannel());
    ImageProvider? image = const AssetImage('not-yet');
    var imageDone = false;
    unawaited(
      host.image({'asset': 'x.png'}).then((value) {
        image = value;
        imageDone = true;
      }),
    );
    var pickDone = false;
    unawaited(host.pickFile(image: true).then((_) => pickDone = true));
    await tester.pump(hostCallTimeout - const Duration(seconds: 1));
    expect(imageDone, isFalse);
    await tester.pump(const Duration(seconds: 2));
    expect(imageDone, isTrue);
    expect(image, isNull);
    await tester.pump(const Duration(minutes: 10));
    expect(pickDone, isFalse);
  });

  // Fails if the same name or user is asked again (a host call per frame).
  testWidgets('names and users are asked once per id', (tester) async {
    final side = _HostSide(tester);
    side.answers['names'] = (_) => {
      'users': {'42': 'Ava'},
      'channels': <String, Object?>{},
    };
    side.answers['user'] = (_) => {'name': 'Kai', 'avatar_path': null};
    final host = _host(AreaChannel());
    await _show(
      tester,
      Column(
        children: [
          host.richText('<@42>', const TextStyle()),
          host.richText('again <@42>', const TextStyle()),
          host.user('1001', 24, true),
          host.user('1001', 24, false),
        ],
      ),
    );
    await tester.pump();
    await tester.pump();
    await _show(tester, Column(children: [host.richText('<@42>', const TextStyle()), host.user('1001', 24, true)]));
    await tester.pump();
    expect(side.sentTo('names').length, 1);
    expect(side.sentTo('user').length, 1);
  });
}
