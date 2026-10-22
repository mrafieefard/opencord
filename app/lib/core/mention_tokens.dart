// Mention tokens in message content, `<@user id>` and `<#channel id>` (a
// client convention, D18). Anyone can send any digits: ones that do not fit
// a 64-bit id are not a mention and stay as typed.

final userMentionToken = RegExp(r'<@(\d+)>');
final channelMentionToken = RegExp(r'<#(\d+)>');

/// The id a token's digits name, or null when they cannot be one.
int? mentionId(String digits) => int.tryParse(digits);

/// [content] with each mention token replaced by what [user] or
/// [channel] make of its id and the token itself.
String replaceMentionTokens(
  String content, {
  required String Function(int id, String token) user,
  required String Function(int id, String token) channel,
}) => content
    .replaceAllMapped(
      userMentionToken,
      (m) => switch (mentionId(m[1]!)) {
        final id? => user(id, m[0]!),
        null => m[0]!,
      },
    )
    .replaceAllMapped(
      channelMentionToken,
      (m) => switch (mentionId(m[1]!)) {
        final id? => channel(id, m[0]!),
        null => m[0]!,
      },
    );
