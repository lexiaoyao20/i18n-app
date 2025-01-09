use anyhow::Result;
use i18n_app::{api, config::Config, translation::TranslationFile};
use mockito::Server;
use std::collections::HashMap;
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

fn create_test_translation() -> TranslationFile {
    let mut content = HashMap::new();
    content.insert("test.key".to_string(), "Test Value".to_string());

    TranslationFile {
        language_code: "en-US".to_string(),
        relative_path: "test/en-US.json".to_string(),
        content,
    }
}

#[test]
fn test_api_upload_success() -> Result<()> {
    let mut server = Server::new();
    let rt = tokio::runtime::Runtime::new()?;

    rt.block_on(async {
        let (_temp_dir, config) = create_test_config(&server.url())?;
        let translation = create_test_translation();

        let mock = server
            .mock("POST", "/api/At.Locazy/cli/terms/upload")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"code":0,"message":"success","data":{"success":true,"notVerifyTerminologies":{},"notVerifyVariables":{}}}"#)
            .create();

        let result = api::upload_translation(&config, &translation).await;
        assert!(result.is_ok());

        mock.assert();
        Ok(())
    })
}

#[test]
fn test_api_upload_failure() -> Result<()> {
    let mut server = Server::new();
    let rt = tokio::runtime::Runtime::new()?;

    rt.block_on(async {
        let (_temp_dir, config) = create_test_config(&server.url())?;
        let translation = create_test_translation();

        let mock = server
            .mock("POST", "/api/At.Locazy/cli/terms/upload")
            .with_status(400)
            .with_header("content-type", "application/json")
            .with_body(r#"{"code":400,"message":"Bad Request","data":null}"#)
            .create();

        let result = api::upload_translation(&config, &translation).await;
        assert!(result.is_err());

        mock.assert();
        Ok(())
    })
}
