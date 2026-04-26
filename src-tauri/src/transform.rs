use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::json;

use crate::cleanup::cleanup_text;
use crate::secrets::{get_secret, SecretKind};
use crate::settings::Settings;

pub enum TransformProvider {
    None,
    OpenAiCompatible {
        base_url: String,
        model: String,
        api_key: Option<String>,
    },
}

pub struct TransformRequest {
    pub mode: String,
    pub language: String,
    pub selected_text: Option<String>,
}

impl TransformProvider {
    pub async fn transform(
        &self,
        text: &str,
        request: &TransformRequest,
    ) -> Result<String, String> {
        match self {
            TransformProvider::None => Ok(text.to_string()),
            TransformProvider::OpenAiCompatible {
                base_url,
                model,
                api_key,
            } => {
                let prompt = build_mode_prompt(request, text);
                let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
                let client = reqwest::Client::new();
                let mut request_builder = client
                    .post(url)
                    .header(CONTENT_TYPE, "application/json")
                    .json(&json!({
                        "model": model,
                        "temperature": 0.1,
                        "messages": [
                            {
                                "role": "system",
                                "content": "You are a careful dictation post-processor. Preserve technical syntax, indentation, line breaks, code fences, bullet lists, numbered lists, filenames, and command formatting. Return only the rewritten text with no commentary."
                            },
                            {
                                "role": "user",
                                "content": prompt
                            }
                        ]
                    }));

                if let Some(api_key) = api_key {
                    request_builder =
                        request_builder.header(AUTHORIZATION, format!("Bearer {}", api_key));
                }

                let response = request_builder
                    .send()
                    .await
                    .map_err(|e| format!("Transform request failed: {}", e))?;

                if !response.status().is_success() {
                    let status = response.status();
                    let body = response.text().await.unwrap_or_default();
                    return Err(format!("Transform provider error ({}): {}", status, body));
                }

                let json: serde_json::Value = response
                    .json()
                    .await
                    .map_err(|e| format!("Failed to parse transform response: {}", e))?;

                json["choices"][0]["message"]["content"]
                    .as_str()
                    .map(|s| s.to_string())
                    .ok_or("No transformed text returned by provider".to_string())
            }
        }
    }
}

pub async fn apply_text_transform(
    settings: &Settings,
    text: &str,
    selected_text: Option<String>,
) -> Result<String, String> {
    let cleaned = cleanup_text(text);
    if cleaned.is_empty() {
        return Ok(cleaned);
    }

    match settings.text_mode.as_str() {
        "raw" => Ok(cleaned),
        "polished" | "command" => {
            let provider = build_transform_provider(settings)?;
            let transformed = provider
                .transform(
                    &cleaned,
                    &TransformRequest {
                        mode: settings.text_mode.clone(),
                        language: settings.language.clone(),
                        selected_text,
                    },
                )
                .await?;

            Ok(cleanup_text(&transformed))
        }
        _ => Ok(cleaned),
    }
}

fn build_transform_provider(settings: &Settings) -> Result<TransformProvider, String> {
    match settings.transform_provider.as_str() {
        "lmstudio" | "openai-compatible" => {
            let api_key = get_secret(SecretKind::TransformApiKey)?;

            Ok(TransformProvider::OpenAiCompatible {
                base_url: settings.transform_base_url.clone(),
                model: settings.transform_model.clone(),
                api_key,
            })
        }
        _ => Ok(TransformProvider::None),
    }
}

fn build_mode_prompt(request: &TransformRequest, text: &str) -> String {
    let language_hint = if request.language.trim().is_empty() || request.language == "auto" {
        "Keep the original language of the text.".to_string()
    } else {
        format!(
            "Keep the final output in the language represented by this code: {}.",
            request.language
        )
    };

    match request.mode.as_str() {
        "polished" => format!(
            "Rewrite this dictated text so it is polished but still faithful to the speaker. {} Remove obvious filler words when safe, fix punctuation conservatively, and preserve formatting.\n\n{}",
            language_hint, text
        ),
        "command" => {
            let selected_text = request
                .selected_text
                .as_deref()
                .unwrap_or("")
                .trim();

            format!(
                "Apply the following spoken editing instruction to the selected text. {} Return only the edited replacement text.\n\nInstruction:\n{}\n\nSelected text:\n{}",
                language_hint, text, selected_text
            )
        }
        _ => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_none_provider_returns_input() {
        let provider = TransformProvider::None;
        let result = provider
            .transform(
                "hello world",
                &TransformRequest {
                    mode: "polished".to_string(),
                    language: "auto".to_string(),
                    selected_text: None,
                },
            )
            .await
            .unwrap();

        assert_eq!(result, "hello world");
    }

    #[test]
    fn test_build_command_prompt_contains_instruction_and_selection() {
        let prompt = build_mode_prompt(
            &TransformRequest {
                mode: "command".to_string(),
                language: "auto".to_string(),
                selected_text: Some("old text".to_string()),
            },
            "make this shorter",
        );

        assert!(prompt.contains("Instruction:\nmake this shorter"));
        assert!(prompt.contains("Selected text:\nold text"));
    }
}
