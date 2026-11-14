return {
  id = "tickets",
  name = "Tickets",
  version = "1.0.0",
  author = "Opencord",
  description = "Open and track support tickets.",
  license = "MIT",
  api = 1,

  permissions = {
    "create_channel_types",
    "send_messages",
    "read_roles",
  },

  config = {
    { key = "support_role", label = "Support role", kind = "role" },
    { key = "log_channel", label = "Log channel", kind = "channel" },
  },

  server = { "server/main.luau" },
  client = "client/main.luau",
}
