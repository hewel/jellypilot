/// Session identity returned when authentication starts.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SessionToken(u64);

/// Identity of one Home request inside an authenticated session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HomeToken {
    session: SessionToken,
    sequence: u64,
}

/// Identity of one detail request inside an authenticated session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DetailToken {
    session: SessionToken,
    sequence: u64,
}

/// Kind of auxiliary request attached to the current detail item.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DetailAuxKind {
    Streams,
    Recommendations,
    SeasonNeighbors,
    SimilarItems,
}

/// Identity of one detail-auxiliary request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DetailAuxToken {
    session: SessionToken,
    kind: DetailAuxKind,
    sequence: u64,
    item_id: String,
}

/// Identity of one remote-control lifecycle generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RemoteToken(u64);

/// Identity of one remote Play-item fetch inside a remote generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RemotePlayToken {
    remote: RemoteToken,
    sequence: u64,
}

/// Identity of one image-cache stats or clear request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImageCacheToken(u64);

/// Display-free gate for shell async request families.
#[derive(Clone, Debug, Default)]
pub struct RequestGate {
    session: u64,
    login: Option<SessionToken>,
    home_sequence: u64,
    home: Option<HomeToken>,
    detail_sequence: u64,
    detail: Option<DetailToken>,
    detail_item: Option<String>,
    aux_sequence: [u64; 4],
    aux_live: [bool; 4],
    remote: u64,
    remote_play: u64,
    image_cache: u64,
    image_cache_live: bool,
}

impl DetailAuxKind {
    const fn index(self) -> usize {
        match self {
            Self::Streams => 0,
            Self::Recommendations => 1,
            Self::SeasonNeighbors => 2,
            Self::SimilarItems => 3,
        }
    }
}

impl RequestGate {
    pub fn begin_login(&mut self) -> SessionToken {
        self.advance_session();
        let token = SessionToken(self.session);
        self.login = Some(token);
        token
    }

    #[must_use]
    pub fn finish_login(&mut self, token: SessionToken) -> bool {
        if self.login != Some(token) {
            return false;
        }
        self.login = None;
        true
    }

    #[must_use]
    pub fn is_current_login(&self, token: SessionToken) -> bool {
        self.login == Some(token) && token == SessionToken(self.session)
    }

    pub fn disconnect(&mut self) {
        self.advance_session();
    }

    pub fn begin_home(&mut self) -> HomeToken {
        self.home_sequence = self.home_sequence.saturating_add(1);
        let token = HomeToken {
            session: SessionToken(self.session),
            sequence: self.home_sequence,
        };
        self.home = Some(token);
        token
    }

    #[must_use]
    pub fn finish_home(&mut self, token: HomeToken) -> bool {
        if self.home != Some(token) || token.session != SessionToken(self.session) {
            return false;
        }
        self.home = None;
        true
    }

    pub fn begin_detail(&mut self) -> DetailToken {
        self.detail_sequence = self.detail_sequence.saturating_add(1);
        let token = DetailToken {
            session: SessionToken(self.session),
            sequence: self.detail_sequence,
        };
        self.detail = Some(token);
        token
    }

    #[must_use]
    pub fn finish_detail(&mut self, token: DetailToken) -> bool {
        if self.detail != Some(token) || token.session != SessionToken(self.session) {
            return false;
        }
        self.detail = None;
        true
    }

    /// Abandons in-flight detail and season-page loads while the displayed detail
    /// stays on screen (season-back, parent restore). Aux families stay valid.
    pub fn cancel_detail_loads(&mut self) {
        self.detail = None;
    }

    /// Leaves the detail view entirely: detail loads and aux families go stale.
    pub fn navigate(&mut self) {
        self.detail = None;
        self.detail_item = None;
        for sequence in &mut self.aux_sequence {
            *sequence = sequence.saturating_add(1);
        }
        self.aux_live = [false; 4];
    }

    /// Invalidates one aux family regardless of whether a detail item is set.
    pub fn invalidate_detail_aux(&mut self, kind: DetailAuxKind) {
        let index = kind.index();
        self.aux_sequence[index] = self.aux_sequence[index].saturating_add(1);
        self.aux_live[index] = false;
    }

    #[must_use]
    pub const fn current_session(&self) -> SessionToken {
        SessionToken(self.session)
    }

    #[must_use]
    pub const fn is_current_session(&self, token: SessionToken) -> bool {
        token.0 == self.session
    }

    /// Records the item the current detail view presents.
    pub fn set_detail_item(&mut self, item_id: Option<String>) {
        self.detail_item = item_id;
    }

    /// Mints an aux token for the current detail item; None when no detail item is set.
    pub fn begin_detail_aux(&mut self, kind: DetailAuxKind) -> Option<DetailAuxToken> {
        let item_id = self.detail_item.clone()?;
        let index = kind.index();
        self.aux_sequence[index] = self.aux_sequence[index].saturating_add(1);
        self.aux_live[index] = true;
        Some(DetailAuxToken {
            session: SessionToken(self.session),
            kind,
            sequence: self.aux_sequence[index],
            item_id,
        })
    }

    /// True only when session, kind sequence, and item all match. Season paging
    /// remints DetailTokens without invalidating aux families.
    #[must_use]
    pub fn finish_detail_aux(&mut self, token: DetailAuxToken) -> bool {
        if !self.is_current_detail_aux(&token) {
            return false;
        }
        self.aux_live[token.kind.index()] = false;
        true
    }

    pub fn begin_remote(&mut self) -> RemoteToken {
        self.remote = self.remote.saturating_add(1);
        self.remote_play = 0;
        RemoteToken(self.remote)
    }

    #[must_use]
    pub const fn is_current_remote(&self, token: RemoteToken) -> bool {
        token.0 == self.remote
    }

    pub fn begin_remote_play(&mut self) -> RemotePlayToken {
        self.remote_play = self.remote_play.saturating_add(1);
        RemotePlayToken {
            remote: RemoteToken(self.remote),
            sequence: self.remote_play,
        }
    }

    #[must_use]
    pub const fn is_current_remote_play(&self, token: RemotePlayToken) -> bool {
        token.remote.0 == self.remote && token.sequence == self.remote_play
    }

    pub fn begin_image_cache(&mut self) -> ImageCacheToken {
        self.image_cache = self.image_cache.saturating_add(1);
        self.image_cache_live = true;
        ImageCacheToken(self.image_cache)
    }

    #[must_use]
    pub fn finish_image_cache(&mut self, token: ImageCacheToken) -> bool {
        if !self.image_cache_live || token.0 != self.image_cache {
            return false;
        }
        self.image_cache_live = false;
        true
    }

    fn is_current_detail_aux(&self, token: &DetailAuxToken) -> bool {
        let index = token.kind.index();
        token.session == SessionToken(self.session)
            && self.aux_live[index]
            && token.sequence == self.aux_sequence[index]
            && self.detail_item.as_deref() == Some(token.item_id.as_str())
    }

    fn advance_session(&mut self) {
        self.session = self.session.saturating_add(1);
        self.login = None;
        self.home = None;
        self.detail = None;
        self.detail_item = None;
        self.aux_live = [false; 4];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_completion_after_disconnect_is_rejected() {
        let mut gate = RequestGate::default();
        let login = gate.begin_login();
        gate.disconnect();

        assert!(!gate.finish_login(login));
    }

    #[test]
    fn earlier_login_completion_after_relogin_is_rejected() {
        let mut gate = RequestGate::default();
        let earlier = gate.begin_login();
        let current = gate.begin_login();

        assert!(!gate.finish_login(earlier));
        assert!(gate.finish_login(current));
    }

    #[test]
    fn login_progress_is_current_only_for_the_active_login_generation() {
        let mut gate = RequestGate::default();
        let earlier = gate.begin_login();
        let current = gate.begin_login();

        assert!(!gate.is_current_login(earlier));
        assert!(gate.is_current_login(current));

        gate.disconnect();
        assert!(!gate.is_current_login(current));
    }

    #[test]
    fn detail_completion_after_navigation_is_rejected() {
        let mut gate = RequestGate::default();
        let detail = gate.begin_detail();
        gate.navigate();

        assert!(!gate.finish_detail(detail));
    }

    #[test]
    fn earlier_detail_completion_after_new_selection_is_rejected() {
        let mut gate = RequestGate::default();
        let earlier = gate.begin_detail();
        let current = gate.begin_detail();

        assert!(!gate.finish_detail(earlier));
        assert!(gate.finish_detail(current));
    }

    #[test]
    fn earlier_home_completion_after_retry_is_rejected() {
        let mut gate = RequestGate::default();
        let earlier = gate.begin_home();
        let current = gate.begin_home();

        assert!(!gate.finish_home(earlier));
        assert!(gate.finish_home(current));
    }

    #[test]
    fn home_completion_after_disconnect_is_rejected() {
        let mut gate = RequestGate::default();
        let home = gate.begin_home();
        gate.disconnect();

        assert!(!gate.finish_home(home));
    }

    #[test]
    fn detail_token_can_gate_a_nested_season_request() {
        let mut gate = RequestGate::default();
        let show = gate.begin_detail();
        assert!(gate.finish_detail(show));

        let season = gate.begin_detail();
        assert!(gate.finish_detail(season));
    }

    #[test]
    fn detail_aux_kinds_are_isolated() {
        let mut gate = RequestGate::default();
        gate.set_detail_item(Some("item-1".to_owned()));
        let streams = gate
            .begin_detail_aux(DetailAuxKind::Streams)
            .expect("streams aux requires a detail item");
        let recommendations = gate
            .begin_detail_aux(DetailAuxKind::Recommendations)
            .expect("recommendations aux requires a detail item");

        assert!(gate.finish_detail_aux(streams));
        assert!(gate.finish_detail_aux(recommendations));
    }

    #[test]
    fn detail_aux_item_mismatch_is_rejected() {
        let mut gate = RequestGate::default();
        gate.set_detail_item(Some("item-1".to_owned()));
        let token = gate
            .begin_detail_aux(DetailAuxKind::Streams)
            .expect("streams aux requires a detail item");
        gate.set_detail_item(Some("item-2".to_owned()));

        assert!(!gate.finish_detail_aux(token));
    }

    #[test]
    fn navigate_rejects_detail_aux_after_reopening_the_same_item() {
        let mut gate = RequestGate::default();
        gate.set_detail_item(Some("item-1".to_owned()));
        let similar = gate
            .begin_detail_aux(DetailAuxKind::SimilarItems)
            .expect("similar-items aux requires a detail item");
        let season_neighbors = gate
            .begin_detail_aux(DetailAuxKind::SeasonNeighbors)
            .expect("season-neighbor aux requires a detail item");
        gate.navigate();
        gate.set_detail_item(Some("item-1".to_owned()));

        assert!(!gate.finish_detail_aux(similar));
        assert!(!gate.finish_detail_aux(season_neighbors));
    }

    #[test]
    fn season_back_keeps_displayed_detail_aux_valid() {
        let mut gate = RequestGate::default();
        gate.set_detail_item(Some("show-1".to_owned()));
        gate.begin_detail();
        let streams = gate
            .begin_detail_aux(DetailAuxKind::Streams)
            .expect("streams aux requires a detail item");

        gate.cancel_detail_loads();

        assert!(gate.finish_detail_aux(streams));
        assert!(gate.begin_detail_aux(DetailAuxKind::SimilarItems).is_some());
    }

    #[test]
    fn season_paging_does_not_invalidate_detail_aux() {
        let mut gate = RequestGate::default();
        gate.set_detail_item(Some("show-1".to_owned()));
        gate.begin_detail();
        let recommendations = gate
            .begin_detail_aux(DetailAuxKind::Recommendations)
            .expect("recommendations aux requires a detail item");

        gate.begin_detail();

        assert!(gate.finish_detail_aux(recommendations));
    }

    #[test]
    fn same_kind_aux_is_latest_wins() {
        let mut gate = RequestGate::default();
        gate.set_detail_item(Some("show-1".to_owned()));
        let earlier = gate
            .begin_detail_aux(DetailAuxKind::Streams)
            .expect("streams aux requires a detail item");
        let current = gate
            .begin_detail_aux(DetailAuxKind::Streams)
            .expect("streams aux requires a detail item");

        assert!(!gate.finish_detail_aux(earlier));
        assert!(gate.finish_detail_aux(current));
    }

    #[test]
    fn invalidate_detail_aux_rejects_outstanding_token() {
        let mut gate = RequestGate::default();
        gate.set_detail_item(Some("show-1".to_owned()));
        let similar = gate
            .begin_detail_aux(DetailAuxKind::SimilarItems)
            .expect("similar-items aux requires a detail item");
        gate.invalidate_detail_aux(DetailAuxKind::SimilarItems);

        assert!(!gate.finish_detail_aux(similar));
    }

    #[test]
    fn disconnect_invalidates_session_families_but_not_remote_or_image_cache() {
        let mut gate = RequestGate::default();
        let login = gate.begin_login();
        let session = gate.current_session();
        let home = gate.begin_home();
        let detail = gate.begin_detail();
        gate.set_detail_item(Some("item-1".to_owned()));
        let aux = gate
            .begin_detail_aux(DetailAuxKind::SeasonNeighbors)
            .expect("season-neighbor aux requires a detail item");
        let remote = gate.begin_remote();
        let play = gate.begin_remote_play();
        let image = gate.begin_image_cache();

        gate.disconnect();

        assert!(!gate.is_current_session(session));
        assert!(!gate.finish_login(login));
        assert!(!gate.finish_home(home));
        assert!(!gate.finish_detail(detail));
        assert!(!gate.finish_detail_aux(aux));
        assert!(gate.is_current_remote(remote));
        assert!(gate.is_current_remote_play(play));
        assert!(gate.finish_image_cache(image));
    }

    #[test]
    fn begin_remote_invalidates_outstanding_play_tokens() {
        let mut gate = RequestGate::default();
        let remote = gate.begin_remote();
        let play = gate.begin_remote_play();
        assert!(gate.is_current_remote(remote));
        assert!(gate.is_current_remote_play(play));

        let next_remote = gate.begin_remote();
        assert!(!gate.is_current_remote(remote));
        assert!(!gate.is_current_remote_play(play));
        assert!(gate.is_current_remote(next_remote));

        let next_play = gate.begin_remote_play();
        assert!(gate.is_current_remote_play(next_play));
    }

    #[test]
    fn earlier_image_cache_completion_after_retry_is_rejected() {
        let mut gate = RequestGate::default();
        let earlier = gate.begin_image_cache();
        let current = gate.begin_image_cache();

        assert!(!gate.finish_image_cache(earlier));
        assert!(gate.finish_image_cache(current));
    }

    #[test]
    fn stateful_property_preserves_session_remote_and_aux_invariants() {
        use crate::prop_test::{check_stateful, Rng};

        #[derive(Clone, Debug)]
        enum Action {
            BeginLogin,
            FinishLogin(usize),
            Disconnect,
            BeginHome,
            FinishHome(usize),
            BeginDetail,
            FinishDetail(usize),
            CancelDetailLoads,
            Navigate,
            SetDetailItem(Option<u8>),
            BeginDetailAux(u8),
            FinishDetailAux(usize),
            InvalidateDetailAux(u8),
            BeginRemote,
            BeginRemotePlay,
            BeginImageCache,
            FinishImageCache(usize),
        }

        fn item_name(idx: Option<u8>) -> Option<String> {
            idx.map(|n| format!("item-{n}"))
        }

        const AUX_KINDS: [DetailAuxKind; 4] = [
            DetailAuxKind::Streams,
            DetailAuxKind::Recommendations,
            DetailAuxKind::SeasonNeighbors,
            DetailAuxKind::SimilarItems,
        ];

        check_stateful(
            256,
            100,
            |rng: &mut Rng| match rng.below(17) {
                0 => Action::BeginLogin,
                1 => Action::FinishLogin(rng.below(8)),
                2 => Action::Disconnect,
                3 => Action::BeginHome,
                4 => Action::FinishHome(rng.below(8)),
                5 => Action::BeginDetail,
                6 => Action::FinishDetail(rng.below(8)),
                7 => Action::CancelDetailLoads,
                8 => Action::Navigate,
                9 => Action::SetDetailItem(match rng.below(3) {
                    0 => None,
                    1 => Some(0),
                    _ => Some(1),
                }),
                10 => Action::BeginDetailAux(rng.below(4) as u8),
                11 => Action::FinishDetailAux(rng.below(8)),
                12 => Action::InvalidateDetailAux(rng.below(4) as u8),
                13 => Action::BeginRemote,
                14 => Action::BeginRemotePlay,
                15 => Action::BeginImageCache,
                _ => Action::FinishImageCache(rng.below(8)),
            },
            |actions| {
                let mut gate = RequestGate::default();
                let mut session: u64 = 0;
                let mut active_login: Option<SessionToken> = None;
                let mut active_home: Option<HomeToken> = None;
                let mut active_detail: Option<DetailToken> = None;
                let mut detail_item: Option<String> = None;
                let mut aux_seq = [0_u64; 4];
                let mut aux_live = [false; 4];
                let mut active_remote: Option<RemoteToken> = None;
                let mut active_remote_play: Option<RemotePlayToken> = None;
                let mut active_image: Option<ImageCacheToken> = None;

                let mut logins = Vec::new();
                let mut homes = Vec::new();
                let mut details = Vec::new();
                let mut aux_tokens: Vec<(DetailAuxToken, u64, u64)> = Vec::new();
                let mut remotes = Vec::new();
                let mut remote_plays = Vec::new();
                let mut images = Vec::new();

                for (step, action) in actions.iter().enumerate() {
                    match action {
                        Action::BeginLogin => {
                            let token = gate.begin_login();
                            session += 1;
                            active_login = Some(token);
                            active_home = None;
                            active_detail = None;
                            detail_item = None;
                            aux_live = [false; 4];
                            logins.push(token);
                            if !gate.is_current_login(token) {
                                return Err(format!("step {step}: newly begun login not current"));
                            }
                            for old in &homes {
                                if gate.finish_home(*old) {
                                    return Err(format!("step {step}: old home survived login"));
                                }
                            }
                            for old in &details {
                                if gate.finish_detail(*old) {
                                    return Err(format!("step {step}: old detail survived login"));
                                }
                            }
                        }
                        Action::FinishLogin(pick) => {
                            if !logins.is_empty() {
                                let tok = logins[*pick % logins.len()];
                                let expected = active_login == Some(tok);
                                let got = gate.finish_login(tok);
                                if got != expected {
                                    return Err(format!(
                                        "step {step}: finish_login expected {expected}, got {got}"
                                    ));
                                }
                                if got {
                                    active_login = None;
                                    if gate.finish_login(tok) {
                                        return Err(format!(
                                            "step {step}: login token settled twice"
                                        ));
                                    }
                                }
                            }
                        }
                        Action::Disconnect => {
                            gate.disconnect();
                            session += 1;
                            active_login = None;
                            active_home = None;
                            active_detail = None;
                            detail_item = None;
                            aux_live = [false; 4];
                            for old in &logins {
                                if gate.is_current_login(*old) {
                                    return Err(format!(
                                        "step {step}: login stayed current after disconnect"
                                    ));
                                }
                            }
                            if let Some(remote) = active_remote {
                                if !gate.is_current_remote(remote) {
                                    return Err(format!(
                                        "step {step}: disconnect invalidated remote"
                                    ));
                                }
                            }
                            if let Some(play) = active_remote_play {
                                if !gate.is_current_remote_play(play) {
                                    return Err(format!(
                                        "step {step}: disconnect invalidated remote_play"
                                    ));
                                }
                            }
                        }
                        Action::BeginHome => {
                            let tok = gate.begin_home();
                            active_home = Some(tok);
                            homes.push(tok);
                        }
                        Action::FinishHome(pick) => {
                            if !homes.is_empty() {
                                let tok = homes[*pick % homes.len()];
                                let expected = active_home == Some(tok);
                                let got = gate.finish_home(tok);
                                if got != expected {
                                    return Err(format!(
                                        "step {step}: finish_home expected {expected}, got {got}"
                                    ));
                                }
                                if got {
                                    active_home = None;
                                    if gate.finish_home(tok) {
                                        return Err(format!("step {step}: home settled twice"));
                                    }
                                }
                            }
                        }
                        Action::BeginDetail => {
                            let tok = gate.begin_detail();
                            active_detail = Some(tok);
                            details.push(tok);
                        }
                        Action::FinishDetail(pick) => {
                            if !details.is_empty() {
                                let tok = details[*pick % details.len()];
                                let expected = active_detail == Some(tok);
                                let got = gate.finish_detail(tok);
                                if got != expected {
                                    return Err(format!(
                                        "step {step}: finish_detail expected {expected}, got {got}"
                                    ));
                                }
                                if got {
                                    active_detail = None;
                                    if gate.finish_detail(tok) {
                                        return Err(format!("step {step}: detail settled twice"));
                                    }
                                }
                            }
                        }
                        Action::CancelDetailLoads => {
                            gate.cancel_detail_loads();
                            active_detail = None;
                        }
                        Action::Navigate => {
                            gate.navigate();
                            active_detail = None;
                            detail_item = None;
                            for seq in &mut aux_seq {
                                *seq += 1;
                            }
                            aux_live = [false; 4];
                        }
                        Action::SetDetailItem(item) => {
                            let next = item_name(*item);
                            gate.set_detail_item(next.clone());
                            detail_item = next;
                        }
                        Action::BeginDetailAux(kind_idx) => {
                            let kind = AUX_KINDS[*kind_idx as usize];
                            let got = gate.begin_detail_aux(kind);
                            match (&detail_item, got) {
                                (None, Some(_)) => {
                                    return Err(format!(
                                        "step {step}: minted detail aux with no detail_item"
                                    ));
                                }
                                (Some(_), None) => {
                                    return Err(format!(
                                        "step {step}: failed to mint detail aux when detail_item set"
                                    ));
                                }
                                (Some(_), Some(tok)) => {
                                    let idx = *kind_idx as usize;
                                    aux_seq[idx] += 1;
                                    aux_live[idx] = true;
                                    aux_tokens.push((tok, session, aux_seq[idx]));
                                }
                                (None, None) => {}
                            }
                        }
                        Action::FinishDetailAux(pick) => {
                            if !aux_tokens.is_empty() {
                                let (tok, tok_session, tok_seq) =
                                    aux_tokens[*pick % aux_tokens.len()].clone();
                                let idx = match tok.kind {
                                    DetailAuxKind::Streams => 0,
                                    DetailAuxKind::Recommendations => 1,
                                    DetailAuxKind::SeasonNeighbors => 2,
                                    DetailAuxKind::SimilarItems => 3,
                                };
                                let expected = tok_session == session
                                    && aux_live[idx]
                                    && aux_seq[idx] == tok_seq
                                    && detail_item.as_deref() == Some(tok.item_id.as_str());
                                let got = gate.finish_detail_aux(tok.clone());
                                if got != expected {
                                    return Err(format!(
                                        "step {step}: finish_detail_aux expected {expected}, got {got}"
                                    ));
                                }
                                if got {
                                    aux_live[idx] = false;
                                    if gate.finish_detail_aux(tok) {
                                        return Err(format!(
                                            "step {step}: detail aux settled twice"
                                        ));
                                    }
                                }
                            }
                        }
                        Action::InvalidateDetailAux(kind_idx) => {
                            let kind = AUX_KINDS[*kind_idx as usize];
                            gate.invalidate_detail_aux(kind);
                            let idx = *kind_idx as usize;
                            aux_seq[idx] += 1;
                            aux_live[idx] = false;
                        }
                        Action::BeginRemote => {
                            let tok = gate.begin_remote();
                            active_remote = Some(tok);
                            active_remote_play = None;
                            remotes.push(tok);
                            for old in &remotes[..remotes.len() - 1] {
                                if gate.is_current_remote(*old) {
                                    return Err(format!(
                                        "step {step}: prior remote survived begin_remote"
                                    ));
                                }
                            }
                            for old_play in &remote_plays {
                                if gate.is_current_remote_play(*old_play) {
                                    return Err(format!(
                                        "step {step}: prior remote_play survived begin_remote"
                                    ));
                                }
                            }
                        }
                        Action::BeginRemotePlay => {
                            let tok = gate.begin_remote_play();
                            active_remote_play = Some(tok);
                            remote_plays.push(tok);
                            for old_play in &remote_plays[..remote_plays.len() - 1] {
                                if gate.is_current_remote_play(*old_play) {
                                    return Err(format!(
                                        "step {step}: prior remote_play survived new play"
                                    ));
                                }
                            }
                        }
                        Action::BeginImageCache => {
                            let tok = gate.begin_image_cache();
                            active_image = Some(tok);
                            images.push(tok);
                        }
                        Action::FinishImageCache(pick) => {
                            if !images.is_empty() {
                                let tok = images[*pick % images.len()];
                                let expected = active_image == Some(tok);
                                let got = gate.finish_image_cache(tok);
                                if got != expected {
                                    return Err(format!(
                                        "step {step}: finish_image_cache expected {expected}, got {got}"
                                    ));
                                }
                                if got {
                                    active_image = None;
                                    if gate.finish_image_cache(tok) {
                                        return Err(format!(
                                            "step {step}: image cache settled twice"
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(())
            },
        );
    }
}
