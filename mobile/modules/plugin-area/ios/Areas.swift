import Flutter
import Foundation

/// Where an area's inputs, status and host calls go: the module's events.
typealias AreaEvents = (_ name: String, _ body: [String: Any?]) -> Void

/// What goes to one area's Dart side, in order: the setup, then the
/// batches, held until Dart says `ready` (as `AreaPipe.kt`).
final class AreaPipe {
  private var held: [String] = []
  private var setupJson: String?
  private(set) var isReady = false
  private let send: (_ method: String, _ json: String?) -> Void

  init(send: @escaping (_ method: String, _ json: String?) -> Void) {
    self.send = send
  }

  func setup(_ json: String) {
    setupJson = json
    if isReady { send("setup", json) }
  }

  func push(_ json: String) {
    if isReady { send("batch", json) } else { held.append(json) }
  }

  func ready() {
    isReady = true
    if let setupJson { send("setup", setupJson) }
    let waiting = held
    held.removeAll()
    waiting.forEach { send("batch", $0) }
  }
}

/// One area: its engine, its channel and what waits for Dart. Main thread
/// only (as `Area` in `Areas.kt`).
final class Area {
  static let channelName = "dev.opencord/plugin_area"

  let engine: FlutterEngine
  var id: String?
  weak var view: PluginAreaView?
  private let channel: FlutterMethodChannel
  private var readyWaiters: [(Error?) -> Void] = []
  private let events: () -> AreaEvents?
  private(set) lazy var pipe = AreaPipe { [weak self] method, json in
    self?.channel.invokeMethod(method, arguments: json.map { ["json": $0] })
  }

  init(engine: FlutterEngine, events: @escaping () -> AreaEvents?) {
    self.engine = engine
    self.events = events
    channel = FlutterMethodChannel(name: Area.channelName, binaryMessenger: engine.binaryMessenger)
    channel.setMethodCallHandler { [weak self] call, result in
      self?.handle(call)
      result(nil)
    }
  }

  private func handle(_ call: FlutterMethodCall) {
    let arguments = call.arguments as? [String: Any]
    let json = arguments?["json"] as? String
    switch call.method {
    case "ready":
      pipe.ready()
      let waiting = readyWaiters
      readyWaiters.removeAll()
      waiting.forEach { $0(nil) }
    case "input", "status":
      if let id, let json { events()?(call.method, ["areaId": id, "json": json]) }
    case "hostCall":
      if let id {
        events()?("hostCall", [
          "areaId": id,
          "id": arguments?["id"] as? Int,
          "method": arguments?["method"] as? String,
          "json": json ?? "null",
        ])
      }
    default:
      break
    }
  }

  /// Runs [done] once Dart is listening: at once when it already is.
  func whenReady(_ done: @escaping (Error?) -> Void) {
    if pipe.isReady { done(nil) } else { readyWaiters.append(done) }
  }

  func hostReply(id: Int, json: String) {
    channel.invokeMethod("hostReply", arguments: ["id": id, "json": json])
  }

  func back(_ answer: @escaping (Bool) -> Void) {
    channel.invokeMethod("back", arguments: nil) { result in
      answer((result as? Bool) == true)
    }
  }

  func destroy() {
    view?.detach()
    view = nil
    let waiting = readyWaiters
    readyWaiters.removeAll()
    waiting.forEach { $0(AreaGone()) }
    channel.setMethodCallHandler(nil)
    engine.destroyContext()
  }
}

struct AreaGone: Error {}

/// Every area in the process: one engine group, a spare engine after
/// `warmUp`, and the areas by id. Main thread only.
enum Areas {
  private static var group: FlutterEngineGroup?
  private static var spare: Area?
  private static var areas: [String: Area] = [:]
  static var events: AreaEvents?

  private static func newArea() -> Area {
    let engineGroup = group ?? FlutterEngineGroup(name: "plugin-areas", project: nil)
    group = engineGroup
    let engine = engineGroup.makeEngine(withEntrypoint: nil, libraryURI: nil)
    return Area(engine: engine) { events }
  }

  static func warmUp() {
    if spare == nil { spare = newArea() }
  }

  static func coolDown() {
    spare?.destroy()
    spare = nil
  }

  static func create(id: String, setupJson: String) -> Area {
    areas.removeValue(forKey: id)?.destroy()
    let area = spare ?? newArea()
    spare = nil
    area.id = id
    area.pipe.setup(setupJson)
    areas[id] = area
    return area
  }

  static subscript(id: String) -> Area? { areas[id] }

  static func destroy(id: String) {
    areas.removeValue(forKey: id)?.destroy()
  }

  static func destroyAll() {
    areas.values.forEach { $0.destroy() }
    areas.removeAll()
  }
}
