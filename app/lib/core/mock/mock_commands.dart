/// The server's music commands in the mock repository (Phase 3 plan §17.2),
/// as the server describes them.
library;

import 'package:opencord/core/model/commands.dart';
import 'package:opencord/core/model/permissions.dart';

const _channel = CommandOptionSpec(
  name: 'channel',
  description: 'Another voice channel (Manage music)',
  kind: CommandOptionKind.channel,
);

CommandSpec _music(
  String name,
  String description,
  Permissions permissions,
  String summary, {
  List<CommandOptionSpec> options = const [],
  List<String> aliases = const [],
}) => CommandSpec(
  name: name,
  description: description,
  options: [...options, _channel],
  feature: 'music',
  permissionSummary: summary,
  permissions: permissions,
  aliases: aliases,
);

final mockCommandSpecs = [
  _music(
    'play',
    'Play a song, or add it to the queue',
    Permissions.musicAdd,
    'Add music; next needs Manage the queue',
    options: const [
      CommandOptionSpec(
        name: 'query',
        description: 'Words or a link',
        kind: CommandOptionKind.text,
        required: true,
        autocomplete: true,
      ),
      CommandOptionSpec(
        name: 'next',
        description: 'Play it next',
        kind: CommandOptionKind.boolean,
      ),
    ],
  ),
  _music(
    'pause',
    'Pause the music',
    Permissions.musicControl,
    'Control playback',
  ),
  _music(
    'resume',
    'Play the music again',
    Permissions.musicControl,
    'Control playback',
  ),
  _music(
    'skip',
    'Skip the track that plays',
    Permissions.musicAdd,
    'Control playback, or Add music for your own track',
  ),
  _music(
    'previous',
    'Play the track before again',
    Permissions.musicControl,
    'Control playback',
  ),
  _music(
    'seek',
    'Go to a time in the track',
    Permissions.musicControl,
    'Control playback',
    options: const [
      CommandOptionSpec(
        name: 'time',
        description: 'Where, like 1:30',
        kind: CommandOptionKind.text,
        required: true,
      ),
    ],
  ),
  _music(
    'stop',
    'Stop the music and clear the queue',
    Permissions.musicManage,
    'Manage music',
  ),
  _music(
    'queue',
    'Show what comes next, only to you',
    Permissions.none,
    'Anyone who can see the channel',
    options: const [
      CommandOptionSpec(
        name: 'page',
        description: 'Which page',
        kind: CommandOptionKind.integer,
        min: 1,
        max: 50,
      ),
    ],
  ),
  _music(
    'nowplaying',
    'Show what plays, only to you',
    Permissions.none,
    'Anyone who can see the channel',
    aliases: const ['np'],
  ),
  _music(
    'remove',
    'Take a track out of the queue',
    Permissions.musicAdd,
    'Add music for your own tracks; others need Manage the queue',
    options: const [
      CommandOptionSpec(
        name: 'position',
        description: 'Its place in Up next',
        kind: CommandOptionKind.integer,
        required: true,
        min: 1,
        max: 500,
      ),
    ],
  ),
  _music(
    'move',
    'Move a track in the queue',
    Permissions.musicManageQueue,
    'Manage the queue',
    options: const [
      CommandOptionSpec(
        name: 'from',
        description: 'Its place now',
        kind: CommandOptionKind.integer,
        required: true,
        min: 1,
        max: 500,
      ),
      CommandOptionSpec(
        name: 'to',
        description: 'Its new place',
        kind: CommandOptionKind.integer,
        required: true,
        min: 1,
        max: 500,
      ),
    ],
  ),
  _music(
    'clear',
    'Clear the queue',
    Permissions.musicManageQueue,
    'Manage the queue',
  ),
  _music(
    'shuffle',
    'Turn shuffle on or off',
    Permissions.musicControl,
    'Control playback',
  ),
  _music(
    'repeat',
    'Repeat nothing, the queue or the track',
    Permissions.musicControl,
    'Control playback',
    options: const [
      CommandOptionSpec(
        name: 'mode',
        description: 'Off, the queue or the track',
        kind: CommandOptionKind.choice,
        required: true,
        choices: [
          CommandChoice(name: 'Off', value: 'off'),
          CommandChoice(name: 'Queue', value: 'queue'),
          CommandChoice(name: 'Track', value: 'track'),
        ],
      ),
    ],
  ),
];
