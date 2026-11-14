package dev.opencord.pluginarea

import android.app.Activity
import android.app.Application
import android.content.Context
import android.os.Bundle
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.embedding.engine.FlutterEngineGroup
import io.flutter.plugin.common.MethodCall
import io.flutter.plugin.common.MethodChannel

enum class EngineState { RESUMED, PAUSED }

/**
 * What an area's engine is told: it runs (ticks, animates, draws) only while
 * a view shows it and the app is in the foreground.
 */
fun engineState(hasView: Boolean, inBackground: Boolean): EngineState =
  if (hasView && !inBackground) EngineState.RESUMED else EngineState.PAUSED

/**
 * Backs waiting for Dart's answer. Each is answered once: by Dart, or with
 * false when the area goes first, so JavaScript's `back()` always settles.
 */
class PendingBacks {
  private var next = 0
  private val waiting = mutableMapOf<Int, (Boolean) -> Unit>()

  fun add(answer: (Boolean) -> Unit): Int {
    val id = next++
    waiting[id] = answer
    return id
  }

  fun answer(id: Int, result: Boolean) {
    waiting.remove(id)?.invoke(result)
  }

  fun failAll() {
    val all = waiting.values.toList()
    waiting.clear()
    all.forEach { it(false) }
  }
}

/**
 * Whether the app is in the background, from its activities starting and
 * stopping: only when none is started. Begins in the foreground (an area is
 * made by the running app). Expo's foreground event can be skipped (no
 * current activity at resume), so it isn't used.
 */
class Foreground(private val changed: (inBackground: Boolean) -> Unit) {
  private var started = 1

  val inBackground: Boolean
    get() = started == 0

  fun started() {
    val was = inBackground
    started += 1
    if (was) changed(false)
  }

  fun stopped() {
    if (started == 0) return
    started -= 1
    if (started == 0) changed(true)
  }
}

/** Where an area's inputs, status and host calls go: the module's events. */
typealias AreaEvents = (name: String, body: Map<String, Any?>) -> Unit

/**
 * One area: its engine, its channel and what waits for Dart. Made with its
 * channel handler set, so a warmed engine's `ready` is never missed. Main
 * thread only.
 */
class Area(val engine: FlutterEngine, private val events: () -> AreaEvents?) {
  var id: String? = null
  private val channel = MethodChannel(engine.dartExecutor.binaryMessenger, CHANNEL)
  val pipe = AreaPipe { method, json -> channel.invokeMethod(method, json?.let { mapOf("json" to it) }) }
  private val readyWaiters = mutableListOf<(Throwable?) -> Unit>()

  /** The view drawing it now, if any. */
  var view: PluginAreaView? = null
    set(value) {
      field = value
      tellLifecycle()
    }

  private var told: EngineState? = null

  /** Tells the engine whether to run, when that changed. */
  fun tellLifecycle() {
    val state = engineState(view != null, Areas.inBackground)
    if (state == told) return
    told = state
    when (state) {
      EngineState.RESUMED -> engine.lifecycleChannel.appIsResumed()
      EngineState.PAUSED -> {
        engine.lifecycleChannel.appIsInactive()
        engine.lifecycleChannel.appIsPaused()
      }
    }
  }

  init {
    channel.setMethodCallHandler { call, result ->
      handle(call)
      result.success(null)
    }
  }

  private fun handle(call: MethodCall) {
    val areaId = id
    val json = call.argument<String>("json")
    when (call.method) {
      "ready" -> {
        pipe.ready()
        readyWaiters.toList().forEach { it(null) }
        readyWaiters.clear()
      }
      "input", "status" -> if (areaId != null && json != null) {
        events()?.invoke(call.method, mapOf("areaId" to areaId, "json" to json))
      }
      "hostCall" -> if (areaId != null) {
        events()?.invoke(
          "hostCall",
          mapOf(
            "areaId" to areaId,
            "id" to call.argument<Int>("id"),
            "method" to call.argument<String>("method"),
            "json" to (json ?: "null"),
          ),
        )
      }
    }
  }

  /** Runs [done] once Dart is listening: at once when it already is. */
  fun whenReady(done: (Throwable?) -> Unit) {
    if (pipe.isReady) done(null) else readyWaiters += done
  }

  fun hostReply(id: Int, json: String) {
    channel.invokeMethod("hostReply", mapOf("id" to id, "json" to json))
  }

  private val backs = PendingBacks()

  fun back(answer: (Boolean) -> Unit) {
    val id = backs.add(answer)
    channel.invokeMethod(
      "back",
      null,
      object : MethodChannel.Result {
        override fun success(result: Any?) = backs.answer(id, result == true)
        override fun error(code: String, message: String?, details: Any?) = backs.answer(id, false)
        override fun notImplemented() = backs.answer(id, false)
      },
    )
  }

  fun destroy() {
    view?.detach()
    readyWaiters.toList().forEach { it(IllegalStateException("destroyed")) }
    readyWaiters.clear()
    backs.failAll()
    channel.setMethodCallHandler(null)
    engine.destroy()
  }

  companion object {
    const val CHANNEL = "dev.opencord/plugin_area"
  }
}

/**
 * Every area in the process (plugins M4 plan, Task 4): one engine group, a
 * spare engine after `warmUp`, and the areas by id. An area lives from
 * `create` to `destroy`, whatever views show it. Main thread only.
 */
object Areas {
  private var group: FlutterEngineGroup? = null
  private var spare: Area? = null
  private val areas = mutableMapOf<String, Area>()
  var events: AreaEvents? = null

  private val foreground = Foreground { areas.values.forEach { area -> area.tellLifecycle() } }
  private var watching = false

  /** The app is in the background: every engine pauses. */
  val inBackground: Boolean
    get() = foreground.inBackground

  /** Follows the app's activities, once per process. */
  private fun watchApp(context: Context) {
    if (watching) return
    watching = true
    (context.applicationContext as Application).registerActivityLifecycleCallbacks(
      object : Application.ActivityLifecycleCallbacks {
        override fun onActivityStarted(activity: Activity) = foreground.started()
        override fun onActivityStopped(activity: Activity) = foreground.stopped()
        override fun onActivityCreated(activity: Activity, state: Bundle?) {}
        override fun onActivityResumed(activity: Activity) {}
        override fun onActivityPaused(activity: Activity) {}
        override fun onActivitySaveInstanceState(activity: Activity, state: Bundle) {}
        override fun onActivityDestroyed(activity: Activity) {}
      },
    )
  }

  private fun newArea(context: Context): Area {
    watchApp(context)
    val app = context.applicationContext
    val engineGroup = group ?: FlutterEngineGroup(app).also { group = it }
    return Area(engineGroup.createAndRunDefaultEngine(app)) { events }
  }

  fun warmUp(context: Context) {
    if (spare == null) spare = newArea(context)
  }

  fun coolDown() {
    spare?.destroy()
    spare = null
  }

  fun create(context: Context, id: String, setupJson: String): Area {
    areas.remove(id)?.destroy()
    val area = spare?.also { spare = null } ?: newArea(context)
    area.id = id
    area.pipe.setup(setupJson)
    areas[id] = area
    // Paused until a view shows it.
    area.tellLifecycle()
    return area
  }

  operator fun get(id: String): Area? = areas[id]

  fun destroy(id: String) {
    areas.remove(id)?.destroy()
  }

  fun destroyAll() {
    areas.values.toList().forEach { it.destroy() }
    areas.clear()
  }
}
