# chrono-harness-cache

计算、恢复和封存构建缓存。

使用 `cargo build --locked --manifest-path crates/cache/Cargo.toml` 构建。运行 `mise trust` 后执行 `mise exec -- ./check.sh`，根据 Git 变更选择已登记的格式、构建、测试及证明。

源码来自 [固定版本的原实现](https://github.com/ChronoAIProject/chrono-harness/tree/5f6892dfa2f73675a84e4ebdc3b06eb83b824d6e/crates/cache)。

本仓可从 crates.io 和本仓源码分别构建产品与测试。只读取文件或声明值的规划可直接运行。命令探测、原件保管和恢复使用已安装的 `chrono-harness retained-artifacts-v1`；启用宿主日志时，`chrono-diagnostics cli-events-v1` 将本命令的 PID、业务事件、原始错误与退出码写入同一个滚动日志。

缓存库测试用 Rust 替身驱动保管与日志协议，覆盖声明的消费者、动作参数变化、源文件刷新和恢复。旧组合 CI 的检测、Verus 下载及发布配方对应的六项采用检查归实际宿主，新组合采用缓存时需按其配方验收。

本仓从 dev 的预发布标签和指向 main 的正式标签发布 Linux x86_64、macOS arm64 二进制及 SHA256SUMS。发布时，两个平台各用实际编译的命令读取 Cargo.lock 和 Cargo.toml，核对缓存计划选择的生产操作和缓存键，并验证未登记消费者会被拒绝。

本仓的 macOS 原生检查通过 `.chrono-harness/ci/check.json` 登记唯一保存作业。宿主安装的 `chrono-ci generate` 和 `verify` 生成、核对 `.github/actions/native-check/action.yml`。本机和 Ubuntu 的按变更检查依据 `.chrono-harness/FILEMAP.json` 选择两条 Verus 根及 Rust 消费者，汇总读取单元原件。GitHub 调用该脚本的 `build` 阶段编译产品与全部测试并封存输出，之后调用 `verify` 执行本轮测试和证明。计划按 `.chrono-harness/cache.json` 绑定本仓锁文件、清单、源文件、编译器、平台、链接器、SDK 和生效的增量设置。相同编译配置下可以恢复旧源码的 Cargo 产物，然后重跑构建、测试和证明。

依赖包和索引在 `target/cargo-home/registry`，编译产物在 `target/native-cargo`，本轮检查原件在 `.chrono-harness/state/cache`，分别由恢复步骤、Cargo 和本轮诊断消费。GitHub 仓库的缓存保留上限为 7 天，每个托管 job 使用独立检出。成功作业的计划和恢复报告随本轮检出回收；失败作业把诊断原件上传至 GitHub，保留 1 天。遇到非零或校验错误，GitHub 作业日志保存本次处理结果。
