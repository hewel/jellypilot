import { Play, X } from 'lucide-solid';
import { createEffect, createMemo, createSignal } from 'solid-js';
import { Portal } from 'solid-js/web';

import type {
  VideoItemDetail,
  VideoLibraryPlayMode,
  VideoPlaybackStreamOption,
} from '../../bindings';
import { Button, Card, Dialog, JellyPilotSelect } from '../ui';
import type { JellyPilotSelectItem } from '../ui';

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
  const audioItems = createMemo<JellyPilotSelectItem[]>(() => {
    const streams = props.pending.detail.audioStreams;
    if (streams.length === 0) {
      return [{ disabled: true, label: 'No audio tracks', value: '' }];
    }

    return streams.map((stream) => ({
      label: streamLabel(stream),
      value: String(stream.index),
    }));
  });
  const subtitleItems = createMemo<JellyPilotSelectItem[]>(() => [
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
    <Dialog.Root
      open={true}
      onOpenChange={(event) => {
        if (!event.open) {
          props.onCancel();
        }
      }}
      lazyMount
      unmountOnExit
    >
      <Portal>
        <Dialog.Backdrop class={styles.backdrop} />
        <Dialog.Positioner class={`${styles.positioner} ${styles.positionerFill}`}>
          <Dialog.Content class={styles.content}>
            <Card as="section" variant="filled" class={styles.card}>
              <div>
                <p class={styles.eyebrow}>{props.pending.detail.itemType}</p>
                <Dialog.Title class={styles.title}>{props.pending.detail.name}</Dialog.Title>
              </div>

              <div class={styles.fields}>
                <JellyPilotSelect
                  label="Audio track"
                  items={audioItems()}
                  disabled={props.busy || props.pending.detail.audioStreams.length === 0}
                  value={audioValue()}
                  placeholder="No audio tracks"
                  size="compact"
                  onValueChange={setAudioValue}
                />

                <JellyPilotSelect
                  label="Subtitle track"
                  items={subtitleItems()}
                  disabled={props.busy}
                  value={subtitleValue()}
                  size="compact"
                  onValueChange={setSubtitleValue}
                />
              </div>

              <div class={styles.actions}>
                <Dialog.CloseTrigger class={styles.closeButton} disabled={props.busy}>
                  <X class={styles.icon} />
                  Cancel
                </Dialog.CloseTrigger>
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
                  leadingIcon={<Play class={styles.playIcon} />}
                >
                  {props.busy ? 'Starting...' : confirmLabel()}
                </Button>
              </div>
            </Card>
          </Dialog.Content>
        </Dialog.Positioner>
      </Portal>
    </Dialog.Root>
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
