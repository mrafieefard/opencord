package dev.opencord.pluginarea

import org.junit.Assert.assertEquals
import org.junit.Test

class LifecycleTest {
  // Fails if an engine that lost its view, or with the app in the
  // background, is told it's resumed (it would go on ticking and drawing),
  // or if a new engine that was never shown is paused: it must lay out to
  // send the context that opens its view, and a paused one resumed before
  // its surface exists drew nothing (found live).
  @Test
  fun an_engine_runs_while_shown_or_never_shown_and_in_the_foreground() {
    assertEquals(EngineState.RESUMED, engineState(hasView = true, wasShown = true, inBackground = false))
    assertEquals(EngineState.RESUMED, engineState(hasView = false, wasShown = false, inBackground = false))
    assertEquals(EngineState.PAUSED, engineState(hasView = false, wasShown = true, inBackground = false))
    assertEquals(EngineState.PAUSED, engineState(hasView = true, wasShown = true, inBackground = true))
    assertEquals(EngineState.PAUSED, engineState(hasView = false, wasShown = false, inBackground = true))
  }
}
