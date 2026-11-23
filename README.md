# chrono-harness-cache

计算、恢复和封存构建缓存。

使用 `cargo build --locked --manifest-path crates/cache/Cargo.toml` 构建。安装 Verus 0.2026.04.24.f8e1704，运行 `VERUS=/path/to/verus ./check.sh` 检查格式、构建、测试及证明。

源码来自 [固定版本的原实现](https://github.com/ChronoAIProject/chrono-harness/tree/5f6892dfa2f73675a84e4ebdc3b06eb83b824d6e/crates/cache)。

本仓可从 crates.io 和本仓源码分别构建产品与测试。只读取文件或声明值的规划可直接运行。命令探测、原件保管和恢复使用已安装的 `chrono-harness retained-artifacts-v1`；启用宿主日志时，`chrono-diagnostics cli-events-v1` 将本命令的 PID、业务事件、原始错误与退出码写入同一个滚动日志。

缓存库测试用 Rust 替身驱动保管与日志协议，覆盖声明的消费者、动作参数变化、源文件刷新和恢复。旧组合 CI 的检测、Verus 下载及发布配方对应的六项采用检查归实际宿主，新组合采用缓存时需按其配方验收。

本仓从 dev 的预发布标签和指向 main 的正式标签发布 Linux x86_64、macOS arm64 二进制及 SHA256SUMS。发布时，两个平台各用实际编译的命令读取 Cargo.lock 和 Cargo.toml，核对缓存计划选择的生产操作和缓存键，并验证未登记消费者会被拒绝。
