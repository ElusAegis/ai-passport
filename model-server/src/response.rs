//! Response generation for the demo model server.
//!
//! This module generates responses that respect:
//! - Word count requests in the prompt (e.g., "at least 100 words")
//! - max_tokens parameter from the API request

use regex::Regex;
use std::sync::LazyLock;

/// Filler phrases for generating padding content.
const FILLER_PHRASES: &[&str] = &[
    "The digital age continues to reshape how we interact with information and technology. ",
    "Cloud computing has revolutionized enterprise infrastructure and development practices. ",
    "Machine learning algorithms process vast amounts of data to derive meaningful insights. ",
    "Cybersecurity remains a critical concern for organizations of all sizes everywhere. ",
    "Open source software powers much of the modern internet and enterprise systems. ",
    "Distributed systems enable scalable and resilient applications across the globe. ",
    "Programming languages continue to evolve to meet new challenges and requirements. ",
    "Data privacy regulations affect technology development and deployment globally. ",
    "Artificial intelligence transforms industries at an unprecedented rapid pace. ",
    "Network protocols ensure reliable communication across heterogeneous systems. ",
    "Software engineering practices continue to mature and improve over time. ",
    "Hardware advancements enable new computational possibilities and innovations. ",
];

/// Regex to extract word count from prompts like "at least 100 words" or "100 words".
static WORD_COUNT_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:at\s+least\s+)?(\d+)\s+words").expect("Invalid regex"));

/// Extract requested word count from a prompt.
///
/// Looks for patterns like:
/// - "at least 100 words"
/// - "100 words"
/// - "Write 50 words about..."
pub fn extract_word_count(prompt: &str) -> Option<usize> {
    WORD_COUNT_REGEX
        .captures(prompt)
        .and_then(|cap| cap.get(1))
        .and_then(|m| m.as_str().parse().ok())
}

/// Generate a response of approximately the target word count.
///
/// If `max_tokens` is set and would result in fewer words, that limit takes precedence.
pub fn generate_response(target_words: usize, max_tokens: Option<u32>) -> String {
    // Calculate effective word limit based on max_tokens if set
    // Rough estimate: 1 token ≈ 0.75 words for English
    let max_words_from_tokens = max_tokens.map(|t| (t as f64 * 0.75) as usize);

    let effective_words = match max_words_from_tokens {
        Some(max_words) if max_words < target_words => max_words,
        _ => target_words,
    };

    generate_text_with_words(effective_words)
}

/// Generate text with approximately the specified number of words.
fn generate_text_with_words(target_words: usize) -> String {
    if target_words == 0 {
        return String::new();
    }

    let mut result = String::new();
    let mut word_count = 0;
    let mut phrase_idx = 0;

    while word_count < target_words {
        let phrase = FILLER_PHRASES[phrase_idx % FILLER_PHRASES.len()];
        let phrase_words: usize = phrase.split_whitespace().count();

        if word_count + phrase_words <= target_words {
            result.push_str(phrase);
            word_count += phrase_words;
        } else {
            // Add partial phrase to reach target
            let words_needed = target_words - word_count;
            let partial: String = phrase
                .split_whitespace()
                .take(words_needed)
                .collect::<Vec<_>>()
                .join(" ");
            result.push_str(&partial);
            result.push('.');
            word_count += words_needed;
        }

        phrase_idx += 1;
    }

    result.trim().to_string()
}

/// Generate a fixed reply for unknown or simple requests.
pub fn fixed_reply(model: &str, last_user_message: &str) -> String {
    match model {
        "demo-gpt-4o-mini" => format!(
            "You said: \"{}\" - this is a fixed demo reply.",
            last_user_message
        ),
        "demo-gpt-3.5-turbo" => "Hello from demo-gpt-3.5-turbo (fixed reply).".to_string(),
        _ => "Unknown model (demo server) - generic fixed reply.".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_word_count() {
        assert_eq!(
            extract_word_count("Write at least 100 words about AI"),
            Some(100)
        );
        assert_eq!(extract_word_count("Write 50 words"), Some(50));
        assert_eq!(
            extract_word_count("at least 200 words about technology"),
            Some(200)
        );
        assert_eq!(extract_word_count("No word count here"), None);
    }

    #[test]
    fn test_generate_text_word_count() {
        let text = generate_text_with_words(50);
        let actual_words = text.split_whitespace().count();
        assert!(
            (48..=52).contains(&actual_words),
            "Expected ~50 words, got {}",
            actual_words
        );
    }

    #[test]
    fn test_generate_response_respects_max_tokens() {
        // If max_tokens would limit to fewer words, use that
        let text = generate_response(1000, Some(50)); // 50 tokens ≈ 37 words
        let word_count = text.split_whitespace().count();
        assert!(word_count < 100, "Should be limited by max_tokens");
    }

    #[test]
    fn test_generate_response_uses_target_when_no_limit() {
        let text = generate_response(100, None);
        let word_count = text.split_whitespace().count();
        assert!(
            (98..=102).contains(&word_count),
            "Expected ~100 words, got {}",
            word_count
        );
    }
}
