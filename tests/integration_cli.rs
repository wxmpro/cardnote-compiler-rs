//! CLI 集成测试 — 验证命令行参数解析和子命令路由
//!
//! 测试覆盖：
//! 1. CLI 参数解析
//! 2. 子命令识别
//! 3. 默认值
//! 4. 边界条件和错误处理

use std::process::Command;

// ═══════════════════════════════════════════════════════
//  二进制文件调用测试
// ═══════════════════════════════════════════════════════

/// 获取 cardc 二进制路径
fn cardc_binary() -> std::path::PathBuf {
    // 优先使用 CARGO_BIN_EXE_cardc 环境变量（cargo test 自动设置）
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_cardc") {
        return std::path::PathBuf::from(path);
    }

    // 回退：从 target/debug 或 target/release 查找
    let path = std::env::current_dir().unwrap();
    for dir in &["target/debug/cardc", "target/release/cardc"] {
        let candidate = path.join(dir);
        if candidate.exists() {
            return candidate;
        }
    }

    // 最后尝试
    path.join("target/debug/cardc")
}

#[test]
fn test_cli_help() {
    let output = Command::new(cardc_binary())
        .arg("--help")
        .output()
        .expect("执行 cardc --help 失败");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("CardNote Compiler"));
    assert!(stdout.contains("--output"));
    assert!(stdout.contains("--provider"));
    assert!(stdout.contains("--model"));
}

#[test]
fn test_cli_version() {
    let output = Command::new(cardc_binary())
        .arg("--version")
        .output()
        .expect("执行 cardc --version 失败");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // 从 Cargo.toml 读取版本号，避免硬编码
    let expected_version = env!("CARGO_PKG_VERSION");
    assert!(
        stdout.contains(expected_version),
        "版本输出应包含 {}，实际: {}",
        expected_version,
        stdout
    );
}

#[test]
fn test_cli_subcommand_init_help() {
    let output = Command::new(cardc_binary())
        .args(["init", "--help"])
        .output()
        .expect("执行 cardc init --help 失败");

    // init 是有效的子命令
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    // 可能通过 --help 输出或成功执行
    assert!(output.status.success() || stdout.contains("init") || stderr.contains("init"));
}

#[test]
fn test_cli_subcommand_doctor_help() {
    let output = Command::new(cardc_binary())
        .args(["doctor", "--help"])
        .output()
        .expect("执行 cardc doctor --help 失败");

    assert!(output.status.success());
}

#[test]
fn test_cli_subcommand_scan_help() {
    let output = Command::new(cardc_binary())
        .args(["scan", "--help"])
        .output()
        .expect("执行 cardc scan --help 失败");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("DIR") || stdout.contains("dir") || stdout.contains("目录"));
}

#[test]
fn test_cli_subcommand_history_help() {
    let output = Command::new(cardc_binary())
        .args(["history", "--help"])
        .output()
        .expect("执行 cardc history --help 失败");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--limit") || stdout.contains("limit"));
}

#[test]
fn test_cli_subcommand_quality_help() {
    let output = Command::new(cardc_binary())
        .args(["quality", "--help"])
        .output()
        .expect("执行 cardc quality --help 失败");

    assert!(output.status.success());
}

#[test]
fn test_cli_default_output_dir() {
    let output = Command::new(cardc_binary())
        .arg("--help")
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("./output"));
}

#[test]
fn test_cli_force_flag() {
    let output = Command::new(cardc_binary())
        .arg("--help")
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--force"));
}

#[test]
fn test_cli_missing_file_shows_error() {
    let output = Command::new(cardc_binary())
        .arg("/nonexistent/file_that_does_not_exist_12345.md")
        .output()
        .expect("执行 cardc 失败");

    // 应该非成功退出
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{}{}", stdout, stderr);
    // 错误消息应包含文件未找到或类似含义
    assert!(
        !output.status.success()
            || combined.contains("错误")
            || combined.contains("找不到")
            || combined.contains("not found")
            || combined.contains("File")
            || combined.contains("不存在")
            || combined.contains("路径"),
        "不存在的文件应导致错误退出或显示错误消息。stdout: {}, stderr: {}",
        stdout,
        stderr
    );
}

#[test]
fn test_cli_path_traversal_rejected() {
    let output = Command::new(cardc_binary())
        .arg("../etc/passwd")
        .output()
        .expect("执行 cardc 失败");

    // .. 路径应被拒绝
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{}{}", stdout, stderr);
    assert!(
        !output.status.success()
            || combined.contains("非法")
            || combined.contains("遍历")
            || combined.contains("错误"),
        "路径遍历应被拒绝"
    );
}

// ═══════════════════════════════════════════════════════
//  子命令存在性验证（通过 --help）
// ═══════════════════════════════════════════════════════

#[test]
fn test_all_subcommands_listed_in_help() {
    let output = Command::new(cardc_binary())
        .arg("--help")
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let expected_subcommands = [
        "init", "doctor", "quality", "scan", "history", "review",
    ];

    for cmd in &expected_subcommands {
        assert!(
            stdout.contains(cmd),
            "子命令 '{}' 应在 --help 输出中列出",
            cmd
        );
    }
}

// ═══════════════════════════════════════════════════════
//  参数组合测试
// ═══════════════════════════════════════════════════════

#[test]
fn test_cli_provider_and_model_flags() {
    let output = Command::new(cardc_binary())
        .arg("--help")
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--provider"));
    assert!(stdout.contains("--model"));
    assert!(stdout.contains("--api-key"));
    assert!(stdout.contains("--base-url"));
}

#[test]
fn test_cli_scan_subcommand_flags() {
    let output = Command::new(cardc_binary())
        .args(["scan", "--help"])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--recursive") || stdout.contains("-r"));
    assert!(stdout.contains("--threshold") || stdout.contains("-t"));
}
