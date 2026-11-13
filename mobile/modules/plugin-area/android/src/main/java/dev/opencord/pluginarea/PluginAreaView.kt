package dev.opencord.pluginarea

import android.content.Context
import android.view.ViewGroup
import expo.modules.kotlin.AppContext
import expo.modules.kotlin.views.ExpoView
import io.flutter.embedding.android.FlutterTextureView
import io.flutter.embedding.android.FlutterView

/**
 * Shows one area while mounted. The Flutter view draws into a transparent
 * texture, so React Native's sheets, animations and rounded clips work over
 * it. Detaching leaves the engine and its Dart state; a second view of the
 * same area takes it over.
 */
class PluginAreaView(context: Context, appContext: AppContext) : ExpoView(context, appContext) {
  private val flutterView = FlutterView(context, FlutterTextureView(context).apply { isOpaque = false })
  private var areaId: String? = null
  private var attached: Area? = null

  init {
    addView(flutterView, ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
  }

  fun setAreaId(id: String) {
    if (id == areaId) return
    areaId = id
    if (isAttachedToWindow) attach()
  }

  override fun onAttachedToWindow() {
    super.onAttachedToWindow()
    attach()
  }

  override fun onDetachedFromWindow() {
    detach()
    super.onDetachedFromWindow()
  }

  private fun attach() {
    val area = areaId?.let { Areas[it] } ?: return
    if (attached === area) return
    detach()
    area.view?.detach()
    flutterView.attachToFlutterEngine(area.engine)
    area.engine.lifecycleChannel.appIsResumed()
    area.view = this
    attached = area
  }

  fun detach() {
    val area = attached ?: return
    flutterView.detachFromFlutterEngine()
    if (area.view === this) area.view = null
    attached = null
  }
}
