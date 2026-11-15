package dev.opencord.pluginarea

import org.junit.Assert.assertEquals
import org.junit.Test

class ForegroundTest {
  // Fails if the app is taken for backgrounded while one of its activities
  // is still started (a picker's or a dialog's over it), stays backgrounded
  // after one starts again, or goes below zero on a stop it never saw start.
  @Test
  fun the_app_is_in_the_background_only_when_no_activity_is_started() {
    val changes = mutableListOf<Boolean>()
    val foreground = Foreground { background -> changes += background }
    assertEquals(false, foreground.inBackground)
    foreground.started() // a picker over the app
    foreground.stopped()
    assertEquals(false, foreground.inBackground)
    foreground.stopped() // the app's own activity: home pressed
    assertEquals(true, foreground.inBackground)
    foreground.stopped()
    foreground.started()
    assertEquals(false, foreground.inBackground)
    assertEquals(listOf(true, false), changes)
  }
}
