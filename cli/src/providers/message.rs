//! Chat message types for LLM API interactions.
//!
//! This module defines the message format used in chat completion requests
//! and responses. The format is compatible with OpenAI-style chat APIs and
//! serializes to JSON that can be sent directly to model providers.
//!
//! # Example
//!
//! ```
//! use ai_passport::providers::message::ChatMessage;
//!
//! let user_msg = ChatMessage::user("Hello, how are you?");
//! let assistant_msg = ChatMessage::assistant("I'm doing well, thank you!");
//!
//! // Serializes to: {"role": "user", "content": "Hello, how are you?"}
//! let json = serde_json::to_string(&user_msg).unwrap();
//! ```

use serde::{Deserialize, Serialize};

/// Role of a participant in a chat conversation.
///
/// Maps to the `role` field in chat completion API messages.
/// Serializes to lowercase strings as expected by OpenAI-compatible APIs.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ChatMessageRole {
    /// Message from the user/human.
    User,
    /// Message from the AI assistant.
    Assistant,
}

/// A single message in a chat conversation.
///
/// Represents a message exchanged between a user and an AI assistant.
/// Compatible with OpenAI-style chat completion APIs.
///
/// # Serialization
///
/// Serializes to JSON format expected by chat APIs:
/// ```json
/// {"role": "user", "content": "message text"}
/// ```
///
/// # Construction
///
/// Use the [`ChatMessage::user`] and [`ChatMessage::assistant`] constructors
/// to create messages with the appropriate role:
///
/// ```
/// # use ai_passport::providers::message::ChatMessage;
/// let user_msg = ChatMessage::user("What is 2+2?");
/// let assistant_msg = ChatMessage::assistant("2+2 equals 4.");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// The role of the message author (user or assistant).
    role: ChatMessageRole,
    /// The text content of the message.
    pub(crate) content: String,
}

impl ChatMessage {
    /// Create a new user message.
    ///
    /// # Arguments
    ///
    /// * `content` - The message text from the user.
    ///
    /// # Example
    ///
    /// ```
    /// # use ai_passport::providers::message::ChatMessage;
    /// let msg = ChatMessage::user("Hello!");
    /// ```
    pub fn user<S: ToString>(content: S) -> ChatMessage {
        ChatMessage {
            role: ChatMessageRole::User,
            content: content.to_string(),
        }
    }

    /// Create a new assistant message.
    ///
    /// # Arguments
    ///
    /// * `content` - The message text from the assistant.
    ///
    /// # Example
    ///
    /// ```
    /// # use ai_passport::providers::message::ChatMessage;
    /// let msg = ChatMessage::assistant("Hello! How can I help you?");
    /// ```
    pub fn assistant<S: ToString>(content: S) -> ChatMessage {
        ChatMessage {
            role: ChatMessageRole::Assistant,
            content: content.to_string(),
        }
    }

    /// Get the content of the message.
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Get the role of the message.
    pub fn role(&self) -> ChatMessageRole {
        self.role
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    #[test]
    fn test_user_message_serialization() {
        let msg = ChatMessage::user("Hello, world!");
        let json_str = serde_json::to_string(&msg).unwrap();
        let parsed: Value = serde_json::from_str(&json_str).unwrap();

        assert_eq!(parsed["role"], "user");
        assert_eq!(parsed["content"], "Hello, world!");
    }

    #[test]
    fn test_assistant_message_serialization() {
        let msg = ChatMessage::assistant("I'm here to help.");
        let json_str = serde_json::to_string(&msg).unwrap();
        let expected_json = r#"{"role":"assistant","content":"I'm here to help."}"#;
        assert_eq!(json_str, expected_json);
    }

    #[test]
    fn test_user_message_deserialization() {
        let json = json!({"role": "user", "content": "Test message"});
        let msg: ChatMessage = serde_json::from_value(json).unwrap();

        assert_eq!(msg.role(), ChatMessageRole::User);
        assert_eq!(msg.content(), "Test message");
    }

    #[test]
    fn test_assistant_message_deserialization() {
        let json = json!({"role": "assistant", "content": "Response text"});
        let msg: ChatMessage = serde_json::from_value(json).unwrap();

        assert_eq!(msg.role(), ChatMessageRole::Assistant);
        assert_eq!(msg.content(), "Response text");
    }

    #[test]
    fn test_messages_array_serialization() {
        let messages = vec![
            ChatMessage::user("What is Rust?"),
            ChatMessage::assistant("Rust is a systems programming language."),
            ChatMessage::user("Tell me more."),
        ];

        let json_str = serde_json::to_string(&messages).unwrap();
        let parsed: Vec<Value> = serde_json::from_str(&json_str).unwrap();

        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0]["role"], "user");
        assert_eq!(parsed[1]["role"], "assistant");
        assert_eq!(parsed[2]["role"], "user");
    }

    #[test]
    fn test_openai_compatible_format() {
        // Verify the exact JSON format expected by OpenAI-compatible APIs
        let msg = ChatMessage::user("hello");
        let json_str = serde_json::to_string(&msg).unwrap();

        // Should match the format used in budget.rs test
        assert!(json_str.contains(r#""role":"user""#));
        assert!(json_str.contains(r#""content":"hello""#));
    }

    #[test]
    fn test_role_enum_serialization() {
        assert_eq!(
            serde_json::to_string(&ChatMessageRole::User).unwrap(),
            r#""user""#
        );
        assert_eq!(
            serde_json::to_string(&ChatMessageRole::Assistant).unwrap(),
            r#""assistant""#
        );
    }

    #[test]
    fn test_special_characters_in_content() {
        let msg = ChatMessage::user("Hello \"world\"!\nNew line\ttab");
        let json_str = serde_json::to_string(&msg).unwrap();
        let parsed: ChatMessage = serde_json::from_str(&json_str).unwrap();

        assert_eq!(parsed.content(), "Hello \"world\"!\nNew line\ttab");
    }

    #[test]
    fn test_unicode_content() {
        let msg = ChatMessage::user("こんにちは 🌍 émoji");
        let json_str = serde_json::to_string(&msg).unwrap();
        let parsed: ChatMessage = serde_json::from_str(&json_str).unwrap();

        assert_eq!(parsed.content(), "こんにちは 🌍 émoji");
    }
}
