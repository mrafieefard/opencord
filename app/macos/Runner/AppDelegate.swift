import Cocoa
import FlutterMacOS

@main
class AppDelegate: FlutterAppDelegate {
  override func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
    return true
  }

  override func applicationSupportsSecureRestorableState(_ app: NSApplication) -> Bool {
    return true
  }

  /// opencord:// links from the browser or the Finder (desktop UI plan
  /// §15). macOS keeps one instance, so they all arrive here.
  override func application(_ application: NSApplication, open urls: [URL]) {
    for url in urls where url.scheme == "opencord" {
      (mainFlutterWindow as? MainFlutterWindow)?.openLink(url.absoluteString)
    }
  }

  /// A click on the Dock icon brings back a window hidden to the menu bar.
  override func applicationShouldHandleReopen(
    _ sender: NSApplication, hasVisibleWindows flag: Bool
  ) -> Bool {
    if !flag { mainFlutterWindow?.makeKeyAndOrderFront(nil) }
    return true
  }
}
