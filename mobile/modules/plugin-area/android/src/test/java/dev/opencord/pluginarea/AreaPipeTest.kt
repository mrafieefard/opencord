package dev.opencord.pluginarea

import org.junit.Assert.assertEquals
import org.junit.Test

class AreaPipeTest {
  private val sent = mutableListOf<Pair<String, String?>>()
  private val pipe = AreaPipe { method, json -> sent += method to json }

  // Fails if a batch pushed before Dart listens is sent early (lost), out
  // of order, or twice; or if one after ready waits.
  @Test
  fun batches_before_ready_are_held_in_order() {
    pipe.setup("setup-1")
    pipe.push("batch-1")
    pipe.push("batch-2")
    pipe.push("batch-3")
    assertEquals(emptyList<Pair<String, String?>>(), sent)

    pipe.ready()
    assertEquals(
      listOf("setup" to "setup-1", "batch" to "batch-1", "batch" to "batch-2", "batch" to "batch-3"),
      sent,
    )

    pipe.push("batch-4")
    assertEquals("batch" to "batch-4", sent.last())
    assertEquals(5, sent.size)
  }

  // Fails if a second ready (an engine restarted by hot restart) sends the
  // batches again.
  @Test
  fun a_second_ready_sends_only_the_setup() {
    pipe.setup("setup-1")
    pipe.push("batch-1")
    pipe.ready()
    sent.clear()
    pipe.ready()
    assertEquals(listOf<Pair<String, String?>>("setup" to "setup-1"), sent)
  }

  // Fails if a setup that changes after ready isn't sent at once.
  @Test
  fun a_new_setup_after_ready_is_sent() {
    pipe.ready()
    pipe.setup("setup-2")
    assertEquals(listOf<Pair<String, String?>>("setup" to "setup-2"), sent)
  }
}
