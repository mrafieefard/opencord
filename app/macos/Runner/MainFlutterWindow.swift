import Cocoa
import FlutterMacOS

class MainFlutterWindow: NSWindow {
  private var windowChannel: WindowChannel?

  override func awakeFromNib() {
    let flutterViewController = FlutterViewController()
    let windowFrame = self.frame
    self.contentViewController = flutterViewController
    self.setFrame(windowFrame, display: true)

    RegisterGeneratedPlugins(registry: flutterViewController)
    windowChannel = WindowChannel(
      window: self, messenger: flutterViewController.engine.binaryMessenger)

    super.awakeFromNib()
  }

  func openLink(_ link: String) {
    windowChannel?.openLink(link)
  }
}

/// The "dev.opencord/window" method channel (desktop UI plan §3.1): a
/// transparent, full-size title bar with the native traffic lights moved to
/// a 52 pt band above the server rail. The app's header row moves the
/// window through `performDrag`.
final class WindowChannel: NSObject, NSWindowDelegate {
  /// The traffic lights' band: as tall as the rail's top spacer, as wide as
  /// the rail, so the rest of the header row keeps its clicks.
  private let bandHeight: CGFloat = 52
  private let bandWidth: CGFloat = 76
  private let lightsInset: CGFloat = 20

  private weak var window: NSWindow?
  private let channel: FlutterMethodChannel
  private var custom = true
  private var interceptClose = false
  /// Links that arrived before the app configured the window, which is
  /// when it starts listening.
  private var configured = false
  private var pendingLinks: [String] = []

  init(window: NSWindow, messenger: FlutterBinaryMessenger) {
    self.window = window
    channel = FlutterMethodChannel(
      name: "dev.opencord/window", binaryMessenger: messenger)
    super.init()
    window.delegate = self
    channel.setMethodCallHandler { [weak self] call, result in
      self?.handle(call, result: result)
    }
    applyChrome()
  }

  private func applyChrome() {
    guard let window else { return }
    if custom {
      window.titleVisibility = .hidden
      window.titlebarAppearsTransparent = true
      window.styleMask.insert(.fullSizeContentView)
    } else {
      window.titleVisibility = .visible
      window.titlebarAppearsTransparent = false
      window.styleMask.remove(.fullSizeContentView)
    }
    placeTrafficLights()
  }

  /// Centers the traffic lights in the band above the rail, inset 20 pt
  /// from the left. The system resets this on some changes, so it runs
  /// again after resizes and full-screen transitions.
  private func placeTrafficLights() {
    guard let window, custom, !window.styleMask.contains(.fullScreen),
      let close = window.standardWindowButton(.closeButton),
      let miniaturize = window.standardWindowButton(.miniaturizeButton),
      let zoom = window.standardWindowButton(.zoomButton),
      let container = close.superview?.superview
    else { return }
    let spacing = miniaturize.frame.origin.x - close.frame.origin.x
    container.frame = NSRect(
      x: 0, y: window.frame.height - bandHeight, width: bandWidth,
      height: bandHeight)
    for (index, button) in [close, miniaturize, zoom].enumerated() {
      button.setFrameOrigin(
        NSPoint(
          x: lightsInset + CGFloat(index) * spacing,
          y: (bandHeight - button.frame.height) / 2))
    }
  }

  private func status() -> [String: Any] {
    guard let window else { return [:] }
    return [
      "maximized": window.isZoomed,
      "fullscreen": window.styleMask.contains(.fullScreen),
      "tiled": false,
      "focused": window.isKeyWindow,
    ]
  }

  private func sendStatus() {
    channel.invokeMethod("status", arguments: status())
  }

  private func sendGeometry() {
    guard let window, !window.isZoomed,
      !window.styleMask.contains(.fullScreen)
    else { return }
    let frame = window.frame
    channel.invokeMethod(
      "geometry",
      arguments: [
        "width": Int(frame.width), "height": Int(frame.height),
        "x": Int(frame.minX), "y": Int(frame.minY),
      ])
  }

  private func configure(_ args: [String: Any]) -> [String: Any] {
    guard let window else { return [:] }
    custom = (args["chrome"] as? String) != "system"
    interceptClose = args["interceptClose"] as? Bool ?? false
    let minWidth = args["minWidth"] as? Int ?? 940
    let minHeight = args["minHeight"] as? Int ?? 560
    window.contentMinSize = NSSize(width: minWidth, height: minHeight)
    applyChrome()

    let width = CGFloat(args["width"] as? Int ?? 1280)
    let height = CGFloat(args["height"] as? Int ?? 800)
    var frame = NSRect(x: 0, y: 0, width: width, height: height)
    if let x = args["x"] as? Int, let y = args["y"] as? Int {
      frame.origin = NSPoint(x: x, y: y)
    }
    // A position on a screen that is gone falls back to the main screen.
    let visible = NSScreen.screens.contains { $0.visibleFrame.intersects(frame) }
    if !visible || args["x"] == nil {
      let screen = NSScreen.main?.visibleFrame ?? .zero
      frame.origin = NSPoint(
        x: screen.midX - width / 2, y: screen.midY - height / 2)
    }
    window.setFrame(frame, display: true)
    if args["maximized"] as? Bool ?? false, !window.isZoomed {
      window.zoom(nil)
    }
    return [
      "chrome": custom ? "custom" : "system",
      "transparent": false,
      "frameMargin": 0,
      "buttonLayout": "",
      "status": status(),
    ]
  }

  /// Brings the window forward and passes the link to the app (§15).
  func openLink(_ link: String) {
    window?.makeKeyAndOrderFront(nil)
    NSApp.activate(ignoringOtherApps: true)
    if configured {
      channel.invokeMethod("openLink", arguments: link)
    } else {
      pendingLinks.append(link)
    }
  }

  /// A double click on the title bar does what System Settings says:
  /// zoom, minimize or nothing.
  private func titleBarDoubleClick() {
    guard let window else { return }
    let action = UserDefaults.standard.string(forKey: "AppleActionOnDoubleClick")
    switch action {
    case "Minimize": window.miniaturize(nil)
    case "None": break
    default: window.zoom(nil)
    }
  }

  private func handle(_ call: FlutterMethodCall, result: FlutterResult) {
    guard let window else {
      result(nil)
      return
    }
    switch call.method {
    case "configure":
      result(configure(call.arguments as? [String: Any] ?? [:]))
      configured = true
      for link in pendingLinks {
        channel.invokeMethod("openLink", arguments: link)
      }
      pendingLinks.removeAll()
      return
    case "setTitle":
      if let title = call.arguments as? String { window.title = title }
    case "startDrag":
      if let event = NSApp.currentEvent { window.performDrag(with: event) }
    case "toggleMaximize":
      titleBarDoubleClick()
    case "minimize":
      window.miniaturize(nil)
    case "setFullscreen":
      let wanted = call.arguments as? Bool ?? false
      if wanted != window.styleMask.contains(.fullScreen) {
        window.toggleFullScreen(nil)
      }
    case "close":
      window.performClose(nil)
    case "quit":
      interceptClose = false
      NSApp.terminate(nil)
    case "hide":
      window.orderOut(nil)
    case "show":
      window.makeKeyAndOrderFront(nil)
      NSApp.activate(ignoringOtherApps: true)
    case "setUrgent":
      if call.arguments as? Bool ?? false {
        NSApp.requestUserAttention(.informationalRequest)
      }
    case "status":
      result(status())
      return
    case "showWindowMenu", "startResize":
      // macOS has neither a title bar menu nor custom resize handles.
      break
    default:
      result(FlutterMethodNotImplemented)
      return
    }
    result(nil)
  }

  // NSWindowDelegate

  func windowShouldClose(_ sender: NSWindow) -> Bool {
    if interceptClose {
      channel.invokeMethod("closeRequested", arguments: nil)
      return false
    }
    return true
  }

  func windowDidResize(_ notification: Notification) {
    placeTrafficLights()
    sendStatus()
    sendGeometry()
  }

  func windowDidMove(_ notification: Notification) {
    sendGeometry()
  }

  func windowDidEnterFullScreen(_ notification: Notification) {
    sendStatus()
  }

  func windowDidExitFullScreen(_ notification: Notification) {
    placeTrafficLights()
    sendStatus()
  }

  func windowDidBecomeKey(_ notification: Notification) {
    placeTrafficLights()
    sendStatus()
  }

  func windowDidResignKey(_ notification: Notification) {
    sendStatus()
  }
}
