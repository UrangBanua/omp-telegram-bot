//! Definisi tipe data, struct, dan enum untuk protokol JSON-RPC OMP dan konfigurasi aplikasi.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;

/// Konfigurasi aplikasi yang dimuat dari environment variable (.env).
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub teloxide_token: String,
    pub allowed_user_ids: HashSet<u64>,
    pub project_workspace: PathBuf,
    pub omp_bin_path: String,
}

impl AppConfig {
    pub fn load_from_env() -> anyhow::Result<Self> {
        let teloxide_token = std::env::var("TELOXIDE_TOKEN")
            .map_err(|_| anyhow::anyhow!("TELOXIDE_TOKEN wajib disetel di .env"))?;

        let allowed_user_ids_str = std::env::var("ALLOWED_USER_IDS")
            .unwrap_or_else(|_| "".to_string());

        let mut allowed_user_ids = HashSet::new();
        for id_str in allowed_user_ids_str.split(',') {
            let trimmed = id_str.trim();
            if !trimmed.is_empty() {
                if let Ok(id) = trimmed.parse::<u64>() {
                    allowed_user_ids.insert(id);
                }
            }
        }

        let project_workspace = std::env::var("PROJECT_WORKSPACE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."));

        let omp_bin_path = std::env::var("OMP_BIN_PATH")
            .unwrap_or_else(|_| "omp".to_string());

        Ok(Self {
            teloxide_token,
            allowed_user_ids,
            project_workspace,
            omp_bin_path,
        })
    }
}

/// Representasi payload gambar multimodal (Base64).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageContent {
    pub url: String, // Format: data:image/png;base64,... atau URL
}

/// Perintah yang dikirimkan dari Rust Bridge ke stdin OMP RPC.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RpcCommand {
    #[serde(rename = "prompt")]
    Prompt {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        images: Option<Vec<ImageContent>>,
    },

    #[serde(rename = "steer")]
    Steer {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        images: Option<Vec<ImageContent>>,
    },

    #[serde(rename = "abort")]
    Abort {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
    },

    #[serde(rename = "follow_up")]
    FollowUp {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        images: Option<Vec<ImageContent>>,
    },

    #[serde(rename = "new_session")]
    NewSession {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(rename = "parentSession", skip_serializing_if = "Option::is_none")]
        parent_session: Option<String>,
    },

    #[serde(rename = "set_model")]
    SetModel {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provider: Option<String>,
        #[serde(rename = "modelId")]
        model_id: String,
    },

    #[serde(rename = "set_thinking_level")]
    SetThinkingLevel {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        level: String,
    },

    #[serde(rename = "compact")]
    Compact {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(rename = "customInstructions", skip_serializing_if = "Option::is_none")]
        custom_instructions: Option<String>,
    },

    #[serde(rename = "get_state")]
    GetState {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
    },

    #[serde(rename = "switch_session")]
    SwitchSession {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(rename = "sessionPath")]
        session_path: String,
    },

    #[serde(rename = "set_session_name")]
    SetSessionName {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        name: String,
    },
}

impl RpcCommand {
    pub fn type_name(&self) -> &'static str {
        match self {
            RpcCommand::Prompt { .. } => "prompt",
            RpcCommand::Steer { .. } => "steer",
            RpcCommand::Abort { .. } => "abort",
            RpcCommand::FollowUp { .. } => "follow_up",
            RpcCommand::NewSession { .. } => "new_session",
            RpcCommand::SetModel { .. } => "set_model",
            RpcCommand::SetThinkingLevel { .. } => "set_thinking_level",
            RpcCommand::Compact { .. } => "compact",
            RpcCommand::GetState { .. } => "get_state",
            RpcCommand::SwitchSession { .. } => "switch_session",
            RpcCommand::SetSessionName { .. } => "set_session_name",
        }
    }
}

/// Struktur detail event pembaruan asisten (token/text delta).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantMessageEvent {
    #[serde(rename = "type")]
    pub event_type: String, // "text_delta", "thinking_delta", dll.
    pub delta: Option<String>,
}

/// Event yang diterima dari stdout OMP RPC.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RpcEvent {
    #[serde(rename = "ready")]
    Ready {
        #[serde(rename = "protocolVersion")]
        protocol_version: Option<u32>,
    },

    #[serde(rename = "agent_start")]
    AgentStart,

    #[serde(rename = "agent_end")]
    AgentEnd,

    #[serde(rename = "message_update")]
    MessageUpdate {
        #[serde(rename = "assistantMessageEvent")]
        assistant_message_event: Option<AssistantMessageEvent>,
    },

    #[serde(rename = "tool_execution_start")]
    ToolExecutionStart {
        #[serde(rename = "toolName")]
        tool_name: Option<String>,
        intent: Option<String>,
    },

    #[serde(rename = "tool_execution_end")]
    ToolExecutionEnd {
        #[serde(rename = "toolName")]
        tool_name: Option<String>,
    },

    #[serde(rename = "response")]
    Response {
        command: Option<String>,
        success: bool,
        error: Option<String>,
        data: Option<serde_json::Value>,
    },
    #[serde(rename = "disconnected")]
    Disconnected,
    #[serde(other)]
    Unknown,
}

impl RpcEvent {
    pub fn type_name(&self) -> &'static str {
        match self {
            RpcEvent::Ready { .. } => "ready",
            RpcEvent::AgentStart => "agent_start",
            RpcEvent::AgentEnd => "agent_end",
            RpcEvent::MessageUpdate { .. } => "message_update",
            RpcEvent::ToolExecutionStart { .. } => "tool_execution_start",
            RpcEvent::ToolExecutionEnd { .. } => "tool_execution_end",
            RpcEvent::Response { .. } => "response",
            RpcEvent::Disconnected => "disconnected",
            RpcEvent::Unknown => "unknown",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rpc_command_serialization() {
        let cmd = RpcCommand::Prompt {
            id: Some("req_1".to_string()),
            message: "Halo OMP".to_string(),
            images: None,
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("\"type\":\"prompt\""));
        assert!(json.contains("\"message\":\"Halo OMP\""));
        assert_eq!(cmd.type_name(), "prompt");
    }

    #[test]
    fn test_rpc_event_deserialization() {
        let raw_json = r#"{"type":"message_update","assistantMessageEvent":{"type":"text_delta","delta":"Halo"}}"#;
        let event: RpcEvent = serde_json::from_str(raw_json).unwrap();
        assert_eq!(event.type_name(), "message_update");
        if let RpcEvent::MessageUpdate { assistant_message_event } = event {
            let ev = assistant_message_event.unwrap();
            assert_eq!(ev.event_type, "text_delta");
            assert_eq!(ev.delta.as_deref(), Some("Halo"));
        } else {
            panic!("Expected MessageUpdate event");
        }
    }
}
