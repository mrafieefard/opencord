package dev.opencord.pluginarea

/**
 * What goes to one area's Dart side, in order (plugins M4 plan, "The plugin
 * area channel"): the setup, then the batches. Until Dart says `ready` they
 * wait here; `ready` sends the newest setup and the held batches, and
 * everything after goes straight through. Plain Kotlin, so a JVM test can
 * check it.
 */
class AreaPipe(private val send: (method: String, json: String?) -> Unit) {
  private val held = mutableListOf<String>()
  private var setup: String? = null
  var isReady = false
    private set

  fun setup(json: String) {
    setup = json
    if (isReady) send("setup", json)
  }

  fun push(json: String) {
    if (isReady) send("batch", json) else held += json
  }

  /** Dart is listening (again, after a restart of its isolate). */
  fun ready() {
    isReady = true
    setup?.let { send("setup", it) }
    val waiting = held.toList()
    held.clear()
    waiting.forEach { send("batch", it) }
  }
}
