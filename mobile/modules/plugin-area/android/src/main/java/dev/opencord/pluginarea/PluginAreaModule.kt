package dev.opencord.pluginarea

import android.content.Context
import expo.modules.kotlin.Promise
import expo.modules.kotlin.exception.CodedException
import expo.modules.kotlin.exception.Exceptions
import expo.modules.kotlin.functions.Queues
import expo.modules.kotlin.modules.Module
import expo.modules.kotlin.modules.ModuleDefinition
import kotlinx.coroutines.launch

class PluginAreaModule : Module() {
  // Areas live on the main thread. The synchronous calls go through the
  // same queue as the Queues.MAIN async ones, so all keep the order
  // JavaScript made them in.
  private fun onMain(work: () -> Unit) {
    appContext.mainQueue.launch { work() }
  }

  private var ownEvents: AreaEvents? = null

  private val context: Context
    get() = appContext.reactContext ?: throw Exceptions.ReactContextLost()

  override fun definition() = ModuleDefinition {
    Name("PluginArea")

    Events("input", "status", "hostCall")

    OnCreate {
      val events: AreaEvents = { name, body -> sendEvent(name, body) }
      ownEvents = events
      Areas.events = events
    }

    OnDestroy {
      // A reloaded JavaScript's module may already have set its own.
      if (Areas.events === ownEvents) Areas.events = null
    }

    Function("warmUp") { onMain { Areas.warmUp(context) } }

    Function("coolDown") { onMain { Areas.coolDown() } }

    AsyncFunction("createArea") { areaId: String, setupJson: String, promise: Promise ->
      Areas.create(context, areaId, setupJson).whenReady { error ->
        if (error == null) promise.resolve(null) else promise.reject(CodedException("ERR_AREA_GONE", "the area was destroyed before it started", error))
      }
    }.runOnQueue(Queues.MAIN)

    Function("destroyArea") { areaId: String -> onMain { Areas.destroy(areaId) } }

    Function("destroyAllAreas") { onMain { Areas.destroyAll() } }

    Function("setSetup") { areaId: String, setupJson: String -> onMain { Areas[areaId]?.pipe?.setup(setupJson) } }

    Function("pushBatch") { areaId: String, json: String -> onMain { Areas[areaId]?.pipe?.push(json) } }

    Function("hostReply") { areaId: String, id: Int, json: String -> onMain { Areas[areaId]?.hostReply(id, json) } }

    AsyncFunction("back") { areaId: String, promise: Promise ->
      val area = Areas[areaId]
      if (area == null) promise.resolve(false) else area.back { promise.resolve(it) }
    }.runOnQueue(Queues.MAIN)

    View(PluginAreaView::class) {
      Prop("areaId") { view: PluginAreaView, areaId: String -> view.setAreaId(areaId) }
    }
  }
}
