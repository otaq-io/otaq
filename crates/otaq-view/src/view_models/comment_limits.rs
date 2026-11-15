//! The ticket comment limits the app's comment box needs (CMT-02, CMT-03, CMT-11; d0109; tuq 112): the 20 MB report
//! limit, and the one ceiling left on the text, what one message to the service carries. There is no character limit
//! here: the user's comments have none (only agents' have, and the app's boxes are the user's). All read from core (one place), so
//! the page never keeps copies of its own. A view that has a comment box carries [`CommentLimits::current`].

use serde::Serialize;
use ts_rs::TS;

use otaq_core::board::reports::{REPORT_LIMIT_BYTES, size_label};
use otaq_core::protocol::{MAX_FRAME_BYTES, max_frame_label};

/// Room left in a message for everything but the comment's text: the envelope, the ticket's key, the attachments'
/// paths and an edit's numbers. Far more than they take.
const ENVELOPE_BYTES: u64 = 64 * 1024;

/// The limits, as numbers and as the size people read ("20 MB").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct CommentLimits {
    /// The largest report, in bytes: 20 × 1024 × 1024.
    pub report_bytes: u64,
    /// "20 MB".
    pub report_label: String,
    /// The most the comment's text may take as sent, in bytes of its JSON string (UTF-8,
    /// quotes and escapes included): one service message (16 MB) less room for the rest of it.
    pub message_bytes: u64,
    /// What the box says when its text would not fit in one message, instead of sending it.
    pub too_long_to_send: String,
}

impl CommentLimits {
    pub fn current() -> Self {
        Self {
            report_bytes: REPORT_LIMIT_BYTES,
            report_label: size_label(REPORT_LIMIT_BYTES),
            message_bytes: MAX_FRAME_BYTES as u64 - ENVELOPE_BYTES,
            too_long_to_send: format!(
                "This comment is too long to send: one message carries at most {}. Attach the text as a file \
                 instead.",
                max_frame_label()
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_limits_come_from_core() {
        assert_eq!(
            CommentLimits::current(),
            CommentLimits {
                report_bytes: 20 * 1024 * 1024,
                report_label: "20 MB".into(),
                message_bytes: 16 * 1024 * 1024 - 64 * 1024,
                too_long_to_send:
                    "This comment is too long to send: one message carries at most 16 MB. Attach \
                     the text as a file instead."
                        .into(),
            }
        );
    }
}
