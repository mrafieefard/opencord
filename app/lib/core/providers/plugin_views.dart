import 'dart:async';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:opencord/core/providers/providers.dart' show ChannelRef;
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord_blocks/opencord_blocks.dart';

/// One plugin view's controller: connects its renderer to the server and
/// closes it when the page leaves.
final pluginViewProvider = Provider.autoDispose
    .family<BlocksController, ChannelRef>((ref, channel) {
      final repository = ref.watch(repositoryProvider);
      final controller = BlocksController();

      StreamSubscription<ViewOutput>? outputSub;
      StreamSubscription<String>? inputSub;
      String? openViewId;
      var disposed = false;

      ref.onDispose(() {
        disposed = true;
        outputSub?.cancel();
        inputSub?.cancel();
        controller.dispose();
        if (openViewId != null) {
          unawaited(repository.closeView(openViewId!));
        }
      });

      () async {
        try {
          final viewId = await repository.openView(
            channel.server,
            channel.channel,
            '{}',
          );
          if (disposed) {
            unawaited(repository.closeView(viewId));
            return;
          }
          openViewId = viewId;

          outputSub = repository.viewOutputs
              .where((output) => output.viewId == viewId)
              .listen((output) {
                controller.applyBatch(output.json);
              });

          inputSub = controller.inputs.listen((json) {
            repository.viewInput(viewId, json);
          });
        } on Object {
          if (!disposed) {
            // Any open error will be handled or closed
          }
        }
      }();

      return controller;
    });
