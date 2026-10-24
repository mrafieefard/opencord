import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/home/friends_page.dart';
import 'package:opencord/features/home/private_chat.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/shell/no_servers.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';

/// The main content area of the home view (§6.2).
class HomeMain extends ConsumerStatefulWidget {
  const HomeMain({super.key});

  @override
  ConsumerState<HomeMain> createState() => _HomeMainState();
}

class _HomeMainState extends ConsumerState<HomeMain> {
  bool _checkedStartedFresh = false;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) => _checkStartedFresh());
  }

  void _checkStartedFresh() {
    if (!mounted || _checkedStartedFresh) return;
    final repo = ref.read(repositoryProvider);
    if (repo.privateStartedFresh) {
      _checkedStartedFresh = true;
      repo.acknowledgePrivateStartedFresh();
      showOcDialog(
        context: context,
        builder: (context) => OcDialog(
          title: 'Private messages',
          actions: [
            OcButton.primary(
              label: 'OK',
              onPressed: () => Navigator.of(context).pop(),
            ),
          ],
          child: Text(
            "Your private messages on this device couldn't be opened, so this device starts fresh. New messages will arrive normally.",
            style: OcText.body.copyWith(color: context.oc.textSecondary),
          ),
        ),
      );
    }
  }

  @override
  Widget build(BuildContext context) =>
      // The same surface as a channel's chat area.
      ColoredBox(color: context.oc.chat, child: _content(context));

  Widget _content(BuildContext context) {
    final colors = context.oc;
    final repo = ref.watch(repositoryProvider);
    final contactsState = ref.watch(privateContactsProvider);
    final unavailable = repo.privateUnavailable ?? contactsState.error;

    if (unavailable != null) {
      return Center(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 420),
          child: Padding(
            padding: const EdgeInsets.all(OcSpace.s24),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                Icon(OcIcons.error, size: 48, color: colors.textMuted),
                const SizedBox(height: OcSpace.s16),
                Text(
                  unavailable.message,
                  textAlign: TextAlign.center,
                  style: OcText.body.copyWith(color: colors.textSecondary),
                ),
              ],
            ),
          ),
        ),
      );
    }

    final servers = ref.watch(serverListProvider);
    if (servers.isEmpty && contactsState.all.isEmpty) {
      return const NoServersView();
    }

    final currentPeer = ref.watch(currentPeerProvider);
    if (currentPeer != null) {
      return PrivateChat(peer: currentPeer);
    }

    return const FriendsPage();
  }
}
