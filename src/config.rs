use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

const DEFAULT_CONFIG_FILE: &str = ".i18n-app.json";

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub host: String,
    #[serde(rename = "subSystemName")]
    pub sub_system_name: String,
    #[serde(rename = "productCode")]
    pub product_code: String,
    #[serde(rename = "productId")]
    pub product_id: i32,
    #[serde(rename = "versionNo")]
    pub version_no: String,
    #[serde(rename = "baseLanguage")]
    pub base_language: String,
    #[serde(rename = "previewMode")]
    pub preview_mode: String,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            host: "https://backoffice.devactstrade.com".to_string(),
            sub_system_name: "app".to_string(),
            product_code: "bos".to_string(),
            product_id: 1,
            version_no: "1.0.0".to_string(),
            base_language: "en-US".to_string(),
            preview_mode: "1".to_string(),
            include: vec![],
            exclude: vec![],
        }
    }
}

impl Config {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        let config: Config = serde_json::from_str(&content)?;
        Ok(config)
    }

    pub fn init() -> Result<()> {
        if Path::new(DEFAULT_CONFIG_FILE).exists() {
            return Err(anyhow!("Configuration file already exists"));
        }

        let config = Config::default();
        let content = serde_json::to_string_pretty(&config)?;
        fs::write(DEFAULT_CONFIG_FILE, content)?;
        Ok(())
    }

    pub fn ensure_config_exists() -> Result<()> {
        if !Path::new(DEFAULT_CONFIG_FILE).exists() {
            let config = Config::default();
            let content = serde_json::to_string_pretty(&config)?;
            fs::write(DEFAULT_CONFIG_FILE, content)?;
            tracing::warn!(
                "Configuration file not found. Created default configuration at {}",
                DEFAULT_CONFIG_FILE
            );
            tracing::warn!(
                "Please update the configuration file with your settings before proceeding."
            );
            return Err(anyhow!("Please update the configuration file"));
        }
        Ok(())
    }

    pub fn load() -> Result<Self> {
        Self::ensure_config_exists()?;
        Self::from_file(DEFAULT_CONFIG_FILE)
    }

    /// 获取 GitHub Token
    pub fn get_github_token() -> Option<String> {
        // 获取用户主目录
        if let Some(home_dir) = dirs::home_dir() {
            // 构建 ~/.config/i18n-app/config.toml 路径
            let config_path = home_dir
                .join(".config")
                .join("i18n-app")
                .join("config.toml");

            // 读取并解析配置文件
            if let Ok(content) = std::fs::read_to_string(config_path) {
                if let Ok(config) = content.parse::<toml::Table>() {
                    if let Some(github) = config.get("github") {
                        if let Some(token) = github.get("token") {
                            if let Some(token_str) = token.as_str() {
                                return Some(token_str.to_string());
                            }
                        }
                    }
                }
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_config_load() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let config_path = temp_dir.path().join("test_config.json");

        let test_config = r#"{
            "host": "https://test.com",
            "subSystemName": "test-system",
            "productCode": "test",
            "productId": 1,
            "versionNo": "1.0.0",
            "baseLanguage": "en-US",
            "previewMode": "1",
            "include": ["*.json"],
            "exclude": []
        }"#;

        fs::write(&config_path, test_config)?;

        let config = Config::from_file(&config_path)?;
        assert_eq!(config.host, "https://test.com");
        assert_eq!(config.sub_system_name, "test-system");
        assert_eq!(config.product_code, "test");
        assert_eq!(config.product_id, 1);
        assert_eq!(config.base_language, "en-US");
        assert_eq!(config.preview_mode, "1");

        Ok(())
    }

    #[test]
    fn test_config_load_missing_field() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("test_config.json");

        let invalid_config = r#"{
            "host": "https://test.com"
        }"#;

        fs::write(&config_path, invalid_config).unwrap();

        let result = Config::from_file(&config_path);
        assert!(result.is_err());
    }
}
