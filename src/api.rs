use crate::{config::Config, translation::TranslationFile};
use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Serialize)]
struct ConfigRequest {
    #[serde(rename = "productCode")]
    product_code: String,
    #[serde(rename = "subSystemNames")]
    sub_system_name: Vec<String>,
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
    pub files: Option<Vec<FileDownloadInfo>>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDownloadInfo {
    pub sub_system: String,
    pub lang: String,
    #[serde(rename = "internalUrl")]
    pub internal_url: String,
    pub url: String,
}

pub async fn upload_translation(config: &Config, translation: &TranslationFile) -> Result<()> {
    let client = Client::new();
    let url = format!("{}/api/At.Locazy/cli/terms/upload", config.host);

    // 获取父目录路径
    let parent_path = Path::new(&translation.relative_path)
        .parent()
        .and_then(|p| p.to_str())
        .unwrap_or("");
    let path = format!("{}/{}", config.path_prefix, parent_path);
    let request = UploadRequest {
        sub_system_name: config.sub_system_name.clone(),
        version_no: config.version_no.clone(),
        term_and_text: translation.content.clone(),
        product_code: config.product_code.clone(),
        path,
        language_code: translation.language_code.clone(),
    };

    // 添加更详细的上传信息日志
    tracing::info!(
        "Uploading {} keys for language: {}, path: {}",
        translation.content.len(),
        translation.language_code,
        translation.relative_path
    );
    tracing::info!(
        "Keys to upload: {:?}",
        translation.content.keys().collect::<Vec<_>>()
    );

    // 在 debug 模式下打印具体要上传的内容
    #[cfg(debug_assertions)]
    {
        tracing::debug!(
            "Upload request content: {}",
            serde_json::to_string_pretty(&request)?
        );
    }

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
        sub_system_name: vec![config.sub_system_name.clone()],
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
        "curl -X POST '{}' -H 'preview: {}' -H 'Content-Type: application/json' -d '{}'",
        url,
        config.preview_mode,
        serde_json::to_string(&request_body)?
    );

    let response = match client
        .post(&url)
        .header("preview", &config.preview_mode)
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
        "Got {} language files",
        response.data.files.as_ref().map(|f| f.len()).unwrap_or(0)
    );

    Ok(response)
}

#[derive(Debug, Deserialize)]
struct ErrorResponse {
    code: i32,
    message: Option<String>,
    data: Option<String>,
}

pub async fn download_translation(config: &Config, download_url: &str) -> Result<String> {
    let client = Client::new();
    let url = download_url;

    tracing::info!("Downloading translation from: {}", url);

    let response = match client
        .get(url)
        .header("preview", &config.preview_mode)
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

    tracing::debug!("Response status: {}", status);

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

    // 直接返回原始文本内容
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
                "versionNo": "1.0.0",
                "baseLanguage": "en-US",
                "previewMode": "1",
                "pathPrefix": "app",
                "include": ["fixtures/*.json"],
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
                        "files": [
                            {
                                "subSystem": "test-system",
                                "lang": "en-US",
                                "internalUrl": "http://internal.url/test.json",
                                "url": "http://public.url/test.json"
                            }
                        ]
                    }
                }"#,
                )
                .create();

            let result = get_translation_config(&config).await;
            assert!(result.is_ok());
            if let Ok(res) = result {
                assert_eq!(res.data.files.as_ref().unwrap().len(), 1);
                let file_info = &res.data.files.as_ref().unwrap()[0];
                assert_eq!(file_info.lang, "en-US");
                assert_eq!(file_info.url, "http://public.url/test.json");
            }

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

            // 模拟服务器响应，包含新的 pathPrefix 结构
            let mock_response_content = r#"{
                "test_key": "test_value"
            }"#;
            let mock_response = format!(
                r#"{{
                "{}/languages": {}
            }}"#,
                config.path_prefix, mock_response_content
            );

            let download_url_path = "/download/en-US.json";
            let mock = server
                .mock("GET", download_url_path)
                .with_status(200)
                .with_header("content-type", "application/json")
                .with_body(&mock_response)
                .create();

            let full_download_url = format!("{}{}", server.url(), download_url_path);

            let result = download_translation(&config, &full_download_url).await;
            assert!(result.is_ok());

            // 验证返回的内容是否是原始的、包含嵌套结构的 JSON 字符串
            if let Ok(content) = result {
                assert_eq!(content, mock_response);
            }

            mock.assert();
            Ok(())
        })
    }
}
