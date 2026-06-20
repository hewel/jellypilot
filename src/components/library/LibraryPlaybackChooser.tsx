import { Play, X } from 'lucide-solid';
import { createEffect, createMemo, createSignal } from 'solid-js';

import type {
  VideoItemDetail,
  VideoLibraryPlayMode,
  VideoPlaybackStreamOption,
} from '../../bindings';
import { Button, Card, JmsrSelect } from '../ui';
import type { JmsrSelectItem } from '../ui';

const SUBTITLE_AUTO = 'auto';
const SUBTITLE_OFF = 'off';

export interface PendingLibraryPlayback {
  detail: VideoItemDetail;
  mode: VideoLibraryPlayMode;
  startPositionSeconds: number | null;
}

export interface LibraryPlaybackSelection {
  audioStreamIndex: number | null;
  subtitleStreamIndex: number | null;
}

export function LibraryPlaybackChooser(props: {
  pending: PendingLibraryPlayback;
  busy: boolean;
  onCancel: () => void;
  onConfirm: (selection: LibraryPlaybackSelection) => void;
}) {
  const audioItems = createMemo<JmsrSelectItem[]>(() => {
    const streams = props.pending.detail.audioStreams;
    if (streams.length === 0) {
      return [{ disabled: true, label: 'No audio tracks', value: '' }];
    }

    return streams.map((stream) => ({
      label: streamLabel(stream),
      value: String(stream.index),
    }));
  });
  const subtitleItems = createMemo<JmsrSelectItem[]>(() => [
    { label: 'Auto', value: SUBTITLE_AUTO },
    { label: 'Off', value: SUBTITLE_OFF },
    ...props.pending.detail.subtitleStreams.map((stream) => ({
      label: streamLabel(stream),
      value: String(stream.index),
    })),
  ]);
  const defaultAudioValue = createMemo(() => {
    const streams = props.pending.detail.audioStreams;
    const preferred = streams.find((stream) => stream.isDefault) ?? streams[0];
    return preferred ? String(preferred.index) : '';
  });
  const [audioValue, setAudioValue] = createSignal(defaultAudioValue());
  const [subtitleValue, setSubtitleValue] = createSignal(SUBTITLE_AUTO);

  createEffect(() => {
    props.pending.detail.id;
    props.pending.mode;
    setAudioValue(defaultAudioValue());
    setSubtitleValue(SUBTITLE_AUTO);
  });

  const audioStreamIndex = () => (audioValue() === '' ? null : Number(audioValue()));
  const subtitleStreamIndex = () => {
    const value = subtitleValue();
    if (value === SUBTITLE_AUTO) {
      return null;
    }
    if (value === SUBTITLE_OFF) {
      return -1;
    }
    return Number(value);
  };
  const confirmLabel = () =>
    props.pending.mode === 'resume' ? 'Resume playback' : 'Start playback';

  return (
    <Card
      as="section"
      variant="filled"
      class="border-secondary/40 bg-secondary-container/10 space-y-4"
      aria-labelledby="library-playback-chooser-title"
    >
      <div>
        <p class="text-secondary text-[11px] leading-[16px] font-bold tracking-[0.08em] uppercase">
          {props.pending.detail.itemType}
        </p>
        <h2
          id="library-playback-chooser-title"
          class="text-on-surface text-[22px] leading-[28px] font-bold"
        >
          {props.pending.detail.name}
        </h2>
      </div>

      <div class="grid gap-4 sm:grid-cols-2">
        <JmsrSelect
          label="Audio track"
          items={audioItems()}
          disabled={props.busy || props.pending.detail.audioStreams.length === 0}
          value={audioValue()}
          placeholder="No audio tracks"
          size="compact"
          onValueChange={setAudioValue}
        />

        <JmsrSelect
          label="Subtitle track"
          items={subtitleItems()}
          disabled={props.busy}
          value={subtitleValue()}
          size="compact"
          onValueChange={setSubtitleValue}
        />
      </div>

      <div class="flex flex-wrap justify-end gap-3">
        <Button
          type="button"
          variant="outlined"
          class="rounded-full"
          disabled={props.busy}
          onClick={props.onCancel}
          leadingIcon={<X class="h-4 w-4" />}
        >
          Cancel
        </Button>
        <Button
          type="button"
          variant="primary"
          class="rounded-full"
          disabled={props.busy}
          onClick={() =>
            props.onConfirm({
              audioStreamIndex: audioStreamIndex(),
              subtitleStreamIndex: subtitleStreamIndex(),
            })
          }
          leadingIcon={<Play class="h-4 w-4 fill-current" />}
        >
          {props.busy ? 'Starting...' : confirmLabel()}
        </Button>
      </div>
    </Card>
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
