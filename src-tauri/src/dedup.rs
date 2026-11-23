//! Echo suppression between the `me` (mic) and `them` (loopback) roles (D11).
//! On speakers the mic picks up the remote party, so the same utterance lands
//! in both streams; text-level containment drops the duplicate before it is
//! stored. Headphones remain the recommendation; AEC is wave 2.

/// Normalize for comparison: lowercase, keep alphanumerics, split on the rest.
fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 1)
        .map(|w| w.to_string())
        .collect()
}

/// True when `candidate` is (mostly) contained in the recent opposite-role
/// transcript — i.e. an echo, not new speech. Containment threshold 0.7:
/// an echo reproduces nearly every word; new speech does not.
pub fn is_echo(candidate: &str, recent_other: &str) -> bool {
    const THRESHOLD: f32 = 0.7;
    let cand = words(candidate);
    if cand.is_empty() {
        return false;
    }
    let other: std::collections::HashSet<String> = words(recent_other).into_iter().collect();
    if other.is_empty() {
        return false;
    }
    let hits = cand.iter().filter(|w| other.contains(*w)).count();
    hits as f32 / cand.len() as f32 >= THRESHOLD
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_echo_is_dropped() {
        let candidate = "Revenue grew fourteen percent while operating costs stayed flat";
        let other = "revenue grew fourteen percent while operating costs stayed flat.";
        assert!(is_echo(candidate, other));
    }

    #[test]
    fn unrelated_speech_is_kept() {
        let candidate = "Can you share your screen with the dashboard";
        let other = "revenue grew fourteen percent while operating costs stayed flat";
        assert!(!is_echo(candidate, other));
    }

    #[test]
    fn empty_inputs_never_match() {
        assert!(!is_echo("", "some text here"));
        assert!(!is_echo("some text here", ""));
        assert!(!is_echo("", ""));
    }

    #[test]
    fn partial_overlap_below_threshold_is_kept() {
        // one shared word ("budget") is not an echo
        let candidate = "the budget question is different from the churn discussion";
        let other = "budget breakdown for q4 and hiring plans";
        assert!(!is_echo(candidate, other));
    }

    #[test]
    fn punctuation_and_case_do_not_matter() {
        assert!(is_echo(
            "What about churn? The board asked!",
            "what about churn the board asked"
        ));
    }
}
