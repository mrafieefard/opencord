import Cocoa
import FlutterMacOS
import ServiceManagement

class MainFlutterWindow: NSWindow {
  private var windowChannel: WindowChannel?
  private var trayChannel: TrayChannel?

  override func awakeFromNib() {
    let flutterViewController = FlutterViewController()
    let windowFrame = self.frame
    self.contentViewController = flutterViewController
    self.setFrame(windowFrame, display: true)

    RegisterGeneratedPlugins(registry: flutterViewController)
    windowChannel = WindowChannel(
      window: self, messenger: flutterViewController.engine.binaryMessenger)
    trayChannel = TrayChannel(
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
    // Started minimized; the Dock icon brings the window back.
    if args["hidden"] as? Bool ?? false {
      window.orderOut(nil)
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
    case "setLaunchAtLogin":
      let enabled = call.arguments as? Bool ?? false
      // Installed by the launcher (docs/updater.md): login starts the
      // launcher, so it updates first. This version's bundle goes with the
      // next update, so it can't be the login item.
      if let launcher = ProcessInfo.processInfo.environment["OPENCORD_LAUNCHER"],
        !launcher.isEmpty
      {
        setLaunchAgent(enabled: enabled, launcher: launcher)
      } else if #available(macOS 13.0, *) {
        // A login item (desktop UI plan §15); macOS 13 and later.
        do {
          if enabled {
            try SMAppService.mainApp.register()
          } else {
            try SMAppService.mainApp.unregister()
          }
        } catch {
          NSLog("Opencord could not change its login item: \(error)")
        }
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

/// `~/Library/LaunchAgents/dev.opencord.opencord.plist`, starting the
/// launcher at login with `--autostart`. A login item would name this
/// version's bundle, which the next update removes.
private func setLaunchAgent(enabled: Bool, launcher: String) {
  // The real home: inside the sandbox, the usual calls name the app's
  // container. The entitlements let the app write this one folder there.
  guard let home = getpwuid(getuid())?.pointee.pw_dir else { return }
  let agents = URL(fileURLWithPath: String(cString: home))
    .appendingPathComponent("Library/LaunchAgents")
  let plist = agents.appendingPathComponent("dev.opencord.opencord.plist")
  guard enabled else {
    try? FileManager.default.removeItem(at: plist)
    return
  }
  let agent: [String: Any] = [
    "Label": "dev.opencord.opencord",
    "ProgramArguments": [launcher, "--autostart"],
    "RunAtLoad": true,
    "ProcessType": "Interactive",
  ]
  do {
    try FileManager.default.createDirectory(at: agents, withIntermediateDirectories: true)
    let data = try PropertyListSerialization.data(
      fromPropertyList: agent, format: .xml, options: 0)
    try data.write(to: plist, options: .atomic)
  } catch {
    NSLog("Opencord could not write its launch agent: \(error)")
  }
}

/// The "dev.opencord/tray" method channel (desktop UI plan §15): the menu
/// bar icon and its menu. The app draws the icon (a black template image,
/// which macOS tints) and lays out the menu; a click brings the window
/// forward and sends "activate", a right click shows the menu, and a
/// picked item sends "menu" with its id.
final class TrayChannel: NSObject {
  /// The menu bar's height, in points.
  private static let side: CGFloat = 18

  private weak var window: NSWindow?
  private let channel: FlutterMethodChannel
  private var item: NSStatusItem?
  private var menu = NSMenu()

  init(window: NSWindow, messenger: FlutterBinaryMessenger) {
    self.window = window
    channel = FlutterMethodChannel(name: "dev.opencord/tray", binaryMessenger: messenger)
    super.init()
    channel.setMethodCallHandler { [weak self] call, result in
      self?.handle(call, result: result)
    }
  }

  private func handle(_ call: FlutterMethodCall, result: FlutterResult) {
    switch call.method {
    case "show":
      guard let args = call.arguments as? [String: Any] else {
        result(false)
        return
      }
      result(show(args))
    case "close":
      if let item { NSStatusBar.system.removeStatusItem(item) }
      item = nil
      result(nil)
    default:
      result(FlutterMethodNotImplemented)
    }
  }

  private func show(_ args: [String: Any]) -> Bool {
    let item =
      self.item
      ?? {
        let created = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
        created.button?.target = self
        created.button?.action = #selector(clicked(_:))
        created.button?.sendAction(on: [.leftMouseUp, .rightMouseUp])
        return created
      }()
    self.item = item
    if let icons = args["icons"] as? [[String: Any]],
      let image = Self.image(from: icons, template: args["template"] as? Bool ?? true)
    {
      item.button?.image = image
    }
    item.button?.toolTip = args["tooltip"] as? String
    menu = makeMenu(args["menu"] as? [[String: Any]] ?? [])
    return item.button?.image != nil
  }

  @objc private func clicked(_ sender: NSStatusBarButton) {
    let event = NSApp.currentEvent
    if event?.type == .rightMouseUp || event?.modifierFlags.contains(.control) == true {
      menu.popUp(
        positioning: nil, at: NSPoint(x: 0, y: sender.bounds.height + 4), in: sender)
      return
    }
    window?.makeKeyAndOrderFront(nil)
    NSApp.activate(ignoringOtherApps: true)
    channel.invokeMethod("activate", arguments: nil)
  }

  @objc private func picked(_ sender: NSMenuItem) {
    channel.invokeMethod("menu", arguments: sender.tag)
  }

  private func makeMenu(_ items: [[String: Any]]) -> NSMenu {
    let built = NSMenu()
    built.autoenablesItems = false
    for entry in items {
      if entry["separator"] as? Bool == true {
        built.addItem(.separator())
        continue
      }
      let item = NSMenuItem(
        title: entry["label"] as? String ?? "", action: #selector(picked(_:)),
        keyEquivalent: "")
      item.target = self
      item.tag = (entry["id"] as? NSNumber)?.intValue ?? 0
      if let checked = entry["checked"] as? Bool {
        item.state = checked ? .on : .off
      }
      if let children = entry["children"] as? [[String: Any]], !children.isEmpty {
        item.action = nil
        item.submenu = makeMenu(children)
      }
      built.addItem(item)
    }
    return built
  }

  /// The app's straight RGBA pixels, one representation per size.
  private static func image(from icons: [[String: Any]], template: Bool) -> NSImage? {
    let image = NSImage(size: NSSize(width: side, height: side))
    for icon in icons {
      guard let size = (icon["size"] as? NSNumber)?.intValue, size > 0,
        let data = (icon["rgba"] as? FlutterStandardTypedData)?.data,
        data.count == size * size * 4,
        let rep = NSBitmapImageRep(
          bitmapDataPlanes: nil, pixelsWide: size, pixelsHigh: size, bitsPerSample: 8,
          samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
          bitmapFormat: .alphaNonpremultiplied, bytesPerRow: size * 4, bitsPerPixel: 32),
        let pixels = rep.bitmapData
      else { continue }
      data.copyBytes(to: pixels, count: data.count)
      rep.size = NSSize(width: side, height: side)
      image.addRepresentation(rep)
    }
    image.isTemplate = template
    return image.representations.isEmpty ? nil : image
  }
}
