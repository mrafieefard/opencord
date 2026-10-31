package dev.opencord.voiceservice

import android.content.Context
import android.content.Intent
import androidx.core.content.ContextCompat
import expo.modules.kotlin.exception.Exceptions
import expo.modules.kotlin.modules.Module
import expo.modules.kotlin.modules.ModuleDefinition

class VoiceServiceModule : Module() {
  private val context: Context
    get() = appContext.reactContext ?: throw Exceptions.ReactContextLost()

  override fun definition() = ModuleDefinition {
    Name("VoiceService")

    // Before any voice: the core's audio needs the Context (CoreContext).
    OnCreate {
      CoreContext.attach(context)
    }

    // Starting a running service only replaces its notification.
    Function("start") { title: String, text: String ->
      val intent = Intent(context, VoiceService::class.java)
        .putExtra(VoiceService.EXTRA_TITLE, title)
        .putExtra(VoiceService.EXTRA_TEXT, text)
      ContextCompat.startForegroundService(context, intent)
    }

    Function("stop") {
      context.stopService(Intent(context, VoiceService::class.java))
    }
  }
}
