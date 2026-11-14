package dev.opencord.pluginarea

import org.junit.Assert.assertEquals
import org.junit.Test

class PendingBacksTest {
  // Fails if a back waiting when its area is destroyed is never answered
  // (JavaScript's back() would never settle, and that press would do
  // nothing), or if one is answered twice.
  @Test
  fun every_back_is_answered_once_even_when_the_area_goes() {
    val pending = PendingBacks()
    val answers = mutableListOf<Pair<String, Boolean>>()
    val first = pending.add { result -> answers += "first" to result }
    pending.add { result -> answers += "second" to result }
    pending.answer(first, true)
    pending.answer(first, false)
    pending.failAll()
    pending.failAll()
    assertEquals(listOf("first" to true, "second" to false), answers)
  }
}
