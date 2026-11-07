//! Review metadata shared by every content item.

use chrono::NaiveDate;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A citation for content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Citation {
    pub title: String,
    pub publisher: String,
    pub url: Option<String>,
}

/// Review state of a content item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatus {
    /// Drafted but not yet reviewed by the subject matter expert.
    Pending,
    /// Reviewed and accepted by the subject matter expert.
    Reviewed,
}

/// Who reviewed the item and when the next review is due.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Review {
    pub status: ReviewStatus,
    pub reviewed_by: Option<String>,
    pub reviewed_on: Option<String>,
    pub review_due: String,
}

impl Review {
    /// The review-due date, if it parses.
    pub fn due_date(&self) -> Option<NaiveDate> {
        NaiveDate::parse_from_str(&self.review_due, "%Y-%m-%d").ok()
    }

    /// True when the review is overdue on `today`.
    pub fn is_overdue(&self, today: NaiveDate) -> bool {
        self.due_date().is_none_or(|d| d < today)
    }
}

/// The `meta` block of every content file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ContentMeta {
    pub id: String,
    pub title: String,
    pub version: String,
    pub prepared: String,
    pub effective_date: Option<String>,
    pub source: Citation,
    pub related_sources: Vec<Citation>,
    pub review: Review,
    pub verification_note: String,
}

impl ContentMeta {
    /// Caveat text describing the review status, for tool responses.
    pub fn review_caveat(&self) -> Option<String> {
        match self.review.status {
            ReviewStatus::Reviewed => None,
            ReviewStatus::Pending => Some(format!(
                "'{}' (v{}) is a draft that has not yet been reviewed by the subject matter \
                 expert (review due {}). {}",
                self.title, self.version, self.review.review_due, self.verification_note
            )),
        }
    }
}
