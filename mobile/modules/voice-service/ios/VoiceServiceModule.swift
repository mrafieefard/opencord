import ExpoModulesCore

// iOS keeps a call going with the `audio` background mode (app.config.ts),
// so there is no service to run.
public class VoiceServiceModule: Module {
  public func definition() -> ModuleDefinition {
    Name("VoiceService")

    Function("start") { (_: String, _: String) in }

    Function("stop") {}
  }
}
