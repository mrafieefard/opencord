package dev.opencord.pluginarea

import android.content.Context
import android.os.Handler
import android.os.Looper
import expo.modules.kotlin.Promise
import expo.modules.kotlin.exception.CodedException
import expo.modules.kotlin.exception.Exceptions
import expo.modules.kotlin.functions.Queues
import expo.modules.kotlin.modules.Module
import expo.modules.kotlin.modules.ModuleDefinition

class PluginAreaModule : Module() {
  // Areas live on the main thread. The synchronous calls post there in the
  // order JavaScript made them, behind any createArea already posted.
  private val main = Handler(Looper.getMainLooper())

  private fun onMain(work: () -> Unit) {
    main.post(work)
  }

  private val context: Context
    get() = appContext.reactContext ?: throw Exceptions.ReactContextLost()

  override fun definition() = ModuleDefinition {
    Name("PluginArea")

    Events("input", "status", "hostCall")

    OnCreate {
      Areas.events = { name, body -> sendEvent(name, body) }
    }

    OnDestroy {
      Areas.events = null
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
