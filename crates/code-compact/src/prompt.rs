//! Summarization prompt for context compaction.
//!
//! Ref: src/services/compact/compact.ts (system prompt)

/// Build the system prompt for a summarization API call.
///
/// Instructs the model to produce a dense, structured summary of the
/// conversation it will receive.  An optional `custom_instruction` is
/// appended after the standard text.
pub fn build_summarization_prompt(custom_instruction: Option<&str>) -> String {
    let base = "\
Your task is to create a detailed summary of the conversation above. \
This summary will be used to restore context when the conversation is continued later.

Please structure your summary as follows:

1. **Main objectives** — What was the user trying to accomplish?
2. **Key decisions and changes** — Important decisions made, code written or modified, \
commands executed, and their outcomes.
3. **Files and components touched** — List any files, directories, or components \
that were read, created, or modified, with brief notes on what changed.
4. **Current state** — Where things stand right now. What is complete, what is \
in progress, and what still needs to be done?
5. **Open questions / blockers** — Anything unresolved or flagged for follow-up.

Be comprehensive but dense. Preserve technical specifics — exact file paths, \
function names, error messages, command outputs — because they will be needed \
to continue the work accurately. Omit pleasantries and meta-commentary.";

    match custom_instruction {
        Some(extra) if !extra.trim().is_empty() => {
            format!("{base}\n\nAdditional instruction: {}", extra.trim())
        }
        _ => base.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_custom_instruction() {
        let p = build_summarization_prompt(None);
        assert!(p.contains("Main objectives"));
        assert!(!p.contains("Additional instruction"));
    }

    #[test]
    fn custom_instruction_appended() {
        let p = build_summarization_prompt(Some("Focus on Rust code only."));
        assert!(p.contains("Additional instruction: Focus on Rust code only."));
    }

    #[test]
    fn blank_custom_instruction_ignored() {
        let p = build_summarization_prompt(Some("   "));
        assert!(!p.contains("Additional instruction"));
    }
}
