media-type-movie = 电影
media-type-series = 剧集
media-type-episode = 单集
media-type-season = 季
media-type-video = 视频
media-type-box-set = 合集
media-type-music-video = 音乐视频

media-item-caption = { $year } · { $type }
media-year = { $year }
media-premiere-date = { $year }年{ $month }月{ $day }日
media-year-ongoing = { $year } - 至今
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
        [yes] { " · " }已收藏
       *[no] { "" }
    }
