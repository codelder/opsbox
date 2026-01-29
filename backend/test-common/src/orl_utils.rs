//! ORL测试工具
//!
//! 提供ORL（OpsBox Resource Locator）相关的测试工具：
//! - ORL字符串生成
//! - 恶意ORL检测
//! - ORL解析验证

use crate::TestError;

/// ORL类型枚举
#[derive(Debug, Clone, Copy)]
pub enum OrlType {
  Local,
  Agent,
  S3,
}

/// ORL生成器
pub struct OrlGenerator;

impl OrlGenerator {
  /// 创建本地文件ORL
  pub fn local(path: &str) -> String {
    if path.starts_with("orl://") {
      path.to_string()
    } else {
      format!("orl://local{}", path)
    }
  }

  /// 创建Agent ORL
  pub fn agent(agent_name: &str, path: &str, server_addr: Option<&str>) -> String {
    let base = if let Some(addr) = server_addr {
      format!("orl://{}@agent.{}{}", agent_name, addr, path)
    } else {
      format!("orl://{}@agent{}", agent_name, path)
    };
    base
  }

  /// 创建S3 ORL
  pub fn s3(profile_name: &str, path: &str) -> String {
    format!("orl://{}@s3{}", profile_name, path)
  }

  /// 创建带归档入口的ORL
  pub fn with_entry(orl: &str, entry_path: &str) -> String {
    if orl.contains('?') {
      format!("{}&entry={}", orl, entry_path)
    } else {
      format!("{}?entry={}", orl, entry_path)
    }
  }

  /// 创建带查询参数的ORL
  pub fn with_query(orl: &str, query_params: &[(&str, &str)]) -> String {
    let mut result = orl.to_string();

    for (i, (key, value)) in query_params.iter().enumerate() {
      if i == 0 && !orl.contains('?') {
        result.push('?');
      } else {
        result.push('&');
      }
      result.push_str(&format!("{}={}", key, value));
    }

    result
  }
}

/// 恶意ORL测试向量
pub mod malicious {
  /// 路径遍历攻击向量
  pub const PATH_TRAVERSAL: &[&str] = &[
    "orl://local/../../../etc/passwd",
    "orl://local/..\\..\\..\\windows\\system32",
    "orl://local/var/log/../../../../etc/shadow",
    "orl://local/../../../../../../../../etc/passwd",
    "orl://local/C:\\Windows\\..\\..\\System32",
    "orl://local//etc//passwd",
    "orl://local/./././etc/passwd",
  ];

  /// 空字节注入向量
  pub const NULL_BYTE: &[&str] = &[
    "orl://local/var/log/access.log%00",
    "orl://local/var/log%00test/access.log",
    "orl://local/var/log/access.log%00.jpg",
    "orl://local/var/log%00",
  ];

  /// 命令注入向量
  pub const COMMAND_INJECTION: &[&str] = &[
    "orl://local/var/log/| ls -la",
    "orl://local/var/log/; cat /etc/passwd",
    "orl://local/var/log/$(id)",
    "orl://local/var/log/`whoami`",
    "orl://local/var/log/|| ping -c 1 127.0.0.1",
    "orl://local/var/log/&& echo test",
  ];

  /// 特殊字符向量
  pub const SPECIAL_CHARS: &[&str] = &[
    "orl://local/var/log/\x00\x01\x02",
    "orl://local/var/log/\n\r\t",
    "orl://local/var/log/\u{0000}\u{0001}\u{0002}",
    "orl://local/var/log/\u{202e}evil\u{202c}", // RLO字符
  ];

  /// ORL注入尝试
  pub const ORL_INJECTION: &[&str] = &[
    "orl://local/var/log?entry=../../../etc/passwd",
    "orl://local@agent/var/log?entry=|ls",
    "orl://local/var/log?entry=%00/etc/passwd",
    "orl://local/var/log?entry=\x00\x01\x02",
  ];

  /// 超长ORL向量
  pub fn long_orls() -> Vec<String> {
    vec![
      format!("orl://local/{}", "a/".repeat(1000)),
      format!("orl://{}@agent/{}", "a".repeat(100), "b/".repeat(100)),
      format!("orl://{}@s3/{}", "a".repeat(50), "b/".repeat(200)),
    ]
  }

  /// 获取所有恶意ORL向量
  pub fn all_vectors() -> Vec<String> {
    let mut all = Vec::new();

    // 添加所有静态向量
    all.extend(PATH_TRAVERSAL.iter().map(|s| s.to_string()));
    all.extend(NULL_BYTE.iter().map(|s| s.to_string()));
    all.extend(COMMAND_INJECTION.iter().map(|s| s.to_string()));
    all.extend(SPECIAL_CHARS.iter().map(|s| s.to_string()));
    all.extend(ORL_INJECTION.iter().map(|s| s.to_string()));

    // 添加动态生成的向量
    all.extend(long_orls());

    all
  }
}

/// ORL安全分析器
pub struct OrlSecurityAnalyzer;

impl OrlSecurityAnalyzer {
  /// 检测ORL是否包含恶意模式
  pub fn analyze(orl: &str) -> OrlSecurityReport {
    let mut report = OrlSecurityReport::new(orl);

    // 检查路径遍历
    for pattern in malicious::PATH_TRAVERSAL {
      if orl.contains(pattern.trim_start_matches("orl://local")) {
        report.path_traversal_detected = true;
        break;
      }
    }

    // 检查空字节注入
    if orl.contains("%00") || orl.contains('\x00') {
      report.null_byte_detected = true;
    }

    // 检查命令注入模式
    let command_patterns = ["|", ";", "$(", "`", "||", "&&"];
    for pattern in command_patterns {
      if orl.contains(pattern) {
        report.command_injection_detected = true;
        break;
      }
    }

    // 检查特殊控制字符
    if orl
      .chars()
      .any(|c| c.is_control() && c != '\n' && c != '\r' && c != '\t')
    {
      report.control_chars_detected = true;
    }

    // 检查超长路径
    let path_part = orl.splitn(3, '/').nth(2).unwrap_or("");
    if path_part.len() > 500 {
      report.excessively_long = true;
    }

    report
  }

  /// 验证ORL是否安全
  pub fn is_safe(orl: &str) -> bool {
    let report = Self::analyze(orl);
    report.is_safe()
  }
}

/// ORL安全分析报告
#[derive(Debug, Clone)]
pub struct OrlSecurityReport {
  /// 原始ORL
  pub original_orl: String,
  /// 是否检测到路径遍历
  pub path_traversal_detected: bool,
  /// 是否检测到空字节注入
  pub null_byte_detected: bool,
  /// 是否检测到命令注入
  pub command_injection_detected: bool,
  /// 是否检测到控制字符
  pub control_chars_detected: bool,
  /// 是否超长
  pub excessively_long: bool,
}

impl OrlSecurityReport {
  /// 创建新的分析报告
  pub fn new(orl: &str) -> Self {
    Self {
      original_orl: orl.to_string(),
      path_traversal_detected: false,
      null_byte_detected: false,
      command_injection_detected: false,
      control_chars_detected: false,
      excessively_long: false,
    }
  }

  /// 检查ORL是否安全
  pub fn is_safe(&self) -> bool {
    !self.path_traversal_detected
      && !self.null_byte_detected
      && !self.command_injection_detected
      && !self.control_chars_detected
      && !self.excessively_long
  }

  /// 获取检测到的问题列表
  pub fn detected_issues(&self) -> Vec<&'static str> {
    let mut issues = Vec::new();

    if self.path_traversal_detected {
      issues.push("Path traversal detected");
    }
    if self.null_byte_detected {
      issues.push("Null byte injection detected");
    }
    if self.command_injection_detected {
      issues.push("Command injection detected");
    }
    if self.control_chars_detected {
      issues.push("Control characters detected");
    }
    if self.excessively_long {
      issues.push("Excessively long path detected");
    }

    issues
  }
}

/// ORL测试辅助函数
pub mod test_helpers {
  use super::*;

  /// 创建测试用的ORL集合
  pub fn create_test_orls() -> Vec<String> {
    vec![
      OrlGenerator::local("/var/log/nginx/access.log"),
      OrlGenerator::local("/var/log/syslog"),
      OrlGenerator::agent("web-01", "/app/logs/error.log", Some("192.168.1.100:4001")),
      OrlGenerator::agent("db-01", "/var/log/postgresql/postgresql.log", None),
      OrlGenerator::s3("production", "/bucket/logs/2024/01/app.log"),
      OrlGenerator::with_entry(
        &OrlGenerator::s3("prod", "/bucket/archive.tar.gz"),
        "internal/service.log",
      ),
    ]
  }

  /// 断言ORL可以被安全解析
  pub async fn assert_orl_parses_safely(orl: &str) -> Result<(), TestError> {
    // TODO: 实际实现ORL解析验证
    // 这里先检查基本的ORL格式
    if !orl.starts_with("orl://") {
      return Err(TestError::Other(format!("Invalid ORL format: {}", orl)));
    }

    // 检查是否包含明显的恶意模式
    let analyzer = OrlSecurityAnalyzer::analyze(orl);
    if !analyzer.is_safe() {
      return Err(TestError::Other(format!(
        "Unsafe ORL detected: {} - Issues: {:?}",
        orl,
        analyzer.detected_issues()
      )));
    }

    Ok(())
  }

  /// 运行恶意ORL测试套件
  pub async fn run_malicious_orl_test_suite<F>(test_func: F) -> Vec<String>
  where
    F: Fn(&str) -> bool,
  {
    let malicious_orls = malicious::all_vectors();
    let mut detected = Vec::new();

    for orl in malicious_orls {
      if test_func(&orl) {
        detected.push(orl);
      }
    }

    detected
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_orl_type_enum() {
    // 测试OrlType枚举
    let local = OrlType::Local;
    let agent = OrlType::Agent;
    let s3 = OrlType::S3;

    match local {
      OrlType::Local => assert!(true),
      _ => panic!("Expected Local"),
    }

    match agent {
      OrlType::Agent => assert!(true),
      _ => panic!("Expected Agent"),
    }

    match s3 {
      OrlType::S3 => assert!(true),
      _ => panic!("Expected S3"),
    }
  }

  #[test]
  fn test_orl_generator_local() {
    // 测试本地ORL生成
    let orl = OrlGenerator::local("/var/log/nginx/access.log");
    assert_eq!(orl, "orl://local/var/log/nginx/access.log");

    // 测试已包含orl://前缀的情况
    let orl_with_prefix = OrlGenerator::local("orl://local/var/log/syslog");
    assert_eq!(orl_with_prefix, "orl://local/var/log/syslog");
  }

  #[test]
  fn test_orl_generator_agent() {
    // 测试Agent ORL生成（不带服务器地址）
    let orl = OrlGenerator::agent("web-01", "/app/logs/error.log", None);
    assert_eq!(orl, "orl://web-01@agent/app/logs/error.log");

    // 测试Agent ORL生成（带服务器地址）
    let orl_with_addr = OrlGenerator::agent("db-01", "/var/log/postgresql.log", Some("192.168.1.100:4001"));
    assert_eq!(
      orl_with_addr,
      "orl://db-01@agent.192.168.1.100:4001/var/log/postgresql.log"
    );
  }

  #[test]
  fn test_orl_generator_s3() {
    // 测试S3 ORL生成
    let orl = OrlGenerator::s3("production", "/bucket/logs/2024/01/app.log");
    assert_eq!(orl, "orl://production@s3/bucket/logs/2024/01/app.log");
  }

  #[test]
  fn test_orl_generator_with_entry() {
    // 测试添加归档入口
    let base_orl = OrlGenerator::s3("prod", "/bucket/archive.tar.gz");
    let orl_with_entry = OrlGenerator::with_entry(&base_orl, "internal/service.log");
    assert_eq!(
      orl_with_entry,
      "orl://prod@s3/bucket/archive.tar.gz?entry=internal/service.log"
    );

    // 测试已包含查询参数的情况
    let orl_with_query = format!("{}?param=value", base_orl);
    let orl_with_both = OrlGenerator::with_entry(&orl_with_query, "internal/service.log");
    assert_eq!(
      orl_with_both,
      "orl://prod@s3/bucket/archive.tar.gz?param=value&entry=internal/service.log"
    );
  }

  #[test]
  fn test_orl_generator_with_query() {
    // 测试添加查询参数
    let base_orl = OrlGenerator::local("/var/log/nginx/access.log");

    // 添加单个参数
    let orl_with_one = OrlGenerator::with_query(&base_orl, &[("encoding", "UTF-8")]);
    assert_eq!(orl_with_one, "orl://local/var/log/nginx/access.log?encoding=UTF-8");

    // 添加多个参数
    let orl_with_multi = OrlGenerator::with_query(&base_orl, &[("encoding", "UTF-8"), ("limit", "100")]);
    assert_eq!(
      orl_with_multi,
      "orl://local/var/log/nginx/access.log?encoding=UTF-8&limit=100"
    );

    // 测试已包含查询参数的情况
    let orl_with_existing = format!("{}?existing=param", base_orl);
    let orl_with_additional = OrlGenerator::with_query(&orl_with_existing, &[("new", "value")]);
    assert_eq!(
      orl_with_additional,
      "orl://local/var/log/nginx/access.log?existing=param&new=value"
    );
  }

  #[test]
  fn test_malicious_vectors() {
    // 测试恶意ORL向量
    assert!(!malicious::PATH_TRAVERSAL.is_empty());
    assert!(!malicious::NULL_BYTE.is_empty());
    assert!(!malicious::COMMAND_INJECTION.is_empty());
    assert!(!malicious::SPECIAL_CHARS.is_empty());
    assert!(!malicious::ORL_INJECTION.is_empty());

    // 测试长ORL生成
    let long_orls = malicious::long_orls();
    assert_eq!(long_orls.len(), 3);

    // 测试所有向量收集
    let all_vectors = malicious::all_vectors();
    assert!(!all_vectors.is_empty());
  }

  #[test]
  fn test_orl_security_report() {
    // 测试安全报告创建
    let report = OrlSecurityReport::new("orl://local/var/log/nginx/access.log");

    assert_eq!(report.original_orl, "orl://local/var/log/nginx/access.log");
    assert!(!report.path_traversal_detected);
    assert!(!report.null_byte_detected);
    assert!(!report.command_injection_detected);
    assert!(!report.control_chars_detected);
    assert!(!report.excessively_long);
    assert!(report.is_safe());

    // 测试问题检测列表
    let issues = report.detected_issues();
    assert!(issues.is_empty());
  }

  #[test]
  fn test_orl_security_analyzer_safe() {
    // 测试安全ORL分析
    let safe_orl = "orl://local/var/log/nginx/access.log";
    let report = OrlSecurityAnalyzer::analyze(safe_orl);

    assert!(report.is_safe());
    assert!(OrlSecurityAnalyzer::is_safe(safe_orl));
  }

  #[test]
  fn test_orl_security_analyzer_path_traversal() {
    // 测试路径遍历检测
    let malicious_orl = "orl://local/../../../etc/passwd";
    let report = OrlSecurityAnalyzer::analyze(malicious_orl);

    assert!(report.path_traversal_detected);
    assert!(!report.is_safe());
    assert!(!OrlSecurityAnalyzer::is_safe(malicious_orl));

    let issues = report.detected_issues();
    assert!(issues.contains(&"Path traversal detected"));
  }

  #[test]
  fn test_orl_security_analyzer_null_byte() {
    // 测试空字节注入检测
    let malicious_orl = "orl://local/var/log/access.log%00";
    let report = OrlSecurityAnalyzer::analyze(malicious_orl);

    assert!(report.null_byte_detected);
    assert!(!report.is_safe());
  }

  #[test]
  fn test_orl_security_analyzer_command_injection() {
    // 测试命令注入检测
    let malicious_orl = "orl://local/var/log/| ls -la";
    let report = OrlSecurityAnalyzer::analyze(malicious_orl);

    assert!(report.command_injection_detected);
    assert!(!report.is_safe());
  }

  #[test]
  fn test_orl_security_analyzer_control_chars() {
    // 测试控制字符检测
    let malicious_orl = "orl://local/var/log/\x00\x01\x02";
    let report = OrlSecurityAnalyzer::analyze(malicious_orl);

    assert!(report.control_chars_detected);
    assert!(!report.is_safe());
  }

  #[test]
  fn test_orl_security_analyzer_long_path() {
    // 测试超长路径检测
    let long_path = format!("orl://local/{}", "a/".repeat(600)); // 超过500字符
    let report = OrlSecurityAnalyzer::analyze(&long_path);

    assert!(report.excessively_long);
    assert!(!report.is_safe());
  }

  #[test]
  fn test_test_helpers_create_test_orls() {
    // 测试测试ORL创建
    let test_orls = test_helpers::create_test_orls();

    assert!(!test_orls.is_empty());
    assert!(test_orls.iter().any(|orl| orl.contains("orl://local")));
    assert!(test_orls.iter().any(|orl| orl.contains("@agent")));
    assert!(test_orls.iter().any(|orl| orl.contains("@s3")));
  }

  #[tokio::test]
  async fn test_test_helpers_assert_orl_parses_safely() {
    // 测试ORL安全解析断言（安全情况）
    let safe_orl = "orl://local/var/log/nginx/access.log";
    let result = test_helpers::assert_orl_parses_safely(safe_orl).await;
    assert!(result.is_ok());

    // 测试ORL安全解析断言（不安全情况）
    let malicious_orl = "orl://local/../../../etc/passwd";
    let result = test_helpers::assert_orl_parses_safely(malicious_orl).await;
    assert!(result.is_err());
  }

  #[tokio::test]
  async fn test_test_helpers_run_malicious_orl_test_suite() {
    // 测试恶意ORL测试套件运行
    let detected = test_helpers::run_malicious_orl_test_suite(|orl| {
      // 简单的检测函数：检查是否包含恶意模式
      orl.contains("..") || orl.contains("%00") || orl.contains("|")
    })
    .await;

    // 应该检测到一些恶意ORL
    assert!(!detected.is_empty());
  }

  #[test]
  fn test_security_report_issues_format() {
    // 测试安全报告问题格式化
    let mut report = OrlSecurityReport::new("test");
    report.path_traversal_detected = true;
    report.null_byte_detected = true;
    report.command_injection_detected = true;

    let issues = report.detected_issues();
    assert_eq!(issues.len(), 3);
    assert!(issues.contains(&"Path traversal detected"));
    assert!(issues.contains(&"Null byte injection detected"));
    assert!(issues.contains(&"Command injection detected"));
  }
}
