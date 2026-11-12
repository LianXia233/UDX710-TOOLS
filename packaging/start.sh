#!/bin/sh
# UDX710-TOOLS 启动脚本（systemd ExecStart 直接调用本脚本）。
# 约定：server 与 start.sh 位于同一目录（部署目录 /home/root/6677）。
#
# 端口：默认 6677，可用第一个位置参数覆盖：
#   ./start.sh            -> 监听 6677
#   ./start.sh 8080       -> 监听 8080
#   UDX710_PORT=8080 ./start.sh
# 端口以环境变量 UDX710_PORT 传给 server，同时作为首个位置参数透传；
# server 端以 UDX710_PORT 为权威端口来源（报告第 03/05 章配置约定）。
#
# 回滚说明（详见 packaging/README.md）：
#   部署新版本前先 `cp server server.bak` 保留上一版可执行文件。
#   若新版本启动失败 / 健康检查不通过，`cp server.bak server` 并重启服务即可回退；
#   git 历史里的上一版本同样是代码层回退点。

set -eu

# cd 到脚本自身所在目录，保证相对路径 ./server 始终有效（与调用方 cwd 无关）。
DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
cd "$DIR"

# 首次解压 / 上传后 x 位可能丢失，补齐可执行权限。
chmod 755 ./server ./start.sh

PORT=${1:-${UDX710_PORT:-6677}}
export UDX710_PORT="$PORT"

# 用 exec 替换当前 shell，让 systemd 直接管理 server 进程（信号与 Restart 策略生效）。
exec ./server "$@"
