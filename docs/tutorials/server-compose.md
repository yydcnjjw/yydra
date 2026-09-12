<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# 一条 Compose 命令运行服务端

本教程适用于包含 `Dockerfile`、`container/entrypoint.sh` 和新 `compose.yaml`
的 Product Workspace。此功能随当前源码候选提供，尚未发布；旧版产品及
[固定旧提交的开发教程](yydra-cli-getting-started.md)不包含这些配置。

在开发机器上用包含此功能的 CLI 执行 `yydra new` 创建产品，再将产品源码目录
复制或克隆到运行机器。若从本仓库源码创建，可在仓库根目录执行下列命令，
将目标换成仓库及其他 Cargo workspace 之外、尚不存在的绝对路径：

```bash
cargo run --locked --package yydra-cli -- new /path/to/reader \
  --product-name Reader --product-id reader --product-source-license Apache-2.0
```

当前开发模板从本地包仓库获取认证库。首次构建前，在同一台 Linux 构建机器的
匹配 Yydra 源码目录中执行：

```bash
python3 scripts/local-packages.py up
python3 scripts/local-packages.py publish
```

仓库准备需要 Docker Engine 与 Compose 插件、Python 3.11+、Rust nightly 和
Node/npm，详见[本地包仓库说明](../../dev/local-packages/README.md)。容器构建会自行
安装 nightly 并编译后端，需要访问本地仓库、镜像、Rust 和 Cargo 下载服务。
下列 override 让 Linux 构建阶段访问宿主回环地址；运行容器仍使用普通 Compose 网络。
若将已构建镜像移至另一台运行机器，该机器只需要 Docker 和 Compose，无需包仓库
或宿主语言工具链。

## 构建并启动

在运行机器进入产品目录：

```bash
cd /path/to/reader
docker compose -f compose.yaml -f compose.local-registry.yaml up --build --wait
```

命令会构建服务端镜像，首次生成随机数据库密码和游标签名密钥，启动持久化
PostgreSQL，执行数据库迁移，再启动后端并等待健康检查通过。首次编译需要等待。
后端 API 就绪后可检查：

```bash
curl --fail http://127.0.0.1:4000/health
docker compose ps
```

`postgres`、`server` 应处于健康状态；`init` 和 `migrate` 是完成后正常退出的
一次性任务。任一步失败会使启动命令失败。H5 页面不包含在这套服务端部署中。

API 默认只绑定运行机器的 `127.0.0.1:4000`。可通过 shell 环境或产品目录的
本地 `.env` 设置 `YYDRA_SERVER_PORT`、`YYDRA_SERVER_HOST`，例如将绑定地址
改为 `0.0.0.0`。PostgreSQL 不向宿主机发布端口。当前 0.6.0 模板的 Reading Queue
要求产品会话，并按账号隔离数据。未配置 GitHub 时健康检查仍可通过，但不能登录。
启用登录前，按产品 `README.md` 的 **GitHub sign-in** 部分配置 GitHub App、
`GITHUB_CLIENT_ID`、`GITHUB_CLIENT_SECRET`、公开 API 地址及 H5/Android 回调地址。
生产环境还需 HTTPS 和同站点的 H5/API 部署；Compose 不提供 TLS 代理或 H5 托管。
客户端密钥仅交给后端，不写入前端公开环境变量。

## 停止、更新与数据

```bash
docker compose logs --tail 100 server migrate
docker compose stop
docker compose start --wait
```

更新产品源码后，再执行上述包含 `compose.local-registry.yaml` 的启动命令，重建镜像并应用
新的前向迁移。只想删除容器时使用 `docker compose down`；随后仍可用同一条
启动命令重建容器。

数据库和密钥分别保存在项目的 `postgres-data` 与 `credentials` 命名卷中。
停止、删除容器和重建镜像不会删除这些卷；备份或迁移部署时应一起保存两者。
`docker compose down --volumes` 会明确删除数据库和密钥，不用于日常更新。

这是允许停机的单机部署流程。更新不是原子切换，也不自动回滚数据库；迁移
失败时命令返回失败，旧服务可能仍在运行或因结构变化而变得不健康，应查看
迁移日志并向前修复。丢失密钥卷也不会自动恢复已有数据库的密码。

## 只构建镜像

```bash
docker compose -f compose.yaml -f compose.local-registry.yaml build server
```

默认镜像名为 `reader-server:local`，产品 ID 不同时随之变化，也可用
`YYDRA_SERVER_IMAGE` 覆盖。镜像内同时有 `server` 和 `migrate`；独立运行时
提供 `DATABASE_URL`，先运行 `migrate`，再提供
`YYDRA_READING_QUEUE_CURSOR_SIGNING_KEY` 运行默认的 `server`。发布镜像到
仓库是单独操作。

默认部署项目名是 `<产品 ID>-server`，开发项目名是 `<产品 ID>-dev`，两者独立。
开发与自动检查仍使用 `compose.dev.yaml` 中的临时数据库。需要回到本地
开发流程时，请按当前产品 README 使用
`docker compose -f compose.dev.yaml up -d --wait postgres`。
