import ExpoModulesCore

// iOS keeps a call going with the `audio` background mode (app.config.ts),
// so there is no service to run. The module also gives the call's output
// button (AudioRouteButtonView).
public class VoiceServiceModule: Module {
  public func definition() -> ModuleDefinition {
    Name("VoiceService")

    Function("start") { (_: String, _: String) in }

    Function("stop") {}

    View(AudioRouteButtonView.self) {
      Prop("tint") { (view: AudioRouteButtonView, tint: UIColor?) in
        view.tint = tint ?? .label
      }
    }
  }
}
