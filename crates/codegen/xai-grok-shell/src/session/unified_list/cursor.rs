use std::cmp::{Ordering, Reverse};

use base64::Engine as _;
use serde::{Deserialize, Serialize};

use super::PartialReason;
use super::envelope::SessionKind;
use super::row::UnifiedRow;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(super) struct CompositeCursor {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boundary: Option<BoundaryKey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conv_page_token: Option<String>,
    #[serde(default)]
    pub conv_page_drained: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct BoundaryKey {
    pub updated_at: String,
    pub kind: SessionKind,
    pub session_id: String,
}

impl CompositeCursor {
    pub(super) fn decode(raw: Option<&str>) -> Self {
        raw.filter(|s| !s.is_empty())
            .and_then(|s| {
                base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .decode(s)
                    .ok()
            })
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub(super) fn encode(&self) -> String {
        let json = serde_json::to_vec(self).unwrap_or_default();
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json)
    }
}

pub(super) enum ConvLane {
    Skipped,
    Degraded(PartialReason),
    Page {
        rows: Vec<UnifiedRow>,
        next_token: Option<String>,
        frontier: Option<BoundaryKey>,
    },
}

pub(super) fn conv_frontier(raw_rows: &[UnifiedRow], has_more: bool) -> Option<BoundaryKey> {
    if !has_more {
        return None;
    }
    raw_rows
        .iter()
        .max_by(|a, b| cmp_total_order(a, b))
        .map(boundary_of)
}

pub(super) struct Paginated {
    pub candidates: Vec<UnifiedRow>,
    pub emit_count: usize,
    pub next_cursor: Option<CompositeCursor>,
    pub partial: Option<PartialReason>,
}

pub(super) fn merge_and_paginate(
    local: Vec<UnifiedRow>,
    conv: ConvLane,
    cursor: &CompositeCursor,
    limit: usize,
) -> Paginated {
    let (conv_rows, conv_next_token, conv_fetched, conv_frontier, partial) = match conv {
        ConvLane::Skipped => (Vec::new(), None, false, None, None),
        ConvLane::Degraded(reason) => (Vec::new(), None, false, None, Some(reason)),
        ConvLane::Page {
            rows,
            next_token,
            frontier,
        } => (rows, next_token, true, frontier, None),
    };

    let mut keyed: Vec<(SortKey, UnifiedRow)> = local
        .into_iter()
        .chain(conv_rows)
        .map(|row| (row_sort_key(&row), row))
        .collect();

    if let Some(boundary) = &cursor.boundary {
        let bkey = boundary_sort_key(boundary);
        keyed.retain(|(k, _)| k.cmp(&bkey) == Ordering::Greater);
    }

    keyed.sort_by(|(a, _), (b, _)| a.cmp(b));

    let mut emit_count = keyed.len().min(limit);
    if let Some(frontier) = &conv_frontier {
        let fkey = boundary_sort_key(frontier);
        let frontier_count = keyed
            .iter()
            .take_while(|(k, _)| k.cmp(&fkey) != Ordering::Greater)
            .count();
        emit_count = emit_count.min(frontier_count);
    }
    let new_boundary = (emit_count > 0).then(|| boundary_of(&keyed[emit_count - 1].1));

    let tail = &keyed[emit_count..];
    let local_has_more = tail.iter().any(|(_, r)| r.kind == SessionKind::Build);
    let conv_in_tail = tail.iter().any(|(_, r)| r.kind == SessionKind::Chat);

    let (next_conv_token, next_conv_drained, conv_has_more) = if conv_fetched {
        if conv_in_tail {
            (cursor.conv_page_token.clone(), false, true)
        } else {
            let has_more = conv_next_token.is_some();
            (conv_next_token, true, has_more)
        }
    } else if partial.is_some() && cursor.conv_page_token.is_some() && new_boundary.is_some() {
        (
            cursor.conv_page_token.clone(),
            cursor.conv_page_drained,
            true,
        )
    } else {
        (
            cursor.conv_page_token.clone(),
            cursor.conv_page_drained,
            false,
        )
    };

    let next_cursor = (local_has_more || conv_has_more).then(|| CompositeCursor {
        boundary: new_boundary.or_else(|| cursor.boundary.clone()),
        conv_page_token: next_conv_token,
        conv_page_drained: next_conv_drained,
    });

    let candidates: Vec<UnifiedRow> = keyed.into_iter().map(|(_, row)| row).collect();

    Paginated {
        candidates,
        emit_count,
        next_cursor,
        partial,
    }
}

type SortKey = (
    Reverse<Option<chrono::DateTime<chrono::FixedOffset>>>,
    SessionKind,
    String,
);

fn row_sort_key(row: &UnifiedRow) -> SortKey {
    (
        Reverse(row.sort_timestamp()),
        row.kind,
        row.legacy.session_id.clone(),
    )
}

fn boundary_sort_key(boundary: &BoundaryKey) -> SortKey {
    (
        Reverse(parse_ts(&boundary.updated_at)),
        boundary.kind,
        boundary.session_id.clone(),
    )
}

fn boundary_of(row: &UnifiedRow) -> BoundaryKey {
    BoundaryKey {
        updated_at: row.updated_at.clone().unwrap_or_default(),
        kind: row.kind,
        session_id: row.legacy.session_id.clone(),
    }
}

fn parse_ts(s: &str) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    chrono::DateTime::parse_from_rfc3339(s).ok()
}

pub(super) fn timestamp_desc(
    a: Option<chrono::DateTime<chrono::FixedOffset>>,
    b: Option<chrono::DateTime<chrono::FixedOffset>>,
) -> Ordering {
    match (a, b) {
        (Some(x), Some(y)) => y.cmp(&x),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

pub(super) fn cmp_total_order(a: &UnifiedRow, b: &UnifiedRow) -> Ordering {
    timestamp_desc(a.sort_timestamp(), b.sort_timestamp())
        .then_with(|| a.kind.cmp(&b.kind))
        .then_with(|| a.legacy.session_id.cmp(&b.legacy.session_id))
}
