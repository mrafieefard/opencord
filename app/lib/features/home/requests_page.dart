import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/private.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/home/friend_rows.dart';
import 'package:opencord/features/home/person_card.dart';
import 'package:opencord/features/home/private_page.dart';
import 'package:opencord/features/home/request_bar.dart';
import 'package:opencord/features/shell/shell_keys.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/panel_header.dart';

/// The message requests page (§5.16).
class RequestsPage extends ConsumerWidget {
  const RequestsPage({super.key, this.onOpenSidebar});

  final VoidCallback? onOpenSidebar;

  static const _gap = 8.0;
  static const _minCard = 280.0;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final contacts = ref.watch(privateContactsProvider);
    final requests = contacts.messageRequests;

    return Column(
      children: [
        PanelHeader(
          key: ShellKeys.hd,
          padding: EdgeInsets.only(
            left: onOpenSidebar != null ? OcFloats.gap : OcSpace.s16,
            right: OcFloats.gap,
          ),
          child: Row(
            children: [
              if (onOpenSidebar != null) ...[
                OcIconButton(
                  icon: OcIcons.menu,
                  tooltip: 'Open channels',
                  onPressed: onOpenSidebar,
                ),
                const SizedBox(width: OcSpace.s10),
              ],
              Expanded(
                child: Column(
                  mainAxisAlignment: MainAxisAlignment.center,
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      'Message requests',
                      style: OcText.header.copyWith(
                        fontSize: 15,
                        fontWeight: FontWeight.w600,
                        height: 1.25,
                        color: colors.text,
                      ),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                    ),
                    if (requests.isNotEmpty)
                      Text(
                        'They wrote first. Nothing reaches your chats until you accept.',
                        style: TextStyle(
                          fontSize: 12.5,
                          height: 1.25,
                          color: colors.textSecondary,
                        ),
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                      ),
                  ],
                ),
              ),
            ],
          ),
        ),
        Expanded(
          child: PrivatePage(
            rows: (width) => requests.isEmpty
                ? const [
                    PrivateEmptyState(
                      icon: OcIcons.mail,
                      message: 'No one is waiting for an answer',
                    ),
                  ]
                : _grid(requests, width),
          ),
        ),
      ],
    );
  }

  /// Cards at least 280 wide, as many columns as fit and no more than there
  /// are cards; the cards of a row share its height (§5.16).
  List<Widget> _grid(List<PrivateContact> requests, double width) {
    final fit = ((width + _gap) / (_minCard + _gap)).floor();
    final columns = math.min(requests.length, math.max(1, fit));
    return [
      for (var start = 0; start < requests.length; start += columns)
        Padding(
          padding: EdgeInsets.only(top: start == 0 ? 0 : _gap),
          child: IntrinsicHeight(
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              spacing: _gap,
              children: [
                for (var i = start; i < start + columns; i++)
                  Expanded(
                    child: i < requests.length
                        ? RequestCard(
                            key: ValueKey(requests[i].peer),
                            contact: requests[i],
                          )
                        : const SizedBox(),
                  ),
              ],
            ),
          ),
        ),
    ];
  }
}

/// A message request card in the requests grid (§5.16).
class RequestCard extends ConsumerWidget {
  const RequestCard({super.key, required this.contact});

  final PrivateContact contact;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final name = ref.watch(contactNameProvider(contact.peer));
    final sharedServers = ref.watch(sharedServersProvider(contact.peer));
    final relayKey = contact.relayServerKey;
    final relayServer = relayKey != null
        ? ref.watch(serverProvider(relayKey))
        : null;
    final relayServerName =
        relayServer?.data?.info.name ??
        sharedServers.where((s) => s.serverKey == relayKey).firstOrNull?.name ??
        'Server';

    final now = ref.watch(clockProvider)();
    final timeText = contact.lastActivity != null
        ? rowTime(contact.lastActivity!, now)
        : null;

    final messageText = contact.preview.isNotEmpty
        ? contact.preview
        : 'Wants to message you';

    return Hoverable(
      onTap: () => openContactConversation(context, ref, contact),
      semanticLabel: name,
      focusRadius: BorderRadius.circular(14),
      builder: (context, state) {
        return Container(
          padding: const EdgeInsets.all(12),
          decoration: BoxDecoration(
            color: colors.sidebar,
            borderRadius: BorderRadius.circular(14),
            border: Border.all(color: colors.border, width: 1),
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              // Top row: avatar 40, name 14/600 over via Opencord Dev 12 textMuted, time 11 textMuted
              Row(
                children: [
                  ContactAvatar(peer: contact.peer, size: 40),
                  const SizedBox(width: 10),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          name,
                          style: TextStyle(
                            fontSize: 14,
                            fontWeight: FontWeight.w600,
                            color: colors.text,
                          ),
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                        ),
                        Text(
                          'via $relayServerName',
                          style: TextStyle(
                            fontSize: 12,
                            color: colors.textMuted,
                          ),
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                        ),
                      ],
                    ),
                  ),
                  if (timeText != null) ...[
                    const SizedBox(width: 8),
                    Text(
                      timeText,
                      style: TextStyle(fontSize: 11, color: colors.textMuted),
                    ),
                  ],
                ],
              ),
              const SizedBox(height: 10),

              // Message bubble: padding 8 12, radii 16 16 16 5, bubbleIn, 1 px
              // border; it takes what a taller card in the row leaves over.
              Expanded(
                child: Container(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 12,
                    vertical: 8,
                  ),
                  decoration: BoxDecoration(
                    color: colors.bubbleIn,
                    borderRadius: const BorderRadius.only(
                      topLeft: Radius.circular(16),
                      topRight: Radius.circular(16),
                      bottomRight: Radius.circular(16),
                      bottomLeft: Radius.circular(5),
                    ),
                    border: Border.all(color: colors.border, width: 1),
                  ),
                  child: Text(
                    messageText,
                    style: TextStyle(fontSize: 14, color: colors.text),
                  ),
                ),
              ),
              const SizedBox(height: 10),

              // Buttons, gap 6, equal widths
              Row(
                children: [
                  Expanded(
                    child: PersonActionButton(
                      label: 'Accept',
                      primary: true,
                      onPressed: () => acceptMessageRequestAction(
                        context,
                        ref,
                        contact.peer,
                        name,
                        openChat: true,
                      ),
                    ),
                  ),
                  const SizedBox(width: 6),
                  Expanded(
                    child: PersonActionButton(
                      label: 'Decline',
                      primary: false,
                      onPressed: () => declineMessageRequestAction(
                        context,
                        ref,
                        contact.peer,
                        name,
                      ),
                    ),
                  ),
                ],
              ),
            ],
          ),
        );
      },
    );
  }
}

/// An empty state with icon 32 textMuted, 8, the text textSecondary (§5.16).
class PrivateEmptyState extends StatelessWidget {
  const PrivateEmptyState({
    super.key,
    required this.icon,
    required this.message,
  });

  final IconData icon;
  final String message;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 64),
      child: Center(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(icon, size: 32, color: colors.textMuted),
            const SizedBox(height: 8),
            Text(
              message,
              style: TextStyle(fontSize: 14, color: colors.textSecondary),
              textAlign: TextAlign.center,
            ),
          ],
        ),
      ),
    );
  }
}
