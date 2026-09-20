package io.github.hewel.jellypilot

import io.github.hewel.jellypilot.ffi.*
import io.github.hewel.jellypilot.ui.*
import java.util.Locale

/** Token-free presentation of SDK data; playback target selection remains in the SDK. */
internal class CatalogPresentation(
  private val scope: ProfileScopeRef,
  private val watchlist: Set<String>,
) {
  private fun image(id: String?, width: Int = 384) = id?.let { ArtworkUi(it, scope, width) }
  private fun rating(value: Float?) = value?.takeIf { it.isFinite() }?.let { String.format(Locale.getDefault(), "%.1f", it) }
  private fun episode(season: Int?, number: Int?) = number?.let {
    if (season == null) "E%02d".format(Locale.ROOT, it) else "S%02dE%02d".format(Locale.ROOT, season, it)
  }
  private fun metadata(year: Int?, season: Int? = null, number: Int? = null) =
    listOfNotNull(year?.toString(), episode(season, number)).joinToString(" · ")
  private fun cast(value: VideoDetailMetadata) = value.cast.mapIndexed { index, person ->
    CastUi("${person.name}:$index", person.name, person.role.orEmpty(), image(person.imageId, 192))
  }
  private fun streamLabel(streams: List<VideoStreamInfo>) =
    (streams.firstOrNull { it.isDefault } ?: streams.firstOrNull())?.let { it.displayTitle ?: it.language ?: it.codec }
  private fun quality(info: VideoMediaInfo?): List<String> = info?.let {
    listOfNotNull(
      it.videoHeight?.let { height -> when { height >= 2160u -> "4K"; height >= 1080u -> "1080p"; height >= 720u -> "720p"; else -> "${height}p" } },
      it.videoRange?.takeUnless { range -> range.equals("SDR", true) },
      streamLabel(it.audioStreams),
    )
  } ?: emptyList()

  fun library(item: VideoLibraryItem): MediaUi = MediaUi(
    id = item.id, title = item.name, itemType = item.itemType,
    metadata = metadata(item.productionYear, item.seasonNumber, item.episodeNumber),
    artwork = image(item.artworkImageId ?: item.seriesPosterImageId),
    overview = item.overview.orEmpty(), favorite = item.favorite, played = item.played,
    backdrop = image(item.backdropImageId ?: item.seriesBackdropImageId ?: item.episodeThumbImageId, 1280),
    logo = image(item.logoImageId, 512),
    rating = rating(item.communityRating),
    runtimeMinutes = item.runtimeSeconds?.let { (it / 60).toInt() },
    resumeSeconds = item.resumePositionSeconds ?: 0.0, durationSeconds = item.runtimeSeconds,
    inWatchlist = item.id in watchlist,
    playable = item.itemType in setOf("Movie", "Episode", "Series"),
    playTargetId = item.id,
    episodeCode = episode(item.seasonNumber, item.episodeNumber),
  )

  fun item(item: VideoItemDetail): MediaUi = MediaUi(
    id = item.id, title = item.name, itemType = item.itemType,
    metadata = metadata(item.productionYear, item.seasonNumber, item.episodeNumber),
    artwork = image(item.artworkImageId ?: item.seriesPosterImageId),
    overview = item.overview.orEmpty(), favorite = item.favorite, played = item.played,
    backdrop = image(item.backdropImageId, 1280), logo = image(item.logoImageId, 512),
    genres = item.genres, rating = rating(item.metadata.communityRating),
    runtimeMinutes = item.runtimeSeconds?.let { (it / 60).toInt() },
    resumeSeconds = item.resumePositionSeconds ?: 0.0, durationSeconds = item.runtimeSeconds,
    inWatchlist = item.id in watchlist, playable = item.canPlay,
    playTargetId = item.id, episodeCode = episode(item.seasonNumber, item.episodeNumber),
    cast = cast(item.metadata), quality = quality(item.mediaInfo),
    audioLabel = item.mediaInfo?.let { streamLabel(it.audioStreams) },
    subtitleLabel = item.mediaInfo?.let { streamLabel(it.subtitleStreams) },
  )

  fun show(show: VideoShowDetail): MediaUi = MediaUi(
    id = show.id, title = show.name, itemType = "Series", metadata = metadata(show.productionYear),
    artwork = image(show.artworkImageId), overview = show.overview.orEmpty(),
    favorite = show.favorite, played = show.played,
    backdrop = image(show.backdropImageId, 1280), logo = image(show.logoImageId, 512),
    genres = show.genres, rating = rating(show.metadata.communityRating),
    resumeSeconds = show.nextEpisode?.resumePositionSeconds ?: 0.0,
    durationSeconds = show.nextEpisode?.runtimeSeconds,
    inWatchlist = show.id in watchlist, playable = show.canPlay,
    playTargetId = show.nextEpisode?.id ?: show.id,
    episodeCode = show.nextEpisode?.let { episode(it.seasonNumber, it.episodeNumber) },
    cast = cast(show.metadata), seasons = show.seasons.map { SeasonUi(it.id, it.name) },
  )

  /** Unavailable saved items remain identifiable and removable, without claiming playable metadata. */
  fun unavailable(entry: WatchlistEntry): MediaUi = MediaUi(
    id = entry.itemId, title = entry.name, itemType = entry.itemType,
    metadata = episode(entry.seasonNumber, entry.episodeNumber).orEmpty(), artwork = null,
    overview = "", favorite = false, played = false, inWatchlist = true, playable = false,
    episodeCode = episode(entry.seasonNumber, entry.episodeNumber),
  )
}
