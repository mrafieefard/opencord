import 'package:fake_async/fake_async.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/features/chat/composer_state.dart';

import '../../support/fixtures.dart';

const general = (server: serverKey, channel: generalId);
const random = (server: serverKey, channel: randomId);

void main() {
  late MemoryKeyValueStore store;
  late ProviderContainer container;

  ProviderContainer fresh() {
    final next = ProviderContainer(
      overrides: [keyValueStoreProvider.overrideWithValue(store)],
    );
    addTearDown(next.dispose);
    return next;
  }

  ComposerNotifier composer(ChannelRef channel) =>
      container.read(composerProvider(channel).notifier);

  ComposerState state(ChannelRef channel) =>
      container.read(composerProvider(channel));

  setUp(() {
    store = MemoryKeyValueStore();
    container = fresh();
  });

  test('drafts are kept per channel and survive a restart', () {
    fakeAsync((async) {
      composer(general).setDraft('half a thought');
      composer(random).setDraft('another');
      async.elapse(const Duration(seconds: 1));

      container = fresh();

      expect(state(general).draft, 'half a thought');
      expect(state(random).draft, 'another');
    });
  });

  test('replying keeps the draft', () {
    final original = message(5);
    composer(general).setDraft('so');

    composer(general).reply(original);

    expect(state(general).replyTo, original);
    expect(state(general).draft, 'so');
  });

  test('editing puts the draft aside and cancelling brings it back', () {
    final mine = message(5, authorId: selfId, content: 'typo hree');
    composer(general).setDraft('unsent');

    composer(general).edit(mine);
    expect(state(general).editing, mine);
    expect(state(general).draft, 'typo hree');

    composer(general).cancel();
    expect(state(general).editing, isNull);
    expect(state(general).draft, 'unsent');
  });

  test('an edit in progress is not saved as the draft', () {
    fakeAsync((async) {
      composer(general).setDraft('unsent');
      composer(general).edit(message(5, authorId: selfId, content: 'x'));
      composer(general).setDraft('x, fixed');
      async.elapse(const Duration(seconds: 1));

      container = fresh();

      expect(state(general).draft, 'unsent');
    });
  });

  test('sending clears the draft, the reply and the edit', () {
    composer(general).setDraft('done');
    composer(general).reply(message(5));

    composer(general).sent();

    expect(state(general), const ComposerState());
  });
}
