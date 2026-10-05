package io.github.hewel.jellypilot.player

import org.junit.Assert.*
import org.junit.Test

class SubtitleTimingSessionTest {
  private fun tracks(selected: Int?) = listOf(1, 2).map { id ->
    PlayerTrack(id, TrackKind.SUBTITLE, null, null, "ass", false, false, false, id == selected)
  }

  @Test fun acknowledgedOffsetsFollowTrackIdentityAndOffRetainsThemUntilMediaEnds() {
    val session = SubtitleTimingSession()
    val writes = mutableListOf<Int>()
    val apply: (Int) -> Boolean = { writes += it; true }
    session.select(1, tracks(1), apply)
    val original = requireNotNull(session.state.context)
    assertTrue(session.begin(original))
    session.complete(original, 5, true)
    session.select(1, tracks(2), apply)
    assertEquals(0, session.state.offsetTenths)
    session.select(1, tracks(null), apply)
    assertEquals(SubtitleTimingAvailability.SUBTITLES_OFF, session.state.availability)
    session.select(1, tracks(1), apply)
    assertEquals(5, session.state.offsetTenths)
    assertNotEquals(original, session.state.context)
    assertFalse(session.begin(original))
    assertEquals(listOf(0, 0, 5), writes)
    session.clear()
    session.select(2, tracks(1), apply)
    assertEquals(0, session.state.offsetTenths)
    assertEquals(0, writes.last())
  }

  @Test fun rejectedWriteRetainsConfirmedValueAndRetryCanSucceed() {
    val session = SubtitleTimingSession()
    session.select(1, tracks(1)) { true }
    val context = requireNotNull(session.state.context)
    assertTrue(session.begin(context))
    assertFalse(session.begin(context))
    assertEquals(0, session.state.offsetTenths)
    session.complete(context, 1, false)
    assertFalse(session.state.pending)
    assertTrue(session.state.failed)
    assertEquals(0, session.state.offsetTenths)
    assertEquals(1L, session.state.requestRevision)
    assertTrue(session.begin(context))
    assertFalse(session.state.failed)
    session.complete(context, 1, false)
    assertEquals(2L, session.state.requestRevision)
    assertTrue(session.state.failed)
    assertTrue(session.begin(context))
    session.complete(context, 1, true)
    assertEquals(1, session.state.offsetTenths)
    assertEquals(3L, session.state.requestRevision)
  }

  @Test fun oldCompletionCannotAlterAReenteredTrackOrNewMedia() {
    val session = SubtitleTimingSession()
    session.select(1, tracks(1)) { true }
    val old = requireNotNull(session.state.context)
    assertTrue(session.begin(old))
    session.select(1, tracks(2)) { true }
    session.select(1, tracks(1)) { true }
    val returned = requireNotNull(session.state.context)
    assertTrue(session.begin(returned))
    session.complete(old, 99, true)
    assertTrue(session.state.pending)
    assertEquals(0, session.state.offsetTenths)
    session.complete(returned, -2, true)
    session.clear()
    session.select(2, tracks(1)) { true }
    session.complete(returned, 99, true)
    assertEquals(0, session.state.offsetTenths)
  }

  @Test fun capabilityRequiresAnActiveSubtitleAndSuccessfulNativeApplication() {
    val session = SubtitleTimingSession()
    var writes = 0
    val unavailable: (Int) -> Boolean = { writes++; false }
    session.select(1, emptyList(), unavailable)
    assertEquals(SubtitleTimingAvailability.NO_TRACKS, session.state.availability)
    session.select(1, tracks(null), unavailable)
    assertEquals(SubtitleTimingAvailability.SUBTITLES_OFF, session.state.availability)
    assertEquals(0, writes)
    session.select(1, tracks(1), unavailable)
    assertEquals(SubtitleTimingAvailability.UNSUPPORTED, session.state.availability)
    assertFalse(session.begin(requireNotNull(session.state.context)))
    assertEquals(1, writes)
  }

  @Test fun cancellationKeepsSuccessfulEarlierStepsWithoutInventingFailure() {
    val session = SubtitleTimingSession()
    session.select(1, tracks(1)) { true }
    val context = requireNotNull(session.state.context)
    session.begin(context)
    session.complete(context, 4, true)
    session.begin(context)
    session.cancel(context)
    assertFalse(session.state.pending)
    assertFalse(session.state.failed)
    assertEquals(4, session.state.offsetTenths)
    assertEquals(2L, session.state.requestRevision)
    // An admitted host request may be revoked before the executor begins it.
    session.cancel(context)
    assertEquals(3L, session.state.requestRevision)
  }
}
