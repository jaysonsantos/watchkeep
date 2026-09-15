//! Filter and sort state of the movie and show lists. `frontend/src/lib/lists.ts` holds the browser copy.

text_enum! {
    WatchFilter { All => "all", Watched => "watched", Unwatched => "unwatched" }
}

impl WatchFilter {
    pub const DEFAULT: Self = Self::All;

    /// Whether the list keeps only watched items, and whether it keeps only unwatched items.
    pub fn flags(self) -> (bool, bool) {
        (self == Self::Watched, self == Self::Unwatched)
    }
}

text_enum! {
    /// List order for movies and shows. `Recent` puts the last watched first, then the newest additions.
    SortOrder { Recent => "recent", Title => "title", Year => "year", Added => "added" }
}

impl SortOrder {
    pub const DEFAULT: Self = Self::Recent;
}

/// The sort as booleans, so that one static query can order by any of them.
#[derive(Clone, Copy, Debug, Default)]
pub struct SortFlags {
    pub recent: bool,
    pub title: bool,
    pub year: bool,
    pub added: bool,
}

impl From<SortOrder> for SortFlags {
    fn from(sort: SortOrder) -> Self {
        Self {
            recent: sort == SortOrder::Recent,
            title: sort == SortOrder::Title,
            year: sort == SortOrder::Year,
            added: sort == SortOrder::Added,
        }
    }
}
