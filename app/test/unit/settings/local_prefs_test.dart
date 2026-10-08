import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/core/settings/local_prefs.dart';

ProviderContainer app(
  KeyValueStore store, {
  Set<String> mutedByDefault = const {},
}) {
  final container = ProviderContainer(
    overrides: [
      keyValueStoreProvider.overrideWithValue(store),
      defaultMutedServersProvider.overrideWithValue(mutedByDefault),
    ],
  );
  addTearDown(container.dispose);
  return container;
}

void main() {
  test('muted servers and channels are remembered', () {
    final store = MemoryKeyValueStore();

    app(store).read(notificationPrefsProvider.notifier)
      ..toggleServer('a:1')
      ..toggleChannel('b:2', 10);
    final prefs = app(store).read(notificationPrefsProvider);

    expect(prefs.serverMuted('a:1'), isTrue);
    expect(prefs.channelMuted('b:2', 10), isTrue);
    expect(prefs.channelMuted('b:2', 11), isFalse);
    expect(
      prefs.muted('a:1', 99),
      isTrue,
      reason: 'a muted server mutes its channels',
    );
  });

  test('servers muted by default apply only until the user chooses', () {
    final store = MemoryKeyValueStore();
    final first = app(store, mutedByDefault: {'h:1'});
    final mutedAtFirst = first
        .read(notificationPrefsProvider)
        .serverMuted('h:1');

    first.read(notificationPrefsProvider.notifier).toggleServer('h:1');
    final later = app(
      store,
      mutedByDefault: {'h:1'},
    ).read(notificationPrefsProvider);

    expect(mutedAtFirst, isTrue);
    expect(later.serverMuted('h:1'), isFalse);
  });

  test('collapsed categories are remembered per server', () {
    final store = MemoryKeyValueStore();

    app(store).read(collapsedCategoriesProvider.notifier).toggle('a:1', 5);
    final collapsed = app(store).read(collapsedCategoriesProvider);

    expect(collapsed.contains(categoryKey('a:1', 5)), isTrue);
    expect(collapsed.contains(categoryKey('b:2', 5)), isFalse);
  });

  test('the chosen presence and the audio choices are remembered', () {
    final store = MemoryKeyValueStore();
    final first = app(store);

    first.read(selfPresenceProvider.notifier).choose(SelfPresence.doNotDisturb);
    first
        .read(audioSettingsProvider.notifier)
        .update(
          (audio) =>
              audio.copyWith(inputVolume: 150, inputMode: InputMode.pushToTalk),
        );
    final later = app(store);

    expect(later.read(selfPresenceProvider), SelfPresence.doNotDisturb);
    expect(later.read(audioSettingsProvider).inputVolume, 150);
    expect(later.read(audioSettingsProvider).inputMode, InputMode.pushToTalk);
  });

  test('devices are kept by id, with a name for while they are unplugged', () {
    const usb = AudioDevice(id: 'pipewire:usb-mic', name: 'USB mic');

    final chosen = const AudioSettings().withInput(usb);
    final restored = AudioSettings.fromJson(chosen.toJson());
    final back = restored.withInput(null);

    expect(
      (restored.inputDevice, restored.inputDeviceName),
      ('pipewire:usb-mic', 'USB mic'),
    );
    expect((back.inputDevice, back.inputDeviceName), (null, null));
  });

  test('device names saved before Phase 2 mean the default device', () {
    final old = AudioSettings.fromJson({
      'inputDevice': 'USB headset microphone',
      'outputDevice': 'Default',
    });

    expect(old.inputDevice, isNull);
    expect(old.outputDevice, isNull);
  });

  test('the audio settings tell voice media what to use', () {
    const usb = AudioDevice(id: 'pipewire:usb-mic', name: 'USB mic');

    final config = const AudioSettings()
        .withInput(usb)
        .copyWith(inputMode: InputMode.pushToTalk, outputVolume: 80)
        .config;

    expect(
      config,
      const AudioConfig(
        inputDevice: 'pipewire:usb-mic',
        pushToTalk: true,
        outputVolume: 80,
      ),
    );
  });

  test('volumes stay within 0–200 %', () {
    const audio = AudioSettings();

    expect(audio.copyWith(inputVolume: 300).inputVolume, 200);
    expect(audio.copyWith(outputVolume: -5).outputVolume, 0);
  });

  test('voice processing choices are remembered', () {
    final chosen = const AudioSettings().copyWith(
      noiseSuppression: NoiseSuppression.high,
      automaticSensitivity: false,
      sensitivityDbfs: -55,
      echoCancellation: false,
      automaticGain: false,
      pushToTalkRelease: const Duration(milliseconds: 500),
    );

    final restored = AudioSettings.fromJson(chosen.toJson());

    expect(restored.noiseSuppression, NoiseSuppression.high);
    expect(restored.automaticSensitivity, isFalse);
    expect(restored.sensitivityDbfs, -55);
    expect(restored.echoCancellation, isFalse);
    expect(restored.automaticGain, isFalse);
    expect(restored.pushToTalkRelease, const Duration(milliseconds: 500));
  });

  test('noise suppression waits for the first run to choose it', () {
    final saved = AudioSettings.fromJson({'inputVolume': 120});

    expect(const AudioSettings().noiseSuppression, isNull);
    expect(saved.noiseSuppression, isNull);
    expect(saved.config.noiseSuppression, NoiseSuppression.standard);
    expect(
      saved.copyWith(noiseSuppression: NoiseSuppression.off).noiseSuppression,
      NoiseSuppression.off,
    );
  });

  test('the release delay stays within 2 s and sensitivity within range', () {
    const audio = AudioSettings();

    expect(
      audio
          .copyWith(pushToTalkRelease: const Duration(seconds: 5))
          .pushToTalkRelease,
      const Duration(seconds: 2),
    );
    expect(
      audio
          .copyWith(pushToTalkRelease: const Duration(milliseconds: -10))
          .pushToTalkRelease,
      Duration.zero,
    );
    expect(audio.copyWith(sensitivityDbfs: 10).sensitivityDbfs, 0);
    expect(audio.copyWith(sensitivityDbfs: -150).sensitivityDbfs, -100);
  });

  test('voice processing choices reach voice media', () {
    final config = const AudioSettings()
        .copyWith(
          noiseSuppression: NoiseSuppression.high,
          automaticSensitivity: false,
          sensitivityDbfs: -50,
          echoCancellation: false,
          automaticGain: false,
          pushToTalkRelease: const Duration(milliseconds: 750),
        )
        .config;

    expect(
      config,
      const AudioConfig(
        noiseSuppression: NoiseSuppression.high,
        automaticSensitivity: false,
        sensitivityDbfs: -50,
        echoCancellation: false,
        automaticGain: false,
        pushToTalkRelease: Duration(milliseconds: 750),
      ),
    );
  });

  test('hotkeys start unbound and are remembered', () {
    final container = app(MemoryKeyValueStore());

    final before = container.read(hotkeyBindingsProvider);
    container
        .read(hotkeyBindingsProvider.notifier)
        .bind(HotkeyAction.pushToTalk, 'grave');
    final restored = HotkeyBindings.fromJson(
      container.read(hotkeyBindingsProvider).toJson(),
    );

    expect(before.bindings, isEmpty);
    expect(restored.bindings, {HotkeyAction.pushToTalk: 'grave'});
    expect(
      HotkeyBindings.fromJson({'pushToTalk': 5, 'nonsense': 'F1'}).bindings,
      isEmpty,
    );
  });
}
