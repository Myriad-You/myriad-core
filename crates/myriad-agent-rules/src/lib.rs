//! Pure Agent rules: prompt caps, brew write policy, error classification.
//!
//! No I/O. Backend services re-export moved symbols so existing imports compile.

pub mod image;
pub mod prompt;
pub mod semantic;
pub mod steering;

pub use semantic::{
    capability_needs_conversation_context, capability_needs_memory, extract_semantic_text,
};
pub use steering::{
    append_instruction, inject_directive_to_params, inject_steering_to_params, with_system_guidance,
};
pub use image::{
    clamp_image_dim, parse_image_dim, resolve_image_dimensions, resolve_image_prompt,
    resolve_negative_prompt, DEFAULT_IMAGE_HEIGHT, DEFAULT_IMAGE_WIDTH, IMAGE_DIM_MAX,
    IMAGE_DIM_MIN,
};
pub use prompt::{
    append_memory_to_system_prompt, merge_system_prompt, sanitize_prompt_input,
    take_recent_conversation_messages,
};

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
