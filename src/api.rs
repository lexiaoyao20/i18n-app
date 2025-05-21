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

    // 检查 path_prefix 是否已包含完整的 URL
    let url = if file_group.path_prefix.starts_with("http://")
        || file_group.path_prefix.starts_with("https://")
    {
        format!(
            "{}/{}",
            file_group.path_prefix.trim_end_matches('/'),
            file_name
        )
    } else {
        format!(
            "{}/{}/{}",
            config.host.trim_end_matches('/'),
            file_group.path_prefix.trim_matches('/'),
            file_name
        )
    };

    tracing::info!("Downloading translation from: {}", url);

    let response = match client
        .get(&url)
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

    // 解析返回的 JSON
    let json_value: serde_json::Value = serde_json::from_str(&text)?;

    // 从配置文件的 include 模式中提取基础路径，并移除末尾的斜杠
    let base_path = config
        .include
        .first()
        .ok_or_else(|| anyhow!("No include patterns configured"))?
        .replace("*.json", "")
        .trim_end_matches('/')
        .to_string();

    // 拼接路径, config.path_prefix 和 base_path
    let path = format!("{}/{}", config.path_prefix, base_path);

    // 使用 path 作为匹配键
    let lang_content = json_value
        .as_object()
        .ok_or_else(|| anyhow!("Response is not a JSON object"))?
        .iter()
        .find(|(key, _)| *key == &path)
        .map(|(_, value)| value)
        .ok_or_else(|| anyhow!("Language content not found in response"))?;

    Ok(serde_json::to_string_pretty(lang_content)?)
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
                "pathPrefix": "test",
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

            // 创建一个模拟的 FileGroup
            let file_group = FileGroup {
                path_prefix: "test".to_string(),
                language_code: "en-US".to_string(),
                file_names: vec!["test.json".to_string()],
            };

            // 模拟服务器响应，使用正确的路径结构
            let mock_response = r#"{
                "test/fixtures": {
                    "test": {
                        "key": "Test Value"
                    }
                }
            }"#;

            let mock = server
                .mock("GET", "/test/test.json")
                .with_status(200)
                .with_header("content-type", "application/json")
                .with_body(mock_response)
                .create();

            let result = download_translation(&config, &file_group, "test.json").await;
            assert!(result.is_ok());

            // 验证返回的内容是否正确
            if let Ok(content) = result {
                let parsed: serde_json::Value = serde_json::from_str(&content)?;
                assert_eq!(parsed["test"]["key"].as_str().unwrap(), "Test Value");
            }

            mock.assert();
            Ok(())
        })
    }
}
