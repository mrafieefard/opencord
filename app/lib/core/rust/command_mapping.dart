import 'package:opencord/core/model/commands.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/src/rust/api/types.dart' as core;

CommandChoice commandChoiceFrom(core.CommandChoice choice) => CommandChoice(
  name: choice.name,
  value: choice.value,
  artworkId: choice.artworkId,
);

/// The core's command list as the app's.
CommandList commandListFrom(core.CommandList list) => CommandList(
  commands: [
    for (final spec in list.commands)
      CommandSpec(
        name: spec.name,
        description: spec.description,
        options: [
          for (final option in spec.options)
            CommandOptionSpec(
              name: option.name,
              description: option.description,
              kind: switch (option.kind) {
                core.CommandOptionKind.text => CommandOptionKind.text,
                core.CommandOptionKind.integer => CommandOptionKind.integer,
                core.CommandOptionKind.boolean => CommandOptionKind.boolean,
                core.CommandOptionKind.choice => CommandOptionKind.choice,
                core.CommandOptionKind.user => CommandOptionKind.user,
                core.CommandOptionKind.channel => CommandOptionKind.channel,
              },
              required: option.required_,
              autocomplete: option.autocomplete,
              choices: [
                for (final choice in option.choices) commandChoiceFrom(choice),
              ],
              min: option.min.toInt(),
              max: option.max.toInt(),
            ),
        ],
        feature: spec.feature,
        permissionSummary: spec.permissionSummary,
        aliases: spec.aliases,
        permissions: Permissions(spec.permissions),
        enabled: spec.enabled,
        channelIds: [for (final id in spec.channelIds) id.toInt()],
      ),
  ],
  resultsPublic: list.resultsPublic,
);
