use crate::{config::Config, translation::TranslationFile};
use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize)]
struct ConfigRequest {
    #[serde(rename = "productCode")]
    product_code: String,
    #[serde(rename = "subSystemName")]
    sub_system_name: String,
    #[serde(rename = "versionNo")]
    version_no: String,
}

#[derive(Debug, Serialize)]
struct UploadRequest {
    #[serde(rename = "subSystemName")]
    sub_system_name: String,
    #[serde(rename = "productCode")]
    product_code: String,
    #[serde(rename = "languageCode")]
    language_code: String,
    path: String,
    #[serde(rename = "versionNo")]
    version_no: String,
    #[serde(rename = "termAndText")]
    term_and_text: HashMap<String, String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct LongPollingResponse {
    pub code: i32,
    pub message: String,
    pub data: LongPollingData,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LongPollingData {
    #[serde(rename = "taskHash")]
    #[allow(dead_code)]
    pub task_hash: Option<String>,
    pub file_groups: Option<Vec<FileGroup>>,
    #[allow(dead_code)]
    pub change_terms: Option<serde_json::Value>,
    #[serde(rename = "systemInfos")]
    #[allow(dead_code)]
    pub system_infos: Option<Vec<SystemInfo>>,
    #[serde(rename = "querySubSystemInfo")]
    #[allow(dead_code)]
    pub query_sub_system_info: SystemInfo,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SystemInfo {
    #[allow(dead_code)]
    pub id: i32,
    #[allow(dead_code)]
    pub name: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct FileGroup {
    #[serde(rename = "pathPrefix")]
    pub path_prefix: String,
    #[serde(rename = "languageCode")]
    pub language_code: String,
    #[serde(rename = "fileNames")]
    pub file_names: Vec<String>,
}

pub async fn upload_translation(config: &Config, translation: &TranslationFile) -> Result<()> {
    let client = Client::new();
    let url = format!("{}/api/At.Locazy/cli/terms/upload", config.host);

    let request = UploadRequest {
        sub_system_name: config.sub_system_name.clone(),
        version_no: config.version_no.clone(),
        term_and_text: translation.content.clone(),
        product_code: config.product_code.clone(),
        path: format!("/json/{}", translation.relative_path),
        language_code: translation.language_code.clone(),
    };

    tracing::info!(
        "Uploading translation for language: {}, path: {}",
        translation.language_code,
        translation.relative_path
    );

    #[cfg(debug_assertions)]
    tracing::debug!(
        "curl -X POST '{}' -H 'Content-Type: application/json' -d '{}'",
        url,
        serde_json::to_string(&request)?
    );

    let response = match client.post(&url).json(&request).send().await {
        Ok(resp) => resp,
        Err(e) => {
            tracing::error!("Failed to send request to [{}]: {}", url, e);
            return Err(anyhow!("Failed to send request to {}: {}", url, e));
        }
    };

    let status = response.status();
    let text = match response.text().await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("Failed to read response from [{}]: {}", url, e);
            return Err(anyhow!("Failed to read response from {}: {}", url, e));
        }
    };

    #[cfg(debug_assertions)]
    {
        tracing::debug!("Response status: {}", status);
        tracing::debug!("Response body: {}", text);
    }

    if !status.is_success() {
        if let Ok(error_response) = serde_json::from_str::<ErrorResponse>(&text) {
            tracing::error!(
                "API request failed [{}]: status={}, code={}, message={}",
                url,
                status,
                error_response.code,
                error_response.message.as_deref().unwrap_or("No message")
            );
            if let Some(data) = error_response.data {
                if let Some(first_line) = data.lines().next() {
                    tracing::error!("Error details: {}", first_line);
                }
            }
        }
        return Err(anyhow!("Upload to {} failed with status {}", url, status));
    }

    Ok(())
}

pub async fn get_translation_config(config: &Config) -> Result<LongPollingResponse> {
    let client = Client::new();
    let url = format!("{}/api/At.Locazy/user/i18n/long-polling", config.host);

    let request_body = ConfigRequest {
        product_code: config.product_code.clone(),
        sub_system_name: config.sub_system_name.clone(),
        version_no: config.version_no.clone(),
    };

    tracing::info!(
        "Fetching translation config for system: {}, product: {}, version: {}",
        config.sub_system_name,
        config.product_code,
        config.version_no
    );

    #[cfg(debug_assertions)]
    tracing::debug!(
        "curl -X POST '{}' -H 'preview: 1' -H 'Content-Type: application/json' -d '{}'",
        url,
        serde_json::to_string(&request_body)?
    );

    let response = match client
        .post(&url)
        .header("preview", "1")
        .json(&request_body)
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            tracing::error!("Failed to send request to [{}]: {}", url, e);
            return Err(anyhow!("Failed to send request to {}: {}", url, e));
        }
    };

    let status = response.status();
    let text = match response.text().await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("Failed to read response from [{}]: {}", url, e);
            return Err(anyhow!("Failed to read response from {}: {}", url, e));
        }
    };

    #[cfg(debug_assertions)]
    {
        tracing::debug!("Response status: {}", status);
        tracing::debug!("Response body: {}", text);
    }

    if !status.is_success() {
        if let Ok(error_response) = serde_json::from_str::<ErrorResponse>(&text) {
            tracing::error!(
                "API request failed [{}]: status={}, code={}, message={}",
                url,
                status,
                error_response.code,
                error_response.message.as_deref().unwrap_or("No message")
            );
            if let Some(data) = error_response.data {
                if let Some(first_line) = data.lines().next() {
                    tracing::error!("Error details: {}", first_line);
                }
            }
        }
        return Err(anyhow!("Request to {} failed with status {}", url, status));
    }

    let response: LongPollingResponse = serde_json::from_str(&text)?;
    if response.code != 0 {
        return Err(anyhow!("Request failed: {}", response.message));
    }

    tracing::info!(
        "Got {} language groups with {} total files",
        response
            .data
            .file_groups
            .as_ref()
            .map(|g| g.len())
            .unwrap_or(0),
        response
            .data
            .file_groups
            .as_ref()
            .map(|groups| groups.iter().map(|g| g.file_names.len()).sum::<usize>())
            .unwrap_or(0)
    );

    Ok(response)
}

#[derive(Debug, Deserialize)]
struct ErrorResponse {
    code: i32,
    message: Option<String>,
    data: Option<String>,
}

pub async fn download_translation(
    config: &Config,
    file_group: &FileGroup,
    file_name: &str,
) -> Result<String> {
    let client = Client::new();
    let url = format!(
        "{}/{}/{}",
        config.host,
        file_group
            .path_prefix
            .trim_start_matches('/')
            .trim_end_matches('/'),
        file_name
    );

    tracing::info!("Downloading translation from: {}", url);

    let response = match client.get(&url).header("preview", "1").send().await {
        Ok(resp) => resp,
        Err(e) => {
            tracing::error!("Failed to send request to [{}]: {}", url, e);
            return Err(anyhow!("Failed to send request to {}: {}", url, e));
        }
    };

    let status = response.status();
    let text = match response.text().await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("Failed to read response from [{}]: {}", url, e);
            return Err(anyhow!("Failed to read response from {}: {}", url, e));
        }
    };

    tracing::debug!("Response status: {}", status);
    // tracing::debug!("Response body: {}", text); // 太长了，不打印

    if !status.is_success() {
        tracing::error!(
            "API request failed [{}]: status={}, response={}",
            url,
            status,
            text
        );
        return Err(anyhow!(
            "Download from {} failed with status {}",
            url,
            status
        ));
    }

    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Server;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_config(server_url: &str) -> Result<(TempDir, Config)> {
        let temp_dir = TempDir::new()?;
        let config_path = temp_dir.path().join("test_config.json");

        let config_content = format!(
            r#"{{
                "host": "{}",
                "subSystemName": "test-system",
                "productCode": "test",
                "productId": 1,
                "versionNo": "1.0.0",
                "baseLanguage": "en-US",
                "include": ["*.json"],
                "exclude": []
            }}"#,
            server_url
        );
        let mut file = File::create(&config_path)?;
        file.write_all(config_content.as_bytes())?;

        let config = Config::from_file(&config_path)?;
        Ok((temp_dir, config))
    }

    #[test]
    fn test_upload_translation_success() -> Result<()> {
        let mut server = Server::new();
        let rt = tokio::runtime::Runtime::new()?;

        rt.block_on(async {
            let (_temp_dir, config) = create_test_config(&server.url())?;
            let mut content = HashMap::new();
            content.insert("test.key".to_string(), "test value".to_string());

            let translation = TranslationFile {
                language_code: "en-US".to_string(),
                relative_path: "en-US.json".to_string(),
                content,
            };

            let mock = server
                .mock("POST", "/api/At.Locazy/cli/terms/upload")
                .with_status(200)
                .with_header("content-type", "application/json")
                .with_body(r#"{"code":0,"message":"success","data":{"success":true,"notVerifyTerminologies":{},"notVerifyVariables":{}}}"#)
                .create();

            let result = upload_translation(&config, &translation).await;
            assert!(result.is_ok());

            mock.assert();
            Ok(())
        })
    }

    #[test]
    fn test_upload_translation_failure() -> Result<()> {
        let mut server = Server::new();
        let rt = tokio::runtime::Runtime::new()?;

        rt.block_on(async {
            let (_temp_dir, config) = create_test_config(&server.url())?;
            let mut content = HashMap::new();
            content.insert("test.key".to_string(), "test value".to_string());

            let translation = TranslationFile {
                language_code: "en-US".to_string(),
                relative_path: "en-US.json".to_string(),
                content,
            };

            let mock = server
                .mock("POST", "/api/At.Locazy/cli/terms/upload")
                .with_status(400)
                .with_header("content-type", "application/json")
                .with_body(r#"{"code":400,"message":"Bad Request","data":null}"#)
                .create();

            let result = upload_translation(&config, &translation).await;
            assert!(result.is_err());

            mock.assert();
            Ok(())
        })
    }

    #[test]
    fn test_get_translation_config_success() -> Result<()> {
        let mut server = Server::new();
        let rt = tokio::runtime::Runtime::new()?;

        rt.block_on(async {
            let (_temp_dir, config) = create_test_config(&server.url())?;

            let mock = server
                .mock("POST", "/api/At.Locazy/user/i18n/long-polling")
                .with_status(200)
                .with_header("content-type", "application/json")
                .with_body(
                    r#"{
                    "code": 0,
                    "message": "success",
                    "data": {
                        "taskHash": "test-hash",
                        "fileGroups": [
                            {
                                "pathPrefix": "/test",
                                "languageCode": "en-US",
                                "fileNames": ["test.json"]
                            }
                        ],
                        "changeTerms": null,
                        "systemInfos": [
                            {
                                "id": 1,
                                "name": "test"
                            }
                        ],
                        "querySubSystemInfo": {
                            "id": 1,
                            "name": "test"
                        }
                    }
                }"#,
                )
                .create();

            let result = get_translation_config(&config).await;
            assert!(result.is_ok());

            mock.assert();
            Ok(())
        })
    }

    #[test]
    fn test_download_translation_success() -> Result<()> {
        let mut server = Server::new();
        let rt = tokio::runtime::Runtime::new()?;

        rt.block_on(async {
            let (_temp_dir, config) = create_test_config(&server.url())?;

            let mock = server
                .mock("GET", "/test/test.json")
                .with_status(200)
                .with_header("content-type", "application/json")
                .with_body(r#"{"test":"content"}"#)
                .create();

            let result = download_translation(
                &config,
                &FileGroup {
                    path_prefix: "/test".to_string(),
                    language_code: "en-US".to_string(),
                    file_names: vec!["test.json".to_string()],
                },
                "test.json",
            )
            .await;
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), r#"{"test":"content"}"#);

            mock.assert();
            Ok(())
        })
    }
}
