import EmbeddedPlayer from '@components/EmbeddedPlayer';
import type { EmbeddedPlayerEffects, EmbeddedPlayerViewModel } from '@components/EmbeddedPlayer';
import { useEmbeddedPlayer } from '@components/EmbeddedPlayerProvider';
import { createFileRoute, useNavigate } from '@tanstack/solid-router';
import { createEffect } from 'solid-js';
import type { Accessor } from 'solid-js';

import { AUTHENTICATED_HOME_ROUTE } from '../../router-guards';
import * as styles from './player.styles';

export const Route = createFileRoute('/_authenticated/player')({
  component: PlayerRoute,
});

/**
 * Route-level injection seam for the playback integration. The parent wiring owns
 * the reactive view model and effects so this display component stays transport-agnostic.
 */
export function EmbeddedPlayerRouteContent(props: {
  player: Accessor<EmbeddedPlayerViewModel | null>;
  effects?: EmbeddedPlayerEffects;
}) {
  return (
    <main class={styles.route}>
      <EmbeddedPlayer player={props.player} effects={props.effects} />
    </main>
  );
}

function PlayerRoute() {
  const embeddedPlayer = useEmbeddedPlayer();
  const navigate = useNavigate();
  const effects: EmbeddedPlayerEffects = {
    onControl: embeddedPlayer.control,
    onObservation: embeddedPlayer.observe,
    onPlayInMpv: embeddedPlayer.playInMpv,
  };

  createEffect(() => {
    if (embeddedPlayer.settled() && embeddedPlayer.player() === null) {
      void navigate({ to: AUTHENTICATED_HOME_ROUTE, replace: true });
    }
  });

  return <EmbeddedPlayerRouteContent player={embeddedPlayer.player} effects={effects} />;
}
