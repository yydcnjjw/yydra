<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# 用 yydra-cli 跑起第一个产品

本教程面向首次使用 Yydra 的产品开发者。在 Linux 的 Bash 终端中，
你会安装 `yydra`、创建一个名为 Reader 的 **Product Workspace**，
启动后端和浏览器应用，再完成一条 Reading Queue 记录的新增、完成和重新打开。
最后生成后端与 H5 构建产物。

**适用版本：Yydra Distribution 0.5.0 开发候选版。** 截至 2026-09-10，
该版本尚未发布，本教程从固定源码提交打包安装。
[已发布的 0.1.0](https://github.com/yydcnjjw/yydra/releases/tag/distribution-v0.1.0)
有自己的 CLI 和约定，不能用来执行本教程。

`yydra-cli` 是 Rust 包名，安装后的命令叫 `yydra`。
Product Workspace 是创建后由产品团队独立维护的产品代码库，
与用于打包 CLI 的 Yydra 源码目录分开。术语见[领域词汇表](../../CONTEXT.md)。

希望直接构建并运行服务端容器，请阅读[服务端 Compose 教程](server-compose.md)。
该能力需要包含容器配置的新 Product Workspace；本教程固定的旧提交不包含它，
下文命令仍按旧快照保留。

当前主线已按 [ADR 0009](../adr/0009-consolidate-diagnostics-in-doctor.md)
移除 `check` 并扩展 `doctor` 的环境诊断。本教程固定旧提交中的 `check`
命令仍按当时行为保留；使用当前源码时请遵循[当前 CLI 指南](../../crates/yydra-cli/README.md)。

## 1. 准备终端和工具

准备一台能够访问 Cargo、npm、GitHub 和容器镜像仓库的 Linux 主机，
并在这台主机上使用浏览器。首次安装和编译需要下载依赖、镜像并占用磁盘空间。
本教程不覆盖远程主机的浏览器转发和 Windows PowerShell。

| 工具 | 准备方式与用途 |
| --- | --- |
| Bash、Git、`tar`、`sha256sum`、`curl`、C/C++ 编译工具 | 使用发行版的包管理器安装；用于获取源码、打包和本地编译 |
| Rustup | 按 [Rust 官方安装说明](https://rust-lang.org/tools/install/)安装，再安装下面的 nightly 工具链 |
| Node.js 与 npm | 按 [Node.js 官方下载页](https://nodejs.org/en/download)安装；本教程选用 Node.js 26.8.2、npm 12.0.2 |
| Docker Engine 与 Compose 插件 | 按 [Docker 官方 Linux 安装说明](https://docs.docker.com/engine/install/)安装，确保当前用户能访问运行中的 Docker 服务 |

在 Bash 中运行：

```bash
rustup toolchain install nightly --profile minimal --component rustfmt,clippy
cargo +nightly --version
rustc +nightly --version
node --version
npm --version
docker info
docker compose version
```

成功标志：各工具可执行，`docker info` 能连接服务，Compose 能报告版本。
Node/npm 版本是本教程的环境选择；CLI 会记录实际版本，并没有新增精确版本门禁。
前端依赖要求以锁文件内的 `engines` 为准，遇到版本不满足时先修复环境。
Rust 使用滚动 `nightly`；如需刷新已安装的 nightly，显式执行
`rustup update nightly`。构建过程不会替你更新它。

以下命令按顺序在**同一个 Bash 终端**执行；每一步成功后再继续。
选择一个尚不存在、位于已有 Cargo workspace 之外的练习目录：

```bash
tutorial_root="$HOME/yydra-tutorial"
mkdir "$tutorial_root"
cd "$tutorial_root"
```

如果目录已经存在，先把 `tutorial_root` 改成另一个未使用的绝对路径。
这个目录会容纳源码、安装包、独立 CLI 安装目录和 Reader 产品。

## 2. 从固定源码打包并安装 CLI

执行目录：`$tutorial_root`。本教程固定源码提交
[`96ee68f4284909067a4bac03c912046410867def`](https://github.com/yydcnjjw/yydra/commit/96ee68f4284909067a4bac03c912046410867def)，
避免以后 `main` 的命令或模板变化影响练习。

```bash
git clone https://github.com/yydcnjjw/yydra.git yydra-source
cd "$tutorial_root/yydra-source"
git checkout --detach 96ee68f4284909067a4bac03c912046410867def
git status --short
cargo +nightly package --package yydra-cli --locked \
  --target-dir "$tutorial_root/package-target"
```

`git status --short` 应没有输出。打包会验证包能够编译；成功后得到
`$tutorial_root/package-target/package/yydra-cli-0.5.0.crate`。
如果打包失败，保留并处理错误，不跳过验证继续安装。

为本次生成的包记录校验值，然后解包到单独目录：

```bash
cd "$tutorial_root/package-target/package"
sha256sum yydra-cli-0.5.0.crate > yydra-cli-0.5.0.crate.sha256
sha256sum --check yydra-cli-0.5.0.crate.sha256
mkdir "$tutorial_root/unpacked"
tar -xzf yydra-cli-0.5.0.crate -C "$tutorial_root/unpacked"
```

校验应显示 `OK`。这里的校验值标识你刚生成的本地包；它不是官方发布包的签名，
也不表示这次练习已完成 Distribution 发布验收。保留源码提交、包和校验文件。

使用包内的锁文件安装到练习目录，再把这个 CLI 放到当前终端的 `PATH` 前面：

```bash
cargo +nightly install yydra-cli@0.5.0 \
  --path "$tutorial_root/unpacked/yydra-cli-0.5.0" \
  --locked \
  --root "$tutorial_root/cli-0.5.0" \
  --target-dir "$tutorial_root/install-target"
export PATH="$tutorial_root/cli-0.5.0/bin:$PATH"
command -v yydra
yydra --version
yydra --help
```

成功标志：命令路径是练习目录中的 `cli-0.5.0/bin/yydra`，版本为 `yydra 0.5.0`。
这套安装独立于机器上其他版本的 CLI。后面始终用它操作本次创建的产品。

## 3. 创建 Reader，安装产品依赖

回到练习目录，创建 Product Workspace：

```bash
cd "$tutorial_root"
yydra new ./reader \
  --product-name "Reader" \
  --product-id reader \
  --product-source-license Apache-2.0
cd "$tutorial_root/reader"
yydra doctor .
yydra setup .
```

本例选择 `Apache-2.0` 作为产品团队新写源码的许可；复制进来的 Yydra 源码仍保留
`MIT OR Apache-2.0`，第三方源码保留各自的许可和声明。
目标 `reader` 必须不存在或为空，`new` 不会把模板合并进已有产品。

成功标志：目录里有 `Cargo.toml`、`frontend/`、`compose.yaml` 和 `.yydra/origin.toml`；
`doctor` 报告 `DOCTOR_WORKSPACE_VERIFY` 通过，`setup` 成功结束。
`setup` 会执行 `cargo fetch --locked` 和 `npm ci --no-audit`，使用产品已有的两个锁文件。

`doctor` 核对 Workspace Origin Record 和精确快照；它不检查 Node、Docker 或 Android SDK
是否可用。`setup` 安装依赖，数据库还需要在下一步启动。

## 4. 启动数据库、后端与 H5

执行目录：`$tutorial_root/reader`。先启动模板指定的 PostgreSQL 容器：

```bash
docker compose up -d --wait postgres
docker compose ps postgres
export DATABASE_URL=postgres://postgres:postgres@127.0.0.1:55432/yydra_product
export YYDRA_READING_QUEUE_CURSOR_SIGNING_KEY="$(node -e 'process.stdout.write(require("node:crypto").randomBytes(32).toString("hex"))')"
```

成功标志：PostgreSQL 显示为运行且健康。模板将数据库端口绑定到本机 `55432`。
环境变量必须在启动 `dev` 的同一终端中导出；仅把它们写入 `.env` 不会自动生效。
上面的 Node 命令生成本地练习用的随机游标签名密钥，不会把密钥打印到终端。
服务要求该密钥至少 32 字节；不要把它写入源码或日志。

**这个数据库用于临时练习。** 模板的 PostgreSQL 数据目录挂载在 `tmpfs` 上，
停止或移除数据库容器会丢失数据。仅停止 `yydra dev` 会保留仍在运行的数据库容器。
需要长期保留数据时，应先为产品另行配置持久存储。

开始开发：

```bash
yydra dev .
```

保持这个终端运行。CLI 依次执行 `dev.migration`、启动 `dev.backend` 与
`dev.frontend`。首次运行会编译 Rust 后端、准备 API 客户端并启动 Expo，可能需要等待。
后端默认使用 `http://127.0.0.1:4000`，H5 通常使用 `http://localhost:8081`；
以 Expo 实际打印的浏览器地址为准。

`dev` 的第一阶段会应用已提交的数据库迁移。单独的 server 进程只检查数据库结构，
不会应用迁移。如需只执行迁移，可在停止开发进程后运行 `yydra db migrate .`。

另开一个终端检查后端，保留原终端运行：

```bash
curl --fail http://127.0.0.1:4000/health
```

成功标志：健康接口返回成功响应，浏览器页面出现 `Reading Queue` 和新增表单。
若刚启动时连接失败，先等编译与服务启动完成再试；持续失败则检查原终端的错误。

## 5. 在浏览器完成一次操作

打开 Expo 打印的 H5 地址，执行下面的步骤。界面控件保留模板的英文文案：

1. 在 `Entry title` 填入 `My first entry`，在 `Source URL` 填入
   `https://example.com/article`，点击 `Add entry`。
2. 确认列表出现这条记录，再点击 `Complete My first entry` 按钮。
3. 选择 `Completed entries`，确认它出现在已完成列表。
4. 点击 `Reopen My first entry` 按钮，再选择 `Queued entries`，确认记录重新回到待阅读列表。
5. 刷新浏览器，确认记录仍在；数据库容器此时应保持运行。

这条流程确认浏览器操作能够通过后端读写 PostgreSQL。
它完成本教程的人工操作目标；更多检查见第 8 节。

## 6. 停止与重新启动

在运行 `yydra dev` 的终端按 **Ctrl-C**，等待开发子进程退出、终端提示符恢复。
CLI 会一起终止后端和前端。此时数据库容器仍在运行。

在**同一个终端**重新启动，已有环境变量和练习数据继续可用：

```bash
cd "$tutorial_root/reader"
yydra dev .
```

如果换了终端，需要重新设置 `tutorial_root`、CLI 的 `PATH`、`DATABASE_URL` 和
`YYDRA_READING_QUEUE_CURSOR_SIGNING_KEY`。重新生成密钥会使旧分页游标失效；
回到列表第一页重新读取即可，记录本身不会因此删除。

完成练习、允许丢弃数据库中的记录后，先按 Ctrl-C 停止开发，再执行：

```bash
docker compose down
```

以后重新执行第 4 节时，会启动空数据库，`dev` 的迁移阶段会重新创建表结构。

## 7. 构建后端和 H5 产物

先停止 `dev`，在 `$tutorial_root/reader` 执行：

```bash
yydra build .
```

成功标志：CLI 分别报告后端和 H5 的 `BUILD_ARTIFACT_READY`，并给出实际产物路径。
在未覆盖 Cargo 输出目录的本教程环境中，后端位于 `target/release/server`，
H5 位于 `frontend/dist/`，其中应有 `index.html`。

默认构建会编译 release 后端、检查前端类型并导出 H5 静态资源，API 客户端准备自动执行。
构建无需先启动数据库；它不会执行迁移、启动服务或发布产物。
H5 的 API 地址在构建时确定，本例使用本地后端默认地址，产物还不是线上部署配置。
不要直接用浏览器打开 `dist/index.html` 代替通过 HTTP 服务运行应用。

只需要一个目标时，可以使用：

```bash
yydra build . --target server
yydra build . --target h5
```

## 8. 下一步：诊断与质量检查

下面的命令都在 Reader 的 Product Workspace 中执行。

| 想完成的事 | 命令 | 结果范围 |
| --- | --- | --- |
| 检查来源和快照是否匹配 CLI | `yydra doctor .` | Workspace 身份和快照检查 |
| 获取结构化诊断 | `yydra --message-format json doctor .` | 版本化 JSON Lines 输出 |
| 定点检查来源与快照 | `yydra check . --node ownership.generated-snapshots` | 执行该节点及其前置节点，结果为 `complete=false` |
| 执行完整本地质量检查 | `yydra check .` | 包含真实 PostgreSQL、H5 与 Android 构建等完整本地图 |
| 构建 Android APK | `yydra build . --target android` | 显式 Android 构建目标 |

完整 `check` 和 Android 构建另需准备相应的浏览器、JDK 和 Android SDK 等工具，
请先阅读 [CLI 指南](../../crates/yydra-cli/README.md)及产品自己的 `README.md`。
运行 H5 自动检查前停止开发服务，避免争用 `8081`。
本教程的后端/H5 构建和定点检查都不能替代完整 Mechanical Quality Contract。
完整本地检查通过也不等于 Distribution 的聚合符合性结论；本地和聚合证据都受各自的
验证范围约束。

`check` 默认把证据写入 Product Workspace 外的临时目录；需要指定保留位置时，
`--evidence-dir` 必须是 Workspace 外且没有符号链接祖先的路径。
检查结果会报告实际证据位置。完整命令、证据边界和 Android 准备说明见 CLI 指南。

## 常见问题

| 现象 | 检查与处理 |
| --- | --- |
| `yydra` 不存在或版本不对 | 用 `command -v yydra` 确认命令路径，重新导出第 2 节的 `PATH`；本教程使用自己打包的 0.5.0 |
| `doctor` 报版本或快照不匹配 | 使用创建该 Workspace 的原 CLI 和包；依照诊断恢复被改动的快照，不手改 `.yydra/origin.toml` 绕过检查 |
| `new` 拒绝目标目录 | 选择另一个不存在或为空的目录；不要在已有产品上重复执行创建 |
| `setup` 无法下载依赖或提示 Node 版本不满足 | 检查 Cargo/npm 网络和前端依赖的 Node 要求，修好环境后重新执行 `setup`，保留两个锁文件 |
| Docker 无法连接或数据库不健康 | 检查 `docker info`、`docker compose ps postgres` 与 `docker compose logs postgres`；确认模板镜像可下载 |
| `55432` 被占用 | 为本产品设置另一个 `YYDRA_POSTGRES_PORT` 后启动 Compose，并同步修改 `DATABASE_URL` 中的端口 |
| `dev.migration` 失败 | 检查数据库是否健康、当前终端是否导出正确的 `DATABASE_URL`；修复后用 `yydra db migrate .` 单独重试 |
| 后端提示签名密钥缺失或过短 | 在启动 `dev` 的终端执行第 4 节的密钥设置，再重启 |
| H5 打开了但加载队列失败 | 用 `/health` 检查后端；默认 API 地址为 `http://127.0.0.1:4000`。若设置 `YYDRA_BIND_ADDRESS` 改了后端地址，也要在启动或构建前设置对应的 `EXPO_PUBLIC_API_URL` |
| API 客户端准备或构建失败 | 先确认 `setup` 成功，查看 `dev`/`build` 转发的 Cargo 和 npm 错误；客户端由构建流程生成，修复输入后重新执行命令 |
| 重启数据库后记录消失 | 模板使用 `tmpfs`；这是第 4、6 节说明的临时存储行为 |

继续开发时，从产品目录中的 `README.md` 和 `.agents/skills/yydra-product-change/`
了解业务修改路径。Product Workspace 由产品团队独立演进；创建操作不提供模板同步、
自动升级或版本覆盖功能。
