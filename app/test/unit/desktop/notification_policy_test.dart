import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/app_settings.dart';
import 'package:opencord/features/desktop/notification_policy.dart';

const _self = 1000;
final _settings = AppSettings.defaults(TargetPlatform.linux);

Message _message(String content, {int author = 1001, bool system = false}) =>
    Message(
      id: 1,
      channelId: 10,
      authorId: author,
      content: content,
      createdAt: DateTime(2026, 10, 7),
      systemEvent: system ? SystemEvent.memberJoined : null,
    );

NotifyAs _notify(
  Message message, {
  AppSettings? settings,
  bool muted = false,
  bool focused = false,
  SelfPresence presence = SelfPresence.online,
}) => notifyAs(
  message: message,
  selfId: _self,
  settings: settings ?? _settings,
  muted: muted,
  windowFocused: focused,
  presence: presence,
);

void main() {
  test('a message from someone else notifies while the app is away', () {
    expect(_notify(_message('hello')), NotifyAs.message);
    expect(_notify(_message('hi <@$_self>')), NotifyAs.mention);
  });

  test('nothing while the window is focused (§15)', () {
    expect(_notify(_message('hi <@$_self>'), focused: true), NotifyAs.none);
  });

  test('your own messages and system notices never notify', () {
    expect(_notify(_message('mine', author: _self)), NotifyAs.none);
    expect(_notify(_message('', system: true)), NotifyAs.none);
  });

  test('mentions only keeps mentions', () {
    final settings = _settings.copyWith(mentionsOnly: true);

    expect(_notify(_message('hello'), settings: settings), NotifyAs.none);
    expect(
      _notify(_message('<@$_self> look'), settings: settings),
      NotifyAs.mention,
    );
  });

  test('muted channels notify only when they mention you', () {
    expect(_notify(_message('hello'), muted: true), NotifyAs.none);
    expect(_notify(_message('<@$_self>'), muted: true), NotifyAs.mention);
  });

  test('turning notifications off or do not disturb silences them', () {
    expect(
      _notify(
        _message('<@$_self>'),
        settings: _settings.copyWith(desktopNotifications: false),
      ),
      NotifyAs.none,
    );
    expect(
      _notify(_message('<@$_self>'), presence: SelfPresence.doNotDisturb),
      NotifyAs.none,
    );
  });

  group('attention (§15 urgency, taskbar flash)', () {
    bool attention(
      Message message, {
      AppSettings? settings,
      bool focused = false,
      SelfPresence presence = SelfPresence.online,
    }) => wantsAttention(
      message: message,
      selfId: _self,
      settings: settings ?? _settings,
      windowFocused: focused,
      presence: presence,
    );

    test('a mention while away asks for attention', () {
      expect(attention(_message('<@$_self> look')), isTrue);
      expect(attention(_message('look')), isFalse);
    });

    test('even with notifications off, but not with the flash off', () {
      expect(
        attention(
          _message('<@$_self>'),
          settings: _settings.copyWith(desktopNotifications: false),
        ),
        isTrue,
      );
      expect(
        attention(
          _message('<@$_self>'),
          settings: _settings.copyWith(flashTaskbar: false),
        ),
        isFalse,
      );
    });

    test('not while focused, in do not disturb, or for yourself', () {
      expect(attention(_message('<@$_self>'), focused: true), isFalse);
      expect(
        attention(_message('<@$_self>'), presence: SelfPresence.doNotDisturb),
        isFalse,
      );
      expect(attention(_message('<@$_self>', author: _self)), isFalse);
    });
  });
}
