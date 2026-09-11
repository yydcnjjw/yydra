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

创建产品仍需开发机器的 Rust 工具链。运行机器只需要可用的 Docker Engine 和
Compose 插件，以及访问镜像、Rust 和 Cargo 下载服务的网络。容器构建会自行
安装 nightly 并编译后端，不要求运行机器安装 Rust、Node、npm 或 Yydra CLI。

## 构建并启动

在运行机器进入产品目录：

```bash
cd /path/to/reader
docker compose up --build --wait
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
改为 `0.0.0.0`。PostgreSQL 不向宿主机发布端口。模板中的 Reading Queue
允许匿名访问，受保护端点只是鉴权契约示例；面向公网的真实产品仍需配置自身
的访问控制与 HTTPS。

## 停止、更新与数据

```bash
docker compose logs --tail 100 server migrate
docker compose stop
docker compose start --wait
```

更新产品源码后，再执行 `docker compose up --build --wait`，重建镜像并应用
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
docker compose build server
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
