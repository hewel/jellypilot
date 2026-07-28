import type {
  VideoLibraryItem,
  VideoLibraryPlayRequest,
  VideoSeason,
  VideoUserDataUpdateRequest,
} from '@bindings';
import {
  LibraryStatusPanel,
  UserDataControls,
  formatRuntime,
  seasonLabel,
} from '@components/library/shared';
import { Button, JellyPilotSelect, StatusBadge } from '@components/ui';
import type { JellyPilotSelectItem } from '@components/ui';
import { cx } from '@styled-system/css';
import { createMutation, createQuery, useQueryClient } from '@tanstack/solid-query';
import {
  Link,
  createFileRoute,
  useCanGoBack,
  useNavigate,
  useRouter,
} from '@tanstack/solid-router';
import { Exit, Option } from 'effect';
import { ChevronLeft, Play, RefreshCw, Tv } from 'lucide-solid';
import { For, Show, Suspense, createMemo, createSignal } from 'solid-js';
import { commandFailureMessage } from '~effects/commands';
import { fetchConnectionState } from '~effects/connection';
import {
  fetchSeasonEpisodes,
  fetchVideoItemDetail,
  fetchVideoShowDetail,
  initialSeasonForShow,
  startLibraryPlayback,
  updateLibraryUserData,
} from '~effects/library';
import type { LibraryExit, SeasonEpisodesState } from '~effects/library';
import {
  isLibrarySessionKeyConnected,
  librarySessionKeyFromConnectionExit,
  queryKeys,
  runExit,
} from '~effects/query';
import { imageSource } from '~utils/imageSource';

import { AUTHENTICATED_HOME_ROUTE } from '../../../../router-guards';
import * as styles from '../detailRoute.styles';
import * as showStyles from './showDetail.styles';

export const Route = createFileRoute('/_authenticated/library/shows/$seriesId')({
  component: LibraryShowDetailRoute,
});

function LibraryShowDetailRoute() {
  const params = Route.useParams();
  const router = useRouter();
  const navigate = useNavigate();
  const canGoBack = useCanGoBack();
  const queryClient = useQueryClient();
  const connectionQuery = createQuery(() => ({
    queryKey: queryKeys.connectionState,
    queryFn: () => runExit(fetchConnectionState),
    staleTime: Infinity,
  }));
  const sessionKey = createMemo(() => librarySessionKeyFromConnectionExit(connectionQuery.data));
  const showQuery = createQuery(() => ({
    queryKey: queryKeys.libraryShowDetail(sessionKey(), params().seriesId),
    enabled: isLibrarySessionKeyConnected(sessionKey()),
    queryFn: () => runExit(fetchVideoShowDetail(params().seriesId)),
  }));
  const [selectedSeason, setSelectedSeason] = createSignal<VideoSeason | null>(null);
  const playbackMutation = createMutation(() => ({
    mutationFn: (request: VideoLibraryPlayRequest) => runExit(startLibraryPlayback(request)),
  }));
  const userDataMutation = createMutation(() => ({
    mutationFn: (request: VideoUserDataUpdateRequest) => runExit(updateLibraryUserData(request)),
  }));
  const [playBusy, setPlayBusy] = createSignal(false);
  const [episodePlayBusy, setEpisodePlayBusy] = createSignal<string | null>(null);
  const [playError, setPlayError] = createSignal<string | null>(null);

  const closeDetail = () => {
    if (canGoBack()) {
      router.history.back();
      return;
    }

    void navigate({ to: AUTHENTICATED_HOME_ROUTE });
  };
  const detail = () =>
    showQuery.data && Exit.isSuccess(showQuery.data) ? showQuery.data.value : null;
  const activeSeason = () => {
    const selected = selectedSeason();
    if (selected) {
      return selected;
    }

    return Option.fromNullishOr(detail()).pipe(
      Option.flatMap((show) => initialSeasonForShow(show)),
      Option.getOrNull,
    );
  };
  const seasonEpisodesQuery = createQuery<LibraryExit<SeasonEpisodesState> | null>(() => {
    const season = activeSeason();
    return {
      queryKey: queryKeys.librarySeasonEpisodes(
        sessionKey(),
        params().seriesId,
        season?.id ?? 'none',
      ),
      enabled: season !== null && isLibrarySessionKeyConnected(sessionKey()),
      queryFn: () => {
        if (!season) {
          return Promise.resolve(null);
        }
        return runExit(
          fetchSeasonEpisodes({
            seasonId: season.id,
            seasonNumber: season.seasonNumber,
            seriesId: params().seriesId,
          }),
        );
      },
    };
  });
  const currentEpisodes = () => seasonEpisodesQuery.data;
  const seasonEpisodes = () => {
    const current = currentEpisodes();
    return current && Exit.isSuccess(current) ? current.value.page.episodes : [];
  };
  const hasSeasonEpisodes = () => seasonEpisodes().length > 0;
  const episodesLoading = () => seasonEpisodesQuery.isPending || seasonEpisodesQuery.isFetching;
  const loadEpisodes = (season: VideoSeason) => {
    setSelectedSeason(season);
  };
  const startEpisodePlayback = async (itemId: string) => {
    const result = await queryClient.fetchQuery({
      queryKey: queryKeys.libraryItemDetail(sessionKey(), itemId),
      queryFn: () => runExit(fetchVideoItemDetail(itemId)),
    });
    if (Exit.isFailure(result)) {
      setPlayError(commandFailureMessage(result.cause, 'Could not load episode'));
      return;
    }

    const episodeDetail = result.value;
    const mode = episodeDetail.canResume ? 'resume' : 'start';
    const playResult = await playbackMutation.mutateAsync({
      audioStreamIndex: null,
      itemId: episodeDetail.id,
      mode,
      startPositionSeconds: mode === 'resume' ? episodeDetail.resumePositionSeconds : 0,
      subtitleStreamIndex: null,
    });
    setPlayError(
      Exit.match(playResult, {
        onFailure: (cause) => commandFailureMessage(cause, 'Could not start playback'),
        onSuccess: () => null,
      }),
    );
  };
  const playShow = async () => {
    const show = detail();
    if (!show?.nextEpisode || playBusy()) {
      return;
    }

    setPlayBusy(true);
    setPlayError(null);
    await startEpisodePlayback(show.nextEpisode.id);
    setPlayBusy(false);
  };
  const playEpisode = async (episode: VideoLibraryItem) => {
    if (episodePlayBusy()) {
      return;
    }

    setEpisodePlayBusy(episode.id);
    setPlayError(null);
    await startEpisodePlayback(episode.id);
    setEpisodePlayBusy(null);
  };
  const statusTitle = () => {
    const current = showQuery.data;
    if (current && !Exit.isSuccess(current)) {
      return 'Could not load show detail';
    }
    return 'Loading show detail';
  };
  const statusDescription = () => {
    const current = showQuery.data;
    if (current && !Exit.isSuccess(current)) {
      return commandFailureMessage(current.cause, 'Could not load show detail');
    }
    return 'JellyPilot is loading Show detail, seasons, and Jellyfin next-up data.';
  };
  const episodesStatusTitle = () => {
    const current = currentEpisodes();
    if (episodesLoading()) {
      return 'Loading season episodes';
    }
    if (current && Exit.isSuccess(current) && current.value.page.episodes.length === 0) {
      return 'Season has no episodes';
    }
    if (current && !Exit.isSuccess(current)) {
      return 'Could not load season episodes';
    }
    return 'Choose a season';
  };
  const episodesStatusDescription = () => {
    const current = currentEpisodes();
    if (episodesLoading()) {
      return 'JellyPilot is loading exact Episode cards for the selected Season.';
    }
    if (current && Exit.isSuccess(current) && current.value.page.episodes.length === 0) {
      return 'Jellyfin returned no Episodes for the selected Season.';
    }
    if (current && !Exit.isSuccess(current)) {
      return commandFailureMessage(current.cause, 'Could not load season episodes');
    }
    return 'Season buttons keep manual episode selection available alongside Jellyfin next-up resolution.';
  };
  const episodeLabel = (ep: VideoLibraryItem) => {
    if (ep.seasonNumber != null && ep.episodeNumber != null) {
      return `S${ep.seasonNumber.toString().padStart(2, '0')}E${ep.episodeNumber.toString().padStart(2, '0')}`;
    }
    return 'Episode';
  };
  const playShowLabel = () => {
    const show = detail();
    const nextEpisode = show?.nextEpisode;
    if (!nextEpisode) {
      return 'Play';
    }
    const prefix =
      nextEpisode.resumePositionSeconds != null &&
      nextEpisode.resumePositionSeconds > 0 &&
      !nextEpisode.played
        ? 'Continue'
        : 'Play';
    return `${prefix} ${episodeLabel(nextEpisode)}`;
  };
  const heroMeta = () => {
    const show = detail();
    if (!show) {
      return '';
    }
    const parts: string[] = [];
    if (show.productionYear !== null) {
      parts.push(show.productionYear.toString());
    }
    if (show.seasons.length > 0) {
      parts.push(`${show.seasons.length} ${show.seasons.length === 1 ? 'season' : 'seasons'}`);
    }
    return parts.join(' · ');
  };
  const seasonMeta = () => {
    const episodes = seasonEpisodes();
    if (episodes.length === 0) {
      return null;
    }
    const totalSeconds = episodes.reduce((sum, ep) => sum + (ep.runtimeSeconds ?? 0), 0);
    const total = totalSeconds > 0 ? formatRuntime(totalSeconds) : null;
    const count = `${episodes.length} ${episodes.length === 1 ? 'episode' : 'episodes'}`;
    return total ? `${count} · ${total} total` : count;
  };

  return (
    <div class={styles.stack}>
      <Suspense fallback={<ShowDetailSkeleton />}>
        <Show
          when={detail()}
          fallback={<LibraryStatusPanel title={statusTitle()} description={statusDescription()} />}
        >
          {(show) => (
            <div class={styles.page}>
              <button type="button" class={styles.backLink} aria-label="Back" onClick={closeDetail}>
                <ChevronLeft class={styles.icon4} aria-hidden="true" />
                Back to library
              </button>

              <section class={styles.hero} aria-labelledby="show-detail-title">
                <div class={styles.heroInfo}>
                  <div class={styles.badgeRow}>
                    <span class={styles.typeBadge}>
                      <Tv class={styles.icon4} aria-hidden="true" />
                      Series
                    </span>
                    <Show when={show().productionYear}>
                      {(year) => <span class={styles.badge}>{year()}</span>}
                    </Show>
                    <For each={show().genres.slice(0, 3)}>
                      {(genre) => <span class={styles.badge}>{genre}</span>}
                    </For>
                    <StatusBadge variant={show().played ? 'success' : 'neutral'}>
                      {show().played ? 'Played' : 'Unplayed'}
                    </StatusBadge>
                    <StatusBadge variant={show().favorite ? 'success' : 'neutral'}>
                      {show().favorite ? 'Favorite' : 'Not favorite'}
                    </StatusBadge>
                  </div>

                  <h1 id="show-detail-title" class={styles.heroTitle}>
                    {show().name}
                  </h1>

                  <Show when={heroMeta()}>{(meta) => <p class={styles.heroMeta}>{meta()}</p>}</Show>

                  <Show when={show().overview}>
                    {(overview) => <p class={styles.heroOverview}>{overview()}</p>}
                  </Show>

                  <div class={styles.accentBar} aria-hidden="true" />

                  <div class={styles.heroActions}>
                    <Button
                      type="button"
                      variant="primary"
                      class={styles.pillButton}
                      disabled={!show().nextEpisode || playBusy()}
                      onClick={() => void playShow()}
                      leadingIcon={
                        <Show when={playBusy()} fallback={<Play class={styles.playIcon} />}>
                          <RefreshCw class={cx(styles.icon4, styles.spinner)} />
                        </Show>
                      }
                    >
                      {playBusy() ? 'Loading...' : playShowLabel()}
                    </Button>
                    <UserDataControls
                      itemId={show().id}
                      played={show().played}
                      favorite={show().favorite}
                      subject="show"
                      onUpdate={(request) => userDataMutation.mutateAsync(request)}
                      onSuccess={() => {
                        queryClient.invalidateQueries({
                          queryKey: queryKeys.libraryShowDetail(sessionKey(), params().seriesId),
                        });
                        queryClient.invalidateQueries({
                          queryKey: queryKeys.libraryMediaDetail(
                            sessionKey(),
                            'Series',
                            params().seriesId,
                          ),
                        });
                        queryClient.invalidateQueries({
                          queryKey: queryKeys.librarySeasonEpisodesRoot(
                            sessionKey(),
                            params().seriesId,
                          ),
                        });
                        queryClient.invalidateQueries({
                          queryKey: queryKeys.libraryHome(sessionKey()),
                        });
                        queryClient.invalidateQueries({
                          queryKey: queryKeys.libraryBrowseRoot(sessionKey()),
                        });
                      }}
                    />
                  </div>
                </div>

                <div class={styles.heroArt}>
                  <Show
                    when={show().artworkImageId}
                    fallback={
                      <div class={styles.heroArtFallback} aria-hidden="true">
                        {show().name.charAt(0)}
                      </div>
                    }
                  >
                    {(imageId) => (
                      <img
                        src={imageSource(imageId())}
                        alt={`${show().name} artwork`}
                        class={styles.heroArtImage}
                      />
                    )}
                  </Show>
                  <Show when={show().productionYear}>
                    {(year) => <span class={styles.heroArtYear}>{year()}</span>}
                  </Show>
                </div>
              </section>

              <Show
                when={show().seasons.length > 0}
                fallback={
                  <LibraryStatusPanel
                    title="No seasons available"
                    description="Jellyfin returned no seasons for this show."
                  />
                }
              >
                <div class={showStyles.seasonBar}>
                  <SeasonSelector
                    seasons={show().seasons}
                    activeSeason={activeSeason()}
                    disabled={episodesLoading()}
                    onSelect={loadEpisodes}
                  />
                  <Show when={seasonMeta()}>
                    {(meta) => <p class={showStyles.seasonMeta}>{meta()}</p>}
                  </Show>
                </div>

                <Suspense fallback={<SeasonEpisodesSkeleton />}>
                  <Show
                    when={hasSeasonEpisodes()}
                    fallback={
                      episodesLoading() ? (
                        <SeasonEpisodesSkeleton />
                      ) : (
                        <LibraryStatusPanel
                          title={episodesStatusTitle()}
                          description={episodesStatusDescription()}
                        />
                      )
                    }
                  >
                    <section aria-label="Season episodes" class={showStyles.episodeList}>
                      <For each={seasonEpisodes()}>
                        {(episode) => (
                          <EpisodeRow
                            episode={episode}
                            label={episodeLabel(episode)}
                            busy={episodePlayBusy() === episode.id}
                            disabled={episodePlayBusy() !== null}
                            onPlay={() => void playEpisode(episode)}
                          />
                        )}
                      </For>
                    </section>
                  </Show>
                </Suspense>
              </Show>
            </div>
          )}
        </Show>
      </Suspense>
      <Show when={playError()}>{(message) => <p class={styles.error}>{message()}</p>}</Show>
    </div>
  );
}

function SeasonSelector(props: {
  seasons: VideoSeason[];
  activeSeason: VideoSeason | null;
  disabled: boolean;
  onSelect: (season: VideoSeason) => void;
}) {
  const seasonItems = createMemo<JellyPilotSelectItem[]>(() =>
    props.seasons.map((season) => ({
      label: seasonLabel(season),
      value: season.id,
    })),
  );
  const selectSeason = (seasonId: string) => {
    const season = props.seasons.find((item) => item.id === seasonId);
    if (season) {
      props.onSelect(season);
    }
  };

  return (
    <Show
      when={props.seasons.length > 6}
      fallback={
        <ul class={showStyles.seasonTabs} aria-label="Show seasons">
          <For each={props.seasons}>
            {(season) => (
              <li>
                <button
                  type="button"
                  class={showStyles.seasonTab}
                  classList={{ [showStyles.activeSeasonTab]: props.activeSeason?.id === season.id }}
                  aria-pressed={props.activeSeason?.id === season.id}
                  disabled={props.disabled}
                  onClick={() => props.onSelect(season)}
                >
                  {seasonLabel(season)}
                </button>
              </li>
            )}
          </For>
        </ul>
      }
    >
      <div class={showStyles.seasonSelectWrap}>
        <JellyPilotSelect
          label="Season"
          items={seasonItems()}
          disabled={props.disabled}
          value={props.activeSeason?.id ?? ''}
          size="compact"
          onValueChange={selectSeason}
        />
      </div>
    </Show>
  );
}

function EpisodeRow(props: {
  episode: VideoLibraryItem;
  label: string;
  busy: boolean;
  disabled: boolean;
  onPlay: () => void;
}) {
  const hasResume = () =>
    props.episode.resumePositionSeconds != null &&
    props.episode.resumePositionSeconds > 0 &&
    !props.episode.played;

  const number =
    props.episode.episodeNumber !== null
      ? props.episode.episodeNumber.toString().padStart(2, '0')
      : '–';

  return (
    <div class={showStyles.episodeRow}>
      <span class={showStyles.episodeNumber} aria-hidden="true">
        {number}
      </span>

      <div class={showStyles.episodeCopy}>
        <Link
          to="/library/items/$itemId"
          params={{ itemId: props.episode.id }}
          class={showStyles.episodeTitle}
        >
          {props.episode.name}
        </Link>
        <div class={showStyles.episodeSub}>
          <span>{props.label}</span>
          <Show when={props.episode.played}>
            <span aria-hidden="true">·</span>
            <span>Played</span>
          </Show>
          <Show when={hasResume()}>
            <span aria-hidden="true">·</span>
            <span>{Math.round(props.episode.playedPercentage ?? 0)}% watched</span>
          </Show>
        </div>
      </div>

      <Show when={formatRuntime(props.episode.runtimeSeconds)}>
        {(runtime) => <span class={showStyles.episodeRuntime}>{runtime()}</span>}
      </Show>

      <Button
        type="button"
        variant="outlined"
        class={styles.pillButton}
        disabled={props.disabled}
        onClick={props.onPlay}
        leadingIcon={
          <Show when={props.busy} fallback={<Play class={styles.playIcon} />}>
            <RefreshCw class={cx(styles.icon4, styles.spinner)} />
          </Show>
        }
      >
        {props.busy ? 'Loading...' : hasResume() ? 'Resume' : 'Play'}
      </Button>
    </div>
  );
}

function ShowDetailSkeleton() {
  return (
    <div class={styles.page} aria-hidden="true">
      <div class={styles.skeletonHero} />
      <div class={styles.skeletonBar} />
      <SeasonEpisodesSkeleton />
    </div>
  );
}

function SeasonEpisodesSkeleton() {
  return (
    <div class={showStyles.episodeList} aria-hidden="true">
      <For each={[0, 1, 2]}>{() => <div class={showStyles.skeletonRow} />}</For>
    </div>
  );
}
