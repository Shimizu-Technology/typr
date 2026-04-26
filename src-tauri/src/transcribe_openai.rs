use reqwest::multipart;
use std::path::Path;
use std::time::Duration;

pub async fn transcribe_openai(
    api_key: &str,
    audio_path: &Path,
    language: &str,
    model: &str,
) -> Result<String, String> {
    if api_key.trim().is_empty() {
        return Err("OpenAI API key is missing".to_string());
    }

    let model = model.trim();
    if model.is_empty() {
        return Err("OpenAI transcription model is missing".to_string());
    }

    let audio_bytes =
        std::fs::read(audio_path).map_err(|e| format!("Failed to read audio file: {}", e))?;

    let file_part = multipart::Part::bytes(audio_bytes)
        .file_name("audio.wav")
        .mime_str("audio/wav")
        .map_err(|e| e.to_string())?;

    let mut form = multipart::Form::new()
        .text("model", model.to_string())
        .text("response_format", "json")
        .part("file", file_part);

    if !language.trim().is_empty() && language != "auto" {
        form = form.text("language", language.trim().to_string());
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| format!("Failed to build OpenAI client: {}", e))?;

    let response = client
        .post("https://api.openai.com/v1/audio/transcriptions")
        .header("Authorization", format!("Bearer {}", api_key))
        .multipart(form)
        .send()
        .await
        .map_err(|e| format!("OpenAI API request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("OpenAI API error ({}): {}", status, body));
    }

    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse OpenAI response: {}", e))?;

    json["text"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or("No 'text' field in OpenAI response".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_empty_api_key() {
        let path = Path::new("/tmp/test.wav");
        let result = transcribe_openai("", path, "auto", "gpt-4o-mini-transcribe").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("API key is missing"));
    }
}
