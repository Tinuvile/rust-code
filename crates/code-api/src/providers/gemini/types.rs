//! Google Gemini API request/response types.

use serde::{Deserialize, Serialize};

// ── Request types ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateContentRequest {
    pub contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_instruction: Option<GeminiContent>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<GeminiToolConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation_config: Option<GenerationConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiContent {
    pub role: String,
    pub parts: Vec<GeminiPart>,
}

/// A single part in a Gemini content message.
///
/// Uses custom serde to match Gemini's flat key format:
///   `{"text": "..."}` or `{"functionCall": {...}}` etc.
#[derive(Debug, Clone)]
pub enum GeminiPart {
    Text(GeminiTextPart),
    InlineData(GeminiInlineData),
    FunctionCall(GeminiFunctionCall),
    FunctionResponse(GeminiFunctionResponse),
}

impl GeminiPart {
    pub fn text(text: impl Into<String>) -> Self {
        GeminiPart::Text(GeminiTextPart { text: text.into() })
    }

    pub fn function_call(name: impl Into<String>, args: serde_json::Value) -> Self {
        GeminiPart::FunctionCall(GeminiFunctionCall { name: name.into(), args })
    }

    pub fn function_response(name: impl Into<String>, response: serde_json::Value) -> Self {
        GeminiPart::FunctionResponse(GeminiFunctionResponse {
            name: name.into(),
            response,
        })
    }
}

impl Serialize for GeminiPart {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        match self {
            GeminiPart::Text(t) => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("text", &t.text)?;
                map.end()
            }
            GeminiPart::InlineData(d) => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("inlineData", d)?;
                map.end()
            }
            GeminiPart::FunctionCall(fc) => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("functionCall", fc)?;
                map.end()
            }
            GeminiPart::FunctionResponse(fr) => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("functionResponse", fr)?;
                map.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for GeminiPart {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let v: serde_json::Value = serde_json::Value::deserialize(deserializer)?;
        if let Some(text) = v.get("text").and_then(|t| t.as_str()) {
            return Ok(GeminiPart::Text(GeminiTextPart { text: text.to_owned() }));
        }
        if let Some(fc) = v.get("functionCall") {
            let call: GeminiFunctionCall =
                serde_json::from_value(fc.clone()).map_err(serde::de::Error::custom)?;
            return Ok(GeminiPart::FunctionCall(call));
        }
        if let Some(fr) = v.get("functionResponse") {
            let resp: GeminiFunctionResponse =
                serde_json::from_value(fr.clone()).map_err(serde::de::Error::custom)?;
            return Ok(GeminiPart::FunctionResponse(resp));
        }
        if let Some(data) = v.get("inlineData") {
            let inline: GeminiInlineData =
                serde_json::from_value(data.clone()).map_err(serde::de::Error::custom)?;
            return Ok(GeminiPart::InlineData(inline));
        }
        // Fallback: treat as text.
        Ok(GeminiPart::text(v.to_string()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiTextPart {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiInlineData {
    pub mime_type: String,
    pub data: String, // base64
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiFunctionCall {
    pub name: String,
    pub args: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiFunctionResponse {
    pub name: String,
    pub response: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiToolConfig {
    pub function_declarations: Vec<GeminiFunctionDeclaration>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GeminiFunctionDeclaration {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
}

// ── Response types ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateContentResponse {
    #[serde(default)]
    pub candidates: Vec<GeminiCandidate>,
    #[serde(default)]
    pub usage_metadata: Option<GeminiUsageMetadata>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiCandidate {
    pub content: Option<GeminiContent>,
    #[serde(default)]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiUsageMetadata {
    #[serde(default)]
    pub prompt_token_count: u32,
    #[serde(default)]
    pub candidates_token_count: u32,
    #[serde(default)]
    pub total_token_count: u32,
}
