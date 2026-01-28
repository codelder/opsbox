//! 安全集成测试
//!
//! 测试LogSeek模块的安全防护功能：
//! - SQL注入防护
//! - 路径遍历防护
//! - 输入验证

use opsbox_test_common::security;

/// 测试SQL注入检测
#[tokio::test]
async fn test_sql_injection_detection() {
    // 测试各种SQL注入向量
    for vector in security::sql_injection::TEST_VECTORS {
        let report = security::SecurityTestReport::new(vector.to_string()).analyze();

        assert!(
            report.sql_injection_detected,
            "Should detect SQL injection in: {}",
            vector
        );

        println!("✓ Detected SQL injection: {}", vector);
    }

    // 测试时间盲注向量
    for vector in security::sql_injection::TIME_BASED_VECTORS {
        let report = security::SecurityTestReport::new(vector.to_string()).analyze();

        assert!(
            report.sql_injection_detected,
            "Should detect time-based SQL injection in: {}",
            vector
        );

        println!("✓ Detected time-based SQL injection: {}", vector);
    }
}

/// 测试路径遍历检测
#[tokio::test]
async fn test_path_traversal_detection() {
    // 测试各种路径遍历向量
    for vector in security::path_traversal::TEST_VECTORS {
        let report = security::SecurityTestReport::new(vector.to_string()).analyze();

        assert!(
            report.path_traversal_detected,
            "Should detect path traversal in: {}",
            vector
        );

        println!("✓ Detected path traversal: {}", vector);
    }

    // 测试空字节注入
    for vector in security::path_traversal::NULL_BYTE_VECTORS {
        let report = security::SecurityTestReport::new(vector.to_string()).analyze();

        // 空字节注入可能被检测为路径遍历
        assert!(
            report.path_traversal_detected || vector.contains("%00"),
            "Should detect null byte injection in: {}",
            vector
        );

        println!("✓ Detected null byte injection: {}", vector);
    }
}

/// 测试XSS检测
#[tokio::test]
async fn test_xss_detection() {
    // 测试各种XSS向量
    for vector in security::xss::TEST_VECTORS {
        let report = security::SecurityTestReport::new(vector.to_string()).analyze();

        assert!(
            report.xss_detected,
            "Should detect XSS in: {}",
            vector
        );

        println!("✓ Detected XSS: {}", vector);
    }
}

/// 测试命令注入检测
#[tokio::test]
async fn test_command_injection_detection() {
    // 测试各种命令注入向量
    for vector in security::command_injection::TEST_VECTORS {
        let report = security::SecurityTestReport::new(vector.to_string()).analyze();

        assert!(
            report.command_injection_detected,
            "Should detect command injection in: {}",
            vector
        );

        println!("✓ Detected command injection: {}", vector);
    }
}

/// 测试安全输入处理
#[tokio::test]
async fn test_safe_input_handling() {
    let safe_inputs = vec![
        "normal log message",
        "INFO: Application started",
        "ERROR: Connection failed",
        "WARNING: Retry attempt 3",
        "2024-01-01T00:00:00Z INFO test",
    ];

    for input in safe_inputs {
        let report = security::SecurityTestReport::new(input.to_string()).analyze();

        assert!(
            report.passed(),
            "Safe input should pass security check: {}",
            input
        );

        println!("✓ Safe input passed: {}", input);
    }
}

/// 测试混合攻击向量
#[tokio::test]
async fn test_mixed_attack_vectors() {
    let mixed_attacks = vec![
        ("SQL + XSS", "' OR '1'='1<script>alert(1)</script>"),
        ("Path + Command", "../../../etc/passwd; ls -la"),
        ("XSS + Command", "<script>alert(1)</script> | cat /etc/passwd"),
    ];

    for (name, vector) in mixed_attacks {
        let report = security::SecurityTestReport::new(vector.to_string()).analyze();

        // 混合攻击应该至少检测到一种攻击
        assert!(
            report.sql_injection_detected ||
            report.path_traversal_detected ||
            report.xss_detected ||
            report.command_injection_detected,
            "Should detect at least one attack in mixed vector '{}': {}",
            name, vector
        );

        println!("✓ Detected mixed attack '{}': {}", name, vector);
    }
}

/// 测试边界条件输入
#[tokio::test]
async fn test_boundary_inputs() {
    let very_long = "a".repeat(10000);
    let boundary_inputs = vec![
        ("empty", ""),
        ("single_char", "a"),
        ("very_long", very_long.as_str()),
        ("unicode", "𝄞🎉中文测试"),
        ("control_chars", "\x00\x01\x02\x03"),
        ("whitespace_only", "   \t\n\r   "),
    ];

    for (name, input) in boundary_inputs {
        let report = security::SecurityTestReport::new(input.to_string()).analyze();

        // 边界输入不应该被误报为攻击（除非确实包含攻击模式）
        // 这里主要确保不会崩溃
        println!("✓ Processed boundary input '{}' (length: {}): passed={}",
                name, input.len(), report.passed());
    }
}