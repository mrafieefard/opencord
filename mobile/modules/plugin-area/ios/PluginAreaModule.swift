import ExpoModulesCore

/// Plugin areas on iOS (plugins M4 plan, Task 5), as `PluginAreaModule.kt`.
public class PluginAreaModule: Module {
  public func definition() -> ModuleDefinition {
    Name("PluginArea")

    Events("input", "status", "hostCall")

    OnCreate {
      Areas.events = { [weak self] name, body in
        self?.sendEvent(name, body as [String: Any?])
      }
    }

    OnDestroy {
      Areas.events = nil
    }

    Function("warmUp") { Areas.warmUp() }.runOnQueue(.main)

    Function("coolDown") { Areas.coolDown() }.runOnQueue(.main)

    AsyncFunction("createArea") { (areaId: String, setupJson: String, promise: Promise) in
      Areas.create(id: areaId, setupJson: setupJson).whenReady { error in
        if error == nil {
          promise.resolve(nil)
        } else {
          promise.reject("ERR_AREA_GONE", "the area was destroyed before it started")
        }
      }
    }.runOnQueue(.main)

    Function("destroyArea") { (areaId: String) in Areas.destroy(id: areaId) }.runOnQueue(.main)

    Function("destroyAllAreas") { Areas.destroyAll() }.runOnQueue(.main)

    Function("setSetup") { (areaId: String, setupJson: String) in
      Areas[areaId]?.pipe.setup(setupJson)
    }.runOnQueue(.main)

    Function("pushBatch") { (areaId: String, json: String) in
      Areas[areaId]?.pipe.push(json)
    }.runOnQueue(.main)

    Function("hostReply") { (areaId: String, id: Int, json: String) in
      Areas[areaId]?.hostReply(id: id, json: json)
    }.runOnQueue(.main)

    AsyncFunction("back") { (areaId: String, promise: Promise) in
      guard let area = Areas[areaId] else {
        promise.resolve(false)
        return
      }
      area.back { promise.resolve($0) }
    }.runOnQueue(.main)

    View(PluginAreaView.self) {
      Prop("areaId") { (view: PluginAreaView, areaId: String) in
        view.setAreaId(areaId)
      }
    }
  }
}
