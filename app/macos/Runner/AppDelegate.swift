import Cocoa
import FlutterMacOS

@main
class AppDelegate: FlutterAppDelegate {
  /// The app stays in the Dock when its window closes (§7): red hides the
  /// window, and AppKit counts a hidden last window as closed, so `true`
  /// here quit the app instead. ⌘Q and the menu bar's Quit end it.
  override func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
    return false
  }

  override func applicationSupportsSecureRestorableState(_ app: NSApplication) -> Bool {
    return true
  }

  /// opencord:// and opencords:// links from the browser or the Finder (desktop UI plan
  /// §15). macOS keeps one instance, so they all arrive here.
  override func application(_ application: NSApplication, open urls: [URL]) {
    for url in urls where url.scheme == "opencord" || url.scheme == "opencords" {
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
