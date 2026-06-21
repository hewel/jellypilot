import type { VideoLibraryPlayMode, VideoPlaybackStreamOption } from '@bindings';
import {
  LibraryStatusPanel,
  UserDataControls,
  detailSubtitle,
  formatRuntime,
} from '@components/library/shared';
import { Button, Card, JmsrSelect, StatusBadge } from '@components/ui';
import type { JmsrSelectItem } from '@components/ui';
import { createFileRoute } from '@tanstack/solid-router';
import { Exit } from 'effect';
import { Film, Library, Play, RefreshCw, RotateCcw } from 'lucide-solid';
import {
  For,
  Show,
  Suspense,
  createEffect,
  createMemo,
  createResource,
  createSignal,
} from 'solid-js';
import { commandFailureMessage } from '~effects/commands';
import {
  fetchVideoItemDetail,
  startLibraryPlayback,
  updateLibraryUserData,
} from '~effects/library';
import type { LibraryDetailState, LibraryExit } from '~effects/library';

const AUDIO_AUTO = 'auto';
const SUBTITLE_AUTO = 'auto';
const SUBTITLE_OFF = 'off';

export const Route = createFileRoute('/_authenticated/library/items/$itemId')({
  loader: ({ params }) => ({
    detail: fetchVideoItemDetail(params.itemId),
  }),
  component: LibraryItemDetailRoute,
});

function LibraryItemDetailRoute() {
  const params = Route.useParams();
  const loaderData = Route.useLoaderData();
  const [manualDetailPromise, setManualDetailPromise] = createSignal<Promise<
    LibraryExit<LibraryDetailState>
  > | null>(null);
  const [state] = createResource(
    () => manualDetailPromise() ?? loaderData().detail,
    (promise) => promise,
  );
  const [playBusy, setPlayBusy] = createSignal<VideoLibraryPlayMode | null>(null);
  const [audioValue, setAudioValue] = createSignal(AUDIO_AUTO);
  const [subtitleValue, setSubtitleValue] = createSignal(SUBTITLE_AUTO);
  const [playError, setPlayError] = createSignal<string | null>(null);

  createEffect(() => {
    params().itemId;
    setManualDetailPromise(null);
  });

  const reloadDetail = () => {
    setManualDetailPromise(fetchVideoItemDetail(params().itemId));
  };

  const detail = () => {
    const current = state.latest ?? state();
    return current && Exit.isSuccess(current) ? current.value : null;
  };
  const audioItems = createMemo<JmsrSelectItem[]>(() => [
    { label: 'Auto (series preference)', value: AUDIO_AUTO },
    ...(detail()?.audioStreams ?? []).map((stream) => ({
      label: streamLabel(stream),
      value: String(stream.index),
    })),
  ]);
  const subtitleItems = createMemo<JmsrSelectItem[]>(() => [
    { label: 'Auto (preferred subtitles)', value: SUBTITLE_AUTO },
    { label: 'Off', value: SUBTITLE_OFF },
    ...(detail()?.subtitleStreams ?? []).map((stream) => ({
      label: streamLabel(stream),
      value: String(stream.index),
    })),
  ]);

  createEffect(() => {
    const itemId = detail()?.id;
    if (!itemId) {
      return;
    }

    setAudioValue(AUDIO_AUTO);
    setSubtitleValue(SUBTITLE_AUTO);
  });

  const selectedAudioStreamIndex = () =>
    audioValue() === AUDIO_AUTO ? null : Number(audioValue());
  const selectedSubtitleStreamIndex = () => {
    const value = subtitleValue();
    if (value === SUBTITLE_AUTO) {
      return null;
    }
    if (value === SUBTITLE_OFF) {
      return -1;
    }
    return Number(value);
  };
  const playItem = async (mode: VideoLibraryPlayMode) => {
    const item = detail();
    if (!item || playBusy()) {
      return;
    }

    setPlayBusy(mode);
    setPlayError(null);
    const result = await startLibraryPlayback({
      audioStreamIndex: selectedAudioStreamIndex(),
      itemId: item.id,
      mode,
      startPositionSeconds: mode === 'resume' ? item.resumePositionSeconds : 0,
      subtitleStreamIndex: selectedSubtitleStreamIndex(),
    });
    setPlayError(
      Exit.match(result, {
        onFailure: (cause) => commandFailureMessage(cause, 'Could not start playback'),
        onSuccess: () => null,
      }),
    );
    setPlayBusy(null);
  };
  const statusTitle = () => {
    const current = state.latest ?? state();
    if (current && !Exit.isSuccess(current)) {
      return 'Could not load item detail';
    }
    return 'Loading item detail';
  };
  const statusDescription = () => {
    const current = state.latest ?? state();
    if (current && !Exit.isSuccess(current)) {
      return commandFailureMessage(current.cause, 'Could not load item detail');
    }
    return 'JMSR is loading Movie or Episode detail data from Jellyfin.';
  };

  return (
    <div class="space-y-6">
      <div class="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
        <Button
          href="/library"
          variant="outlined"
          class="rounded-full"
          leadingIcon={<Library class="h-4 w-4" />}
        >
          Video Home
        </Button>
        <Button
          type="button"
          variant="outlined"
          class="rounded-full"
          disabled={state.loading}
          onClick={reloadDetail}
          leadingIcon={<RefreshCw class="h-4 w-4" classList={{ 'animate-spin': state.loading }} />}
        >
          Retry Detail
        </Button>
      </div>

      <Suspense fallback={<ItemDetailSkeleton />}>
        <Show
          when={detail()}
          fallback={<LibraryStatusPanel title={statusTitle()} description={statusDescription()} />}
        >
          {(item) => {
            const isEpisode = () => item().itemType === 'Episode';
            const artworkAspectClass = () => (isEpisode() ? 'aspect-video' : 'aspect-[2/3]');
            const missingArtworkLabel = () => (isEpisode() ? 'No episode artwork' : 'No artwork');

            return (
              <article class="grid gap-6 lg:grid-cols-[minmax(240px,360px)_1fr]">
                <Card variant="filled" surfaceTint={false} class="overflow-hidden !p-0">
                  <div class={`${artworkAspectClass()} bg-surface-container-lowest/60`}>
                    <Show
                      when={item().artworkUrl}
                      fallback={
                        <div class="text-on-surface-variant flex h-full flex-col items-center justify-center gap-3 px-6 text-center">
                          <Film class="h-8 w-8" />
                          <p class="text-on-surface text-[16px] leading-[24px] font-semibold">
                            {item().name}
                          </p>
                          <p class="text-on-surface-variant/90 text-[11px] leading-[16px] font-bold tracking-[0.08em] uppercase">
                            {missingArtworkLabel()}
                          </p>
                        </div>
                      }
                    >
                      {(artworkUrl) => (
                        <img
                          src={artworkUrl()}
                          alt={`${item().name} artwork`}
                          class="h-full w-full object-cover"
                        />
                      )}
                    </Show>
                  </div>
                </Card>
                <div class="space-y-5">
                  <div>
                    <p class="text-secondary text-[11px] leading-[16px] font-bold tracking-[0.08em] uppercase">
                      {item().itemType}
                    </p>
                    <h1 class="font-display text-[32px] leading-[40px] font-bold tracking-tight">
                      {item().name}
                    </h1>
                    <p class="text-on-surface-variant mt-2 text-[16px] leading-[24px]">
                      {detailSubtitle(item())}
                    </p>
                    <Show when={isEpisode() && item().seriesId}>
                      <a
                        href={`/library/shows/${item().seriesId}`}
                        class="text-secondary mt-1 inline-block text-[12px] leading-[16px] underline-offset-4 hover:underline"
                      >
                        View series
                      </a>
                    </Show>
                  </div>
                  <div class="flex flex-wrap gap-2">
                    <StatusBadge variant={item().played ? 'success' : 'neutral'}>
                      {item().played ? 'Played' : 'Unplayed'}
                    </StatusBadge>
                    <StatusBadge variant={item().favorite ? 'success' : 'neutral'}>
                      {item().favorite ? 'Favorite' : 'Not favorite'}
                    </StatusBadge>
                    <Show when={formatRuntime(item().runtimeSeconds)}>
                      {(runtime) => <StatusBadge variant="neutral">{runtime()}</StatusBadge>}
                    </Show>
                  </div>
                  <UserDataControls
                    itemId={item().id}
                    played={item().played}
                    favorite={item().favorite}
                    subject={item().itemType.toLowerCase()}
                    onUpdate={updateLibraryUserData}
                    onSuccess={reloadDetail}
                  />
                  <Show when={item().overview}>
                    {(overview) => (
                      <p class="text-on-surface-variant text-[14px] leading-[20px]">{overview()}</p>
                    )}
                  </Show>
                  <Show when={item().genres.length > 0}>
                    <div class="flex flex-wrap gap-2">
                      <For each={item().genres}>
                        {(genre) => (
                          <span class="border-outline-variant text-on-surface-variant/90 rounded-full border px-3 py-1 text-[11px] leading-[16px] font-bold tracking-[0.08em] uppercase">
                            {genre}
                          </span>
                        )}
                      </For>
                    </div>
                  </Show>
                  <Show when={item().resumePositionSeconds !== null}>
                    <p class="text-secondary text-[12px] leading-[16px]">
                      Resume at {Math.floor(item().resumePositionSeconds ?? 0)}s
                      {item().playedPercentage !== null
                        ? ` · ${Math.round(item().playedPercentage ?? 0)}% watched`
                        : ''}
                    </p>
                  </Show>
                  <div class="grid gap-4 sm:grid-cols-2">
                    <JmsrSelect
                      label="Audio track"
                      items={audioItems()}
                      disabled={playBusy() !== null}
                      value={audioValue()}
                      size="compact"
                      onValueChange={setAudioValue}
                    />

                    <JmsrSelect
                      label="Subtitle track"
                      items={subtitleItems()}
                      disabled={playBusy() !== null}
                      value={subtitleValue()}
                      size="compact"
                      onValueChange={setSubtitleValue}
                    />
                  </div>
                  <div class="flex flex-wrap gap-3">
                    <Show
                      when={item().canResume}
                      fallback={
                        <Button
                          type="button"
                          variant="primary"
                          class="rounded-full"
                          disabled={playBusy() !== null}
                          onClick={() => void playItem('start')}
                          leadingIcon={
                            <Show
                              when={playBusy() === 'start'}
                              fallback={<Play class="h-4 w-4 fill-current" />}
                            >
                              <RefreshCw class="h-4 w-4 animate-spin" />
                            </Show>
                          }
                        >
                          {playBusy() === 'start' ? 'Starting...' : 'Play'}
                        </Button>
                      }
                    >
                      <Button
                        type="button"
                        variant="primary"
                        class="rounded-full"
                        disabled={playBusy() !== null}
                        onClick={() => void playItem('resume')}
                        leadingIcon={
                          <Show
                            when={playBusy() === 'resume'}
                            fallback={<Play class="h-4 w-4 fill-current" />}
                          >
                            <RefreshCw class="h-4 w-4 animate-spin" />
                          </Show>
                        }
                      >
                        {playBusy() === 'resume' ? 'Starting...' : 'Resume'}
                      </Button>
                      <Button
                        type="button"
                        variant="secondary"
                        class="rounded-full"
                        disabled={playBusy() !== null}
                        onClick={() => void playItem('start')}
                        leadingIcon={
                          <Show
                            when={playBusy() === 'start'}
                            fallback={<RotateCcw class="h-4 w-4" />}
                          >
                            <RefreshCw class="h-4 w-4 animate-spin" />
                          </Show>
                        }
                      >
                        {playBusy() === 'start' ? 'Starting...' : 'Play from beginning'}
                      </Button>
                    </Show>
                  </div>
                  <Show when={playError()}>
                    {(message) => <p class="text-error text-[12px] leading-[16px]">{message()}</p>}
                  </Show>
                </div>
              </article>
            );
          }}
        </Show>
      </Suspense>
    </div>
  );
}

function ItemDetailSkeleton() {
  return (
    <article class="grid gap-6 lg:grid-cols-[minmax(240px,360px)_1fr]" aria-hidden="true">
      <Card variant="filled" surfaceTint={false} class="overflow-hidden !p-0">
        <div class="bg-surface-container-lowest/60 aspect-[2/3] animate-pulse" />
      </Card>
      <div class="space-y-5">
        <div class="space-y-3">
          <div class="bg-surface-container-high/60 h-3 w-20 animate-pulse rounded" />
          <div class="bg-surface-container-high/80 h-9 w-4/5 max-w-lg animate-pulse rounded-md" />
          <div class="bg-surface-container-high/60 h-5 w-2/3 max-w-md animate-pulse rounded" />
        </div>
        <div class="flex flex-wrap gap-2">
          <For each={[0, 1, 2]}>
            {() => <div class="bg-surface-container-high/70 h-7 w-24 animate-pulse rounded-full" />}
          </For>
        </div>
        <div class="flex flex-wrap gap-3">
          <div class="bg-surface-container-high/70 h-10 w-32 animate-pulse rounded-full" />
          <div class="bg-surface-container-high/60 h-10 w-36 animate-pulse rounded-full" />
        </div>
        <div class="space-y-2">
          <div class="bg-surface-container-high/60 h-4 w-full animate-pulse rounded" />
          <div class="bg-surface-container-high/60 h-4 w-11/12 animate-pulse rounded" />
          <div class="bg-surface-container-high/50 h-4 w-3/5 animate-pulse rounded" />
        </div>
        <div class="grid gap-4 sm:grid-cols-2">
          <div class="bg-surface-container-high/60 h-14 animate-pulse rounded-xl" />
          <div class="bg-surface-container-high/60 h-14 animate-pulse rounded-xl" />
        </div>
        <div class="flex flex-wrap gap-3">
          <div class="bg-primary-container/40 h-11 w-28 animate-pulse rounded-full" />
          <div class="bg-surface-container-high/60 h-11 w-44 animate-pulse rounded-full" />
        </div>
      </div>
    </article>
  );
}

function streamLabel(stream: VideoPlaybackStreamOption) {
  const tags = [
    stream.language?.toUpperCase() ?? null,
    stream.codec?.toUpperCase() ?? null,
    stream.isExternal ? 'External' : null,
    stream.isDefault ? 'Default' : null,
  ].filter((tag) => tag !== null);

  return tags.length > 0 ? `${stream.label} (${tags.join(', ')})` : stream.label;
}
