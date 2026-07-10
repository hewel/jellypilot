import { Button, Card, Dialog, Selector } from '@jellypilot/ui';
import type { SelectorItem } from '@jellypilot/ui';
import { Play, X } from 'lucide-solid';
import { createEffect, createMemo, createSignal } from 'solid-js';

import type {
  VideoItemDetail,
  VideoLibraryPlayMode,
  VideoPlaybackStreamOption,
} from '../../bindings';

import * as styles from './LibraryPlaybackChooser.css';

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
  const audioItems = createMemo<SelectorItem[]>(() => {
    const streams = props.pending.detail.audioStreams;
    if (streams.length === 0) {
      return [];
    }

    return streams.map((stream) => ({
      label: streamLabel(stream),
      value: String(stream.index),
    }));
  });
  const subtitleItems = createMemo<SelectorItem[]>(() => [
    { label: 'Auto', value: SUBTITLE_AUTO },
    { label: 'Off', value: SUBTITLE_OFF },
    ...props.pending.detail.subtitleStreams.map((stream) => ({
      label: streamLabel(stream),
      value: String(stream.index),
    })),
  ]);
  const defaultAudioValue = () => {
    const streams = props.pending.detail.audioStreams;
    if (streams.length === 0) {
      return null;
    }

    const preferred = streams.find((stream) => stream.isDefault) ?? streams[0];
    return preferred ? String(preferred.index) : null;
  };
  const [audioValue, setAudioValue] = createSignal<string | null>(defaultAudioValue());
  const [subtitleValue, setSubtitleValue] = createSignal<string>(SUBTITLE_AUTO);

  createEffect(() => {
    props.pending.detail.id;
    props.pending.mode;
    setAudioValue(defaultAudioValue());
    setSubtitleValue(SUBTITLE_AUTO);
  });

  const audioStreamIndex = () => {
    const value = audioValue();
    return value === null ? null : Number(value);
  };
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
    <Dialog
      open={true}
      title={props.pending.detail.name}
      description="Choose playback target audio and subtitle tracks."
      onOpenChange={(next, _details) => {
        if (!next) {
          props.onCancel();
        }
      }}
      class={styles.content}
    >
      <Card variant="outlined" class={styles.card}>
        <div>
          <p class={styles.eyebrow}>{props.pending.detail.itemType}</p>
        </div>

        <div class={styles.fields}>
          <div>
            <span>Audio track</span>
            <Selector
              aria-label="Audio track"
              items={audioItems()}
              value={audioValue()}
              disabled={props.busy || props.pending.detail.audioStreams.length === 0}
              placeholder="No audio tracks"
              onValueChange={(value) => setAudioValue(value)}
            />
          </div>

          <div>
            <span>Subtitle track</span>
            <Selector
              aria-label="Subtitle track"
              items={subtitleItems()}
              value={subtitleValue()}
              disabled={props.busy}
              onValueChange={(value) => {
                if (value) setSubtitleValue(value);
              }}
            />
          </div>
        </div>

        <div class={styles.actions}>
          <Button
            type="button"
            variant="outline"
            class={styles.pillButton}
            disabled={props.busy}
            onClick={props.onCancel}
          >
            <X class={styles.icon} />
            Cancel
          </Button>
          <Button
            type="button"
            variant="primary"
            class={styles.pillButton}
            disabled={props.busy}
            onClick={() =>
              props.onConfirm({
                audioStreamIndex: audioStreamIndex(),
                subtitleStreamIndex: subtitleStreamIndex(),
              })
            }
          >
            <Play class={styles.playIcon} />
            {props.busy ? 'Starting...' : confirmLabel()}
          </Button>
        </div>
      </Card>
    </Dialog>
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
