//! A tab as the settings keep it, and as their file holds it.
use colet_core::entry::Order;
use serde::{Deserialize, Serialize};

use super::is_default_order;
use crate::{
    kept_path::KeptPath,
    state::{Fold, Kept},
};

/// A tab kept between starts: where it was, and the filter its next search is narrowed by, by name.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(from = "TabFile", into = "TabFile")]
pub struct KeptTab {
    pub place: Kept,
    pub filter: Option<String>,
}

/// A tab as the file holds it: `{"query": ..., "search_filter": ..., "search_order": {"by": ..., "descending":
/// ...}, "filter": ..., "folders": [{"path": ..., "highlight": ...}], "picks": [...], "active": ..., "fold":
/// {"anchor": ..., "open": ...}}`, what it lacks, and the default order, left out.
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct TabFile {
    #[serde(skip_serializing_if = "String::is_empty")]
    query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    search_filter: Option<String>,
    #[serde(skip_serializing_if = "is_default_order")]
    search_order: Order,
    #[serde(skip_serializing_if = "Option::is_none")]
    filter: Option<String>,
    folders: Vec<FolderFile>,
    picks: Vec<Option<KeptPath>>,
    active: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    fold: Option<Fold<usize>>,
}

#[derive(Serialize, Deserialize)]
struct FolderFile {
    path: KeptPath,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    highlight: Option<KeptPath>,
}

impl From<TabFile> for KeptTab {
    fn from(file: TabFile) -> Self {
        let folders: Vec<_> = file.folders.into_iter().map(|f| (f.path.0, f.highlight.map(|h| h.0))).collect();
        let place = Kept {
            query: file.query,
            search_filter: file.search_filter,
            search_order: file.search_order,
            fold: file.fold.filter(|fold| fold.anchor < folders.len()),
            folders,
            picks: file.picks.into_iter().map(|pick| pick.map(|p| p.0)).collect(),
            active: file.active,
        };
        KeptTab { place, filter: file.filter }
    }
}

impl From<KeptTab> for TabFile {
    fn from(tab: KeptTab) -> Self {
        let place = tab.place;
        TabFile {
            query: place.query,
            search_filter: place.search_filter,
            search_order: place.search_order,
            filter: tab.filter,
            folders: place
                .folders
                .into_iter()
                .map(|(path, highlight)| FolderFile { path: KeptPath(path), highlight: highlight.map(KeptPath) })
                .collect(),
            picks: place.picks.into_iter().map(|pick| pick.map(KeptPath)).collect(),
            active: place.active,
            fold: place.fold,
        }
    }
}
