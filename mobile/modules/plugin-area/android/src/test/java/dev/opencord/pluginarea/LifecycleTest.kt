package dev.opencord.pluginarea

import org.junit.Assert.assertEquals
import org.junit.Test

class LifecycleTest {
  // Fails if an engine with no view, or with the app in the background,
  // is told it's resumed (it would go on ticking and drawing frames).
  @Test
  fun an_engine_runs_only_while_shown_in_the_foreground() {
    assertEquals(EngineState.RESUMED, engineState(hasView = true, inBackground = false))
    assertEquals(EngineState.PAUSED, engineState(hasView = false, inBackground = false))
    assertEquals(EngineState.PAUSED, engineState(hasView = true, inBackground = true))
    assertEquals(EngineState.PAUSED, engineState(hasView = false, inBackground = true))
  }
}
