mod lyrics;
mod search;

pub(super) use lyrics::{fetch_lyrics, LyRes, LySlot};
pub(super) use search::{yt_search, Hit, SearchMsg};
