import 'dart:async';
import 'dart:convert';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:opencord/core/logs/app_log.dart';
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
          unawaited(
            repository
                .closeView(openViewId!)
                .catchError(
                  (Object e) => appLog('close view failed: $e', name: 'views'),
                ),
          );
        }
      });

      () async {
        final pendingInputs = <String>[];
        final contextCompleter = Completer<String>();

        inputSub = controller.inputs.listen((json) {
          if (openViewId != null) {
            repository.viewInput(openViewId!, json);
          } else {
            if (!contextCompleter.isCompleted) {
              try {
                final decoded = jsonDecode(json);
                if (decoded is Map && decoded['kind'] == 'context') {
                  contextCompleter.complete(json);
                  return;
                }
              } catch (_) {}
            }
            pendingInputs.add(json);
          }
        });

        try {
          final contextJson = await contextCompleter.future;
          if (disposed) return;

          final viewId = await repository.openView(
            channel.server,
            channel.channel,
            contextJson,
          );
          if (disposed) {
            unawaited(
              repository
                  .closeView(viewId)
                  .catchError(
                    (Object e) =>
                        appLog('close view failed: $e', name: 'views'),
                  ),
            );
            return;
          }
          openViewId = viewId;

          outputSub = repository.viewOutputs
              .where((output) => output.viewId == viewId)
              .listen((output) {
                controller.applyBatch(output.json);
              });

          for (final pending in pendingInputs) {
            repository.viewInput(viewId, pending);
          }
          pendingInputs.clear();
        } on RepoException catch (e) {
          appLog('open view failed: $e', name: 'views');
          if (!disposed) {
            controller.reportError(e.message);
          }
        }
      }();

      return controller;
    });
