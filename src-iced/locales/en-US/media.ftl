media-type-movie = Movie
media-type-series = Series
media-type-episode = Episode
media-type-season = Season
media-type-video = Video
media-type-box-set = Collection
media-type-music-video = Music video

media-item-caption = { $year } · { $type }
media-year = { $year }
media-premiere-date = { $month }/{ $day }/{ $year }
media-year-ongoing = { $year } - Present
media-year-range = { $year } - { $end }
media-episode-caption = { $code } - { $name }
media-hero-year-runtime-episode = { $year } · { $runtime } · { $episode }
media-hero-year-runtime = { $year } · { $runtime }
media-hero-year-episode = { $year } · { $episode }
media-hero-runtime-episode = { $runtime } · { $episode }

media-detail-metadata =
    { $has-year ->
        [yes] { $year } · { $type }
       *[no] { $type }
    }{ $has-genres ->
        [yes] { " · " }{ $genres }
       *[no] { "" }
    }{ $favorite ->
        [yes] { " · " }Favorite
       *[no] { "" }
    }
