# i18n-app

一个用 Rust 编写的国际化翻译文件管理工具，用于同步和管理多语言翻译文件。

## 快速安装

使用 curl 安装:

```bash
/bin/bash -c "$(curl -fsSL https://github.com/lexiaoyao20/i18n-app/raw/main/install.sh)"
```

### 手动安装

如果你不想使用安装脚本，也可以：

1. 从 [Releases](https://github.com/lexiaoyao20/i18n-app/releases) 页面下载对应你系统的压缩包
2. 解压文件
3. 将 `i18n-app` 可执行文件移动到 `/usr/local/bin` 或其他在 PATH 中的目录

## 功能特点

- 支持多语言翻译文件的上传和下载
- 自动同步翻译内容
- 支持基准语言（base language）的设置
- 自动检查并提示版本更新
- 支持自定义文件包含和排除规则
- 提供详细的操作日志

## 配置

首次使用时，需要初始化配置文件：

```bash
i18n-app init
```

这将在当前目录创建 `.i18n-app.json` 配置文件，请根据实际情况修改配置：

```json
{
  "host": "https://your-api-host.com",
  "subSystemName": "your-system-name",
  "productCode": "your-product-code",
  "productId": 1,
  "versionNo": "1.0.0",
  "baseLanguage": "en-US",
  "previewMode": "1",
  "include": [
    "languages/*.json"
  ],
  "exclude": []
}
```

配置说明：
- `host`: API 服务器地址
- `subSystemName`: 子系统名称
- `productCode`: 产品代码
- `productId`: 产品 ID
- `versionNo`: 版本号
- `baseLanguage`: 基准语言（用于比对其他语言的翻译完整性）
- `previewMode`: 预览模式开关（"1"开启，"0"关闭）
- `include`: 要包含的文件匹配模式（支持 glob 语法）
- `exclude`: 要排除的文件匹配模式（支持 glob 语法）

## 使用方法

### 查看帮助信息

```bash
i18n-app --help
```

### 初始化配置

```bash
i18n-app init
```

### 上传翻译文件

```bash
# 上传默认目录下的翻译文件
i18n-app push

# 上传指定目录下的翻译文件
i18n-app push -p path/to/translations
```

### 下载翻译文件

```bash
# 下载到默认目录（.i18n-app/preview）
i18n-app download

# 下载到指定目录
i18n-app download -p path/to/save
```

### 同步翻译文件

```bash
# 从服务器同步最新翻译到本地配置的文件中
i18n-app pull
```

### 更新工具版本

程序会在运行时自动检查更新。你也可以手动运行以下命令来更新到最新版本：

```bash
i18n-app update
```

## 工作流程

1. **上传翻译 (push)**
   - 读取本地翻译文件
   - 对非基准语言，自动补充缺失的翻译键
   - 与服务器现有翻译比对
   - 上传新增的翻译内容，若是首次上传，则上传全部的内容

2. **下载翻译 (download)**
   - 从服务器获取最新翻译配置
   - 下载所有语言的翻译文件
   - 保存到指定目录

3. **同步翻译 (pull)**
   - 从服务器下载最新翻译
   - 根据配置文件中的 include 规则
   - 更新本地对应的翻译文件
   - 自动清理临时文件

## 开发相关

### 构建项目

```bash
cargo build --release
```

### 运行测试

```bash
cargo test
```

### 调试模式

在开发环境中，工具会输出更详细的调试信息。设置环境变量开启调试日志：

```bash
RUST_LOG=debug i18n-app <command>
```

## 常见问题

1. **更新失败**
   - 检查网络连接
   - 确认是否有足够的权限
   - 查看详细的错误日志

2. **翻译同步问题**
   - 确认配置文件中的 include/exclude 规则正确
   - 检查文件路径和权限
   - 确保基准语言文件存在

3. **API 调用限制**
   - 配置 GitHub Token 增加 API 限额
   - 避免频繁检查更新

## 贡献指南

欢迎提交 Issue 和 Pull Request！

1. Fork 本仓库
2. 创建你的特性分支 (`git checkout -b feature/AmazingFeature`)
3. 提交你的改动 (`git commit -m 'Add some AmazingFeature'`)
4. 推送到分支 (`git push origin feature/AmazingFeature`)
5. 开启一个 Pull Request

## 许可证

本项目采用 MIT 许可证 - 查看 [LICENSE](LICENSE) 文件了解详情

## 作者

Bob <subo@vanelink.net>
