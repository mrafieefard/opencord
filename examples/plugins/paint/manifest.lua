return {
  id = "paint",
  name = "Paint",
  version = "1.0.0",
  author = "Opencord",
  description = "A shared whiteboard for everyone in a call.",
  license = "MIT",
  api = 1,

  permissions = {
    "register_activities",
  },

  server = { "server/main.luau" },
  client = "client/main.luau",
}
