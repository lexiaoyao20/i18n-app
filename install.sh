
#!/bin/bash

set -e

GITHUB_REPO="lexiaoyao20/i18n-app"
BINARY_NAME="i18n-app"

# 检测系统类型和架构
detect_platform() {
    local OS
    local ARCH
    OS=$(uname -s | tr '[:lower:]' '[:lower:]')
    ARCH=$(uname -m)
    
    case "$ARCH" in
        x86_64) ARCH="x86_64" ;;
        aarch64|arm64) ARCH="aarch64" ;;
        *) echo "不支持的架构: $ARCH"; exit 1 ;;
    esac
    
    case "$OS" in
        linux) OS="linux" ;;
        darwin) OS="darwin" ;;
        *) echo "不支持的操作系统: $OS"; exit 1 ;;
    esac
    
    echo "${OS}-${ARCH}"
}

# 获取最新发布版本
get_latest_version() {
    curl -sL "https://api.github.com/repos/${GITHUB_REPO}/releases/latest" | \
    grep '"tag_name":' | \
    sed -E 's/.*"([^"]+)".*/\1/'
}

main() {
    echo "开始安装 ${BINARY_NAME}..."
    
    PLATFORM=$(detect_platform)
    VERSION=$(get_latest_version)
    
    if [ -z "$VERSION" ]; then
        echo "无法获取最新版本"
        exit 1
    }
    
    DOWNLOAD_URL="https://github.com/${GITHUB_REPO}/releases/download/${VERSION}/${BINARY_NAME}-${PLATFORM}"
    
    echo "下载 ${BINARY_NAME} ${VERSION} for ${PLATFORM}..."
    
    # 创建临时目录
    TMP_DIR=$(mktemp -d)
    curl -sL "$DOWNLOAD_URL" -o "${TMP_DIR}/${BINARY_NAME}"
    chmod +x "${TMP_DIR}/${BINARY_NAME}"
    
    # 移动到 /usr/local/bin
    if [ -w "/usr/local/bin" ]; then
        mv "${TMP_DIR}/${BINARY_NAME}" "/usr/local/bin/${BINARY_NAME}"
    else
        sudo mv "${TMP_DIR}/${BINARY_NAME}" "/usr/local/bin/${BINARY_NAME}"
    fi
    
    rm -rf "$TMP_DIR"
    
    echo "${BINARY_NAME} 已成功安装到 /usr/local/bin/${BINARY_NAME}"
    echo "运行 '${BINARY_NAME} --help' 查看使用说明"
}

main