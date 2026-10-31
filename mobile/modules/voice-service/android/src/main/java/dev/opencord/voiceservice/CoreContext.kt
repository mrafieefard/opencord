package dev.opencord.voiceservice

import android.content.Context

/**
 * Hands the Java VM and the app's Context to the Rust core, whose audio
 * library (cpal) reads them to talk to AudioManager. Implemented in the
 * core: crates/opencord-mobile/src/android_context.rs.
 */
internal object CoreContext {
  private const val CORE_LIBRARY = "opencord_mobile"

  fun attach(context: Context) {
    // The core is loaded already, as the core-native module's dependency;
    // loading it from Java too binds `attach` below to it.
    System.loadLibrary(CORE_LIBRARY)
    attachNative(context.applicationContext)
  }

  @JvmStatic
  private external fun attachNative(context: Context)
}
