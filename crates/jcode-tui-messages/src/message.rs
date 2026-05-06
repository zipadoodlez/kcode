use jcode_message_types::ToolCall;
use serde_json::Value;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// A message in the conversation for TUI display.
#[derive(Clone)]
pub struct DisplayMessage {
    pub role: String,
    pub content: String,
    pub tool_calls: Vec<String>,
    pub duration_secs: Option<f32>,
    pub title: Option<String>,
    /// Full tool call data for role="tool" messages.
    pub tool_data: Option<ToolCall>,
}

impl DisplayMessage {
    /// Create an error message.
    pub fn error(content: impl Into<String>) -> Self {
        Self {
            role: "error".to_string(),
            content: content.into(),
            tool_calls: Vec::new(),
            duration_secs: None,
            title: None,
            tool_data: None,
        }
    }

    /// Create a system message.
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: content.into(),
            tool_calls: Vec::new(),
            duration_secs: None,
            title: None,
            tool_data: None,
        }
    }

    /// Create a background task completion message (dedicated card display).
    pub fn background_task(content: impl Into<String>) -> Self {
        Self {
            role: "background_task".to_string(),
            content: content.into(),
            tool_calls: Vec::new(),
            duration_secs: None,
            title: None,
            tool_data: None,
        }
    }

    /// Create a display-only usage card. This is shown in the transcript UI but
    /// is not part of provider/model context.
    pub fn usage(content: impl Into<String>) -> Self {
        Self {
            role: "usage".to_string(),
            content: content.into(),
            tool_calls: Vec::new(),
            duration_secs: None,
            title: Some("Usage".to_string()),
            tool_data: None,
        }
    }

    /// Create a display-only overnight progress card. This is shown in the
    /// transcript UI but is not part of provider/model context.
    pub fn overnight(content: impl Into<String>) -> Self {
        Self {
            role: "overnight".to_string(),
            content: content.into(),
            tool_calls: Vec::new(),
            duration_secs: None,
            title: Some("Overnight".to_string()),
            tool_data: None,
        }
    }

    /// Create a memory injection message (bordered box display).
    pub fn memory(title: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "memory".to_string(),
            content: content.into(),
            tool_calls: Vec::new(),
            duration_secs: None,
            title: Some(title.into()),
            tool_data: None,
        }
    }

    /// Create a swarm notification message (DM/channel/broadcast/shared context).
    pub fn swarm(title: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "swarm".to_string(),
            content: content.into(),
            tool_calls: Vec::new(),
            duration_secs: None,
            title: Some(title.into()),
            tool_data: None,
        }
    }

    /// Create a user message.
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: content.into(),
            tool_calls: Vec::new(),
            duration_secs: None,
            title: None,
            tool_data: None,
        }
    }

    /// Create an assistant message.
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: content.into(),
            tool_calls: Vec::new(),
            duration_secs: None,
            title: None,
            tool_data: None,
        }
    }

    /// Create an assistant message with duration.
    pub fn assistant_with_duration(content: impl Into<String>, duration_secs: f32) -> Self {
        Self {
            role: "assistant".to_string(),
            content: content.into(),
            tool_calls: Vec::new(),
            duration_secs: Some(duration_secs),
            title: None,
            tool_data: None,
        }
    }

    /// Create a tool message.
    pub fn tool(content: impl Into<String>, tool_data: ToolCall) -> Self {
        Self {
            role: "tool".to_string(),
            content: content.into(),
            tool_calls: Vec::new(),
            duration_secs: None,
            title: None,
            tool_data: Some(tool_data),
        }
    }

    /// Create a tool message with title.
    pub fn tool_with_title(
        content: impl Into<String>,
        tool_data: ToolCall,
        title: impl Into<String>,
    ) -> Self {
        Self {
            role: "tool".to_string(),
            content: content.into(),
            tool_calls: Vec::new(),
            duration_secs: None,
            title: Some(title.into()),
            tool_data: Some(tool_data),
        }
    }

    /// Add tool calls to message (builder pattern).
    pub fn with_tool_calls(mut self, tool_calls: Vec<String>) -> Self {
        self.tool_calls = tool_calls;
        self
    }

    /// Add title to message (builder pattern).
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn stable_cache_hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.role.hash(&mut hasher);
        self.content.hash(&mut hasher);
        self.tool_calls.hash(&mut hasher);
        self.title.hash(&mut hasher);
        if let Some(tool) = &self.tool_data {
            tool.id.hash(&mut hasher);
            tool.name.hash(&mut hasher);
            hash_json_value(&tool.input, &mut hasher);
        }
        hasher.finish()
    }
}

fn hash_json_value(value: &Value, hasher: &mut DefaultHasher) {
    match value {
        Value::Null => 0u8.hash(hasher),
        Value::Bool(b) => {
            1u8.hash(hasher);
            b.hash(hasher);
        }
        Value::Number(n) => {
            2u8.hash(hasher);
            n.hash(hasher);
        }
        Value::String(s) => {
            3u8.hash(hasher);
            s.hash(hasher);
        }
        Value::Array(arr) => {
            4u8.hash(hasher);
            arr.len().hash(hasher);
            for item in arr {
                hash_json_value(item, hasher);
            }
        }
        Value::Object(map) => {
            5u8.hash(hasher);
            map.len().hash(hasher);
            for (k, v) in map {
                k.hash(hasher);
                hash_json_value(v, hasher);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn message_with_input(input: Value) -> DisplayMessage {
        DisplayMessage {
            role: "tool".to_string(),
            content: "content".to_string(),
            tool_calls: vec!["read".to_string()],
            duration_secs: Some(1.0),
            title: Some("Read".to_string()),
            tool_data: Some(ToolCall {
                id: "call-1".to_string(),
                name: "read".to_string(),
                input,
                intent: None,
            }),
        }
    }

    #[test]
    fn stable_cache_hash_includes_tool_input() {
        let first = message_with_input(json!({ "file_path": "a.rs" }));
        let second = message_with_input(json!({ "file_path": "b.rs" }));
        assert_ne!(first.stable_cache_hash(), second.stable_cache_hash());
    }

    #[test]
    fn stable_cache_hash_ignores_duration() {
        let mut first = message_with_input(json!({ "file_path": "a.rs" }));
        let mut second = first.clone();
        first.duration_secs = Some(1.0);
        second.duration_secs = Some(9.0);
        assert_eq!(first.stable_cache_hash(), second.stable_cache_hash());
    }
}
