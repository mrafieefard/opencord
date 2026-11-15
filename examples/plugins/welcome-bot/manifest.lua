return {
  id = "welcome-bot",
  name = "Welcome Bot",
  version = "1.0.0",
  author = "Opencord",
  description = "Greets new members in a welcome channel, deletes messages with banned words, and rolls dice.",
  license = "MIT",
  api = 1,

  permissions = {
    "read_members",
    "send_messages",
    "read_messages",
    "manage_messages",
    "register_commands",
  },

  config = {
    { key = "welcome_channel", label = "Welcome channel", kind = "channel", required = true },
    { key = "banned_words", label = "Banned words", kind = "text" },
  },

  server = { "server/main.luau" },
}
