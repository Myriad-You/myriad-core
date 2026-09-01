//! Pure Agent rules: prompt caps, brew write policy, error classification.
//!
//! No I/O. Backend services re-export moved symbols so existing imports compile.

/// Shared Unicode-scalar cap for user-authored model text (chat, generate, analyze).
pub const USER_TEXT_MAX_CHARS: usize = 32680;

/// Cap for user text injected into model prompts (analyze instruction, chat, search).
pub const SANITIZE_PROMPT_MAX_CHARS: usize = USER_TEXT_MAX_CHARS;

/// Unicode scalar cap. Must stay <= `image_generation::MAX_PROMPT_CHARS` (32680).
/// Counted with `chars()`, not bytes — CJK prompts are 3 bytes per character.
pub const IMAGE_PROMPT_MAX_CHARS: usize = USER_TEXT_MAX_CHARS;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_caps_share_the_user_authored_limit() {
        assert_eq!(SANITIZE_PROMPT_MAX_CHARS, USER_TEXT_MAX_CHARS);
        assert_eq!(IMAGE_PROMPT_MAX_CHARS, USER_TEXT_MAX_CHARS);
        assert!(USER_TEXT_MAX_CHARS > 0);
        let over: String = "x".repeat(IMAGE_PROMPT_MAX_CHARS + 1);
        assert!(over.chars().count() > IMAGE_PROMPT_MAX_CHARS);
    }
}
