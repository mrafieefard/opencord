import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// The camera chosen in this run, as (device id, name); `null` for the
/// system's.
final chosenCameraProvider =
    NotifierProvider<ChosenCameraNotifier, (String, String)?>(
      ChosenCameraNotifier.new,
    );

class ChosenCameraNotifier extends Notifier<(String, String)?> {
  @override
  (String, String)? build() => null;

  /// Turning the camera on starts from [camera] from now on; with the
  /// camera on, it switches at once.
  Future<void> choose((String, String)? camera) async {
    state = camera;
    await ref.read(repositoryProvider).chooseCamera(camera?.$1);
  }
}

/// Why the camera could not start, for a toast.
String cameraErrorText(RepoException error) => switch (error.kind) {
  RepoErrorKind.cameraLimit => 'Camera limit reached in this channel',
  RepoErrorKind.cameraDenied =>
    "Camera access is off. Allow it in your system's privacy settings.",
  RepoErrorKind.cameraMissing => 'No camera was found.',
  RepoErrorKind.cameraUnsupported => "Cameras don't work on this system yet.",
  RepoErrorKind.forbidden =>
    "You don't have permission to use your camera here.",
  _ => error.message,
};

/// The cameras to choose from, opened at [position] (global): from the
/// Camera buttons' right-click and the Voice & audio page.
Future<void> showCameraMenu(
  BuildContext context,
  WidgetRef ref,
  Offset position,
) async {
  List<(String, String)> cameras;
  try {
    cameras = await ref.read(repositoryProvider).cameras();
  } on Object {
    cameras = const [];
  }
  if (!context.mounted) return;
  final chosen = ref.read(chosenCameraProvider);
  Future<void> choose((String, String)? camera) async {
    try {
      await ref.read(chosenCameraProvider.notifier).choose(camera);
    } on RepoException catch (error) {
      if (context.mounted) showOcToast(context, cameraErrorText(error));
    }
  }

  showOcMenu(
    context: context,
    position: position,
    entries: [
      OcMenuItem(
        label: 'Default',
        checked: chosen == null,
        onSelected: () => choose(null),
      ),
      for (final camera in cameras)
        OcMenuItem(
          label: camera.$2,
          checked: camera.$1 == chosen?.$1,
          onSelected: () => choose(camera),
        ),
    ],
  );
}
