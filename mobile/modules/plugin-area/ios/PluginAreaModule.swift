import ExpoModulesCore

/// Plugin areas on iOS (plugins M4 plan, Task 5), as `PluginAreaModule.kt`.
public class PluginAreaModule: Module {
  public func definition() -> ModuleDefinition {
    Name("PluginArea")

    Events("input", "status", "hostCall")

    OnCreate {
      Areas.eventsOwner = self
      Areas.events = { [weak self] name, body in
        self?.sendEvent(name, body as [String: Any?])
      }
    }

    OnDestroy {
      // A reloaded JavaScript's module may already have set its own.
      if Areas.eventsOwner === self { Areas.events = nil }
    }

    Function("warmUp") { onMain { Areas.warmUp() } }

    Function("coolDown") { onMain { Areas.coolDown() } }

    AsyncFunction("createArea") { (areaId: String, setupJson: String, promise: Promise) in
      Areas.create(id: areaId, setupJson: setupJson).whenReady { error in
        if error == nil {
          promise.resolve(nil)
        } else {
          promise.reject("ERR_AREA_GONE", "the area was destroyed before it started")
        }
      }
    }.runOnQueue(.main)

    Function("destroyArea") { (areaId: String) in onMain { Areas.destroy(id: areaId) } }

    Function("destroyAllAreas") { onMain { Areas.destroyAll() } }

    Function("setSetup") { (areaId: String, setupJson: String) in
      onMain { Areas[areaId]?.pipe.setup(setupJson) }
    }

    Function("pushBatch") { (areaId: String, json: String) in
      onMain { Areas[areaId]?.pipe.push(json) }
    }

    Function("hostReply") { (areaId: String, id: Int, json: String) in
      onMain { Areas[areaId]?.hostReply(id: id, json: json) }
    }

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

/// Runs `work` on the main queue after what is already there, so the
/// synchronous calls keep their order with the `.main` async ones.
private func onMain(_ work: @escaping () -> Void) {
  DispatchQueue.main.async(execute: work)
}
