//! 安全测试工具
//!
//! 提供安全测试相关的工具函数和测试数据

/// SQL注入测试向量
pub mod sql_injection {
  /// 常见的SQL注入测试向量
  pub const TEST_VECTORS: &[&str] = &[
    "' OR '1'='1",
    "' OR '1'='1' --",
    "' OR '1'='1' /*",
    "admin' --",
    "admin' #",
    "' UNION SELECT null, username, password FROM users --",
    "' AND 1=CONVERT(int, (SELECT CURRENT_USER)) --",
    "'; DROP TABLE users; --",
    "' OR EXISTS(SELECT * FROM users) AND '1'='1",
    "' OR (SELECT COUNT(*) FROM users) > 0 --",
    "' OR (SELECT ASCII(SUBSTRING(username,1,1)) FROM users WHERE id=1) > 0 --",
  ];

  /// SQL注入测试向量（带时间盲注）
  pub const TIME_BASED_VECTORS: &[&str] = &[
    "' OR SLEEP(5) --",
    "' OR (SELECT * FROM (SELECT(SLEEP(5)))a) --",
    "' OR pg_sleep(5) --",
    "' OR WAITFOR DELAY '00:00:05' --",
    "' OR BENCHMARK(10000000,MD5('test')) --",
  ];
}

/// 路径遍历测试向量
pub mod path_traversal {
  /// 常见的路径遍历测试向量
  pub const TEST_VECTORS: &[&str] = &[
    "../../../etc/passwd",
    "..\\..\\..\\windows\\system32\\drivers\\etc\\hosts",
    "%2e%2e%2f%2e%2e%2f%2e%2e%2fetc%2fpasswd",
    "..%252f..%252f..%252fetc%252fpasswd",
    "....//....//....//etc/passwd",
    "..;/../..;/../..;/etc/passwd",
    "/etc/passwd",
    "C:\\Windows\\System32\\drivers\\etc\\hosts",
    "\\\\.\\PhysicalDrive0",
    "file:///etc/passwd",
  ];

  /// 空字节注入测试向量
  pub const NULL_BYTE_VECTORS: &[&str] = &[
    "../../../etc/passwd%00",
    "..\\..\\..\\windows\\system32\\drivers\\etc\\hosts%00",
    "file.pdf%00.jpg",
    "test.php%00.txt",
  ];
}

/// XSS测试向量
pub mod xss {
  /// 常见的XSS测试向量
  pub const TEST_VECTORS: &[&str] = &[
    "<script>alert('XSS')</script>",
    "<img src=x onerror=alert('XSS')>",
    "<svg onload=alert('XSS')>",
    "\" onmouseover=\"alert('XSS')\"",
    "javascript:alert('XSS')",
    "data:text/html;base64,PHNjcmlwdD5hbGVydCgnWFNTJyk8L3NjcmlwdD4=",
    "<iframe src=\"javascript:alert('XSS')\">",
    "<body onload=alert('XSS')>",
    "<a href=\"javascript:alert('XSS')\">Click</a>",
    "<script>fetch('/admin/delete-all')</script>",
  ];

  /// 编码后的XSS测试向量
  pub const ENCODED_VECTORS: &[&str] = &[
    "%3Cscript%3Ealert('XSS')%3C%2Fscript%3E",
    "&lt;script&gt;alert('XSS')&lt;/script&gt;",
    "%22%20onmouseover%3D%22alert%28%27XSS%27%29%22",
  ];
}

/// 命令注入测试向量
pub mod command_injection {
  /// 常见的命令注入测试向量
  pub const TEST_VECTORS: &[&str] = &[
    "; ls -la",
    "| ls -la",
    "&& ls -la",
    "|| ls -la",
    "`ls -la`",
    "$(ls -la)",
    "'; ls -la; '",
    "\"; ls -la; \"",
    "| cat /etc/passwd",
    "&& cat /etc/passwd",
    "|| cat /etc/passwd",
    "; cat /etc/passwd",
  ];
}

/// 验证输入是否包含SQL注入模式
pub fn contains_sql_injection(input: &str) -> bool {
  let patterns = [
    "' OR ", "' AND ", " UNION ", " SELECT ", " INSERT ", " UPDATE ", " DELETE ", " DROP ", "--", "/*", "*/", "#", ";",
  ];

  let input_upper = input.to_uppercase();
  patterns
    .iter()
    .any(|&pattern| input_upper.contains(&pattern.to_uppercase()))
}

/// 验证输入是否包含路径遍历模式
pub fn contains_path_traversal(input: &str) -> bool {
  let patterns = [
    "..",
    "../",
    "..\\",
    "/etc/passwd",
    "/etc/shadow",
    "C:\\Windows",
    "\\\\",
    // URL编码的路径遍历
    "%2e%2e",
    "%2e%2e%2f",
    "%2e%2e%5c",
    // 双重编码
    "%252e%252e",
    "....", // 替代表示
  ];

  patterns.iter().any(|&pattern| input.contains(pattern))
}

/// 验证输入是否包含XSS模式
pub fn contains_xss(input: &str) -> bool {
  let patterns = [
    "<script",
    "</script",
    "javascript:",
    "onerror=",
    "onload=",
    "onmouseover=",
    "alert(",
    "eval(",
    "document.cookie",
    "data:text/html",
    "data:image/svg+xml",
    "vbscript:",
    "expression(",
  ];

  let input_lower = input.to_lowercase();
  patterns.iter().any(|&pattern| input_lower.contains(pattern))
}

/// 生成安全测试报告
#[derive(Debug, Clone)]
pub struct SecurityTestReport {
  /// 测试的输入
  pub input: String,
  /// 是否检测到SQL注入
  pub sql_injection_detected: bool,
  /// 是否检测到路径遍历
  pub path_traversal_detected: bool,
  /// 是否检测到XSS
  pub xss_detected: bool,
  /// 是否检测到命令注入
  pub command_injection_detected: bool,
}

impl SecurityTestReport {
  /// 创建新的安全测试报告
  pub fn new(input: String) -> Self {
    Self {
      input,
      sql_injection_detected: false,
      path_traversal_detected: false,
      xss_detected: false,
      command_injection_detected: false,
    }
  }

  /// 分析输入并生成报告
  pub fn analyze(mut self) -> Self {
    self.sql_injection_detected = contains_sql_injection(&self.input);
    self.path_traversal_detected = contains_path_traversal(&self.input);
    self.xss_detected = contains_xss(&self.input);
    self.command_injection_detected = self.input.contains(';')
      || self.input.contains('|')
      || self.input.contains('&')
      || self.input.contains('`')
      || self.input.contains('$');

    self
  }

  /// 检查是否通过安全测试
  pub fn passed(&self) -> bool {
    !self.sql_injection_detected
      && !self.path_traversal_detected
      && !self.xss_detected
      && !self.command_injection_detected
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_sql_injection_test_vectors() {
    // 测试SQL注入向量存在
    assert!(!sql_injection::TEST_VECTORS.is_empty());
    assert!(!sql_injection::TIME_BASED_VECTORS.is_empty());

    // 检查一些已知向量
    assert!(sql_injection::TEST_VECTORS.iter().any(|v| v.contains("OR '1'='1")));
    assert!(sql_injection::TIME_BASED_VECTORS.iter().any(|v| v.contains("SLEEP")));
  }

  #[test]
  fn test_path_traversal_test_vectors() {
    // 测试路径遍历向量存在
    assert!(!path_traversal::TEST_VECTORS.is_empty());
    assert!(!path_traversal::NULL_BYTE_VECTORS.is_empty());

    // 检查一些已知向量
    assert!(path_traversal::TEST_VECTORS.iter().any(|v| v.contains("../")));
    assert!(path_traversal::NULL_BYTE_VECTORS.iter().any(|v| v.contains("%00")));
  }

  #[test]
  fn test_xss_test_vectors() {
    // 测试XSS向量存在
    assert!(!xss::TEST_VECTORS.is_empty());
    assert!(!xss::ENCODED_VECTORS.is_empty());

    // 检查一些已知向量
    assert!(xss::TEST_VECTORS.iter().any(|v| v.contains("<script>")));
    assert!(xss::ENCODED_VECTORS.iter().any(|v| v.contains("%3Cscript")));
  }

  #[test]
  fn test_command_injection_test_vectors() {
    // 测试命令注入向量存在
    assert!(!command_injection::TEST_VECTORS.is_empty());

    // 检查一些已知向量
    assert!(command_injection::TEST_VECTORS.iter().any(|v| v.contains("; ls")));
    assert!(command_injection::TEST_VECTORS.iter().any(|v| v.contains("| cat")));
  }

  #[test]
  fn test_contains_sql_injection() {
    // 测试SQL注入检测
    assert!(contains_sql_injection("' OR '1'='1"));
    assert!(contains_sql_injection("admin' --"));
    assert!(contains_sql_injection("' UNION SELECT"));
    assert!(contains_sql_injection("' AND 1=1"));

    // 测试安全输入
    assert!(!contains_sql_injection("normal input"));
    assert!(!contains_sql_injection("SELECT * FROM users")); // 注意：这个会检测到SELECT
    assert!(!contains_sql_injection("'test'")); // 单引号但不是SQL注入模式
  }

  #[test]
  fn test_contains_sql_injection_case_insensitive() {
    // 测试大小写不敏感
    assert!(contains_sql_injection("' or '1'='1")); // 小写or
    assert!(contains_sql_injection("' uNiOn SeLeCt")); // 混合大小写
    assert!(contains_sql_injection("' aNd 1=1")); // 小写and
  }

  #[test]
  fn test_contains_path_traversal() {
    // 测试路径遍历检测
    assert!(contains_path_traversal("../../../etc/passwd"));
    assert!(contains_path_traversal("..\\..\\..\\windows"));
    assert!(contains_path_traversal("%2e%2e%2fetc"));
    assert!(contains_path_traversal("/etc/passwd"));
    assert!(contains_path_traversal("C:\\Windows"));

    // 测试安全输入
    assert!(!contains_path_traversal("normal/path/file.txt"));
    assert!(contains_path_traversal(".../test")); // 三个点包含两个点，被检测为路径遍历
    assert!(contains_path_traversal("..test")); // 包含..，被检测为路径遍历
  }

  #[test]
  fn test_contains_xss() {
    // 测试XSS检测
    assert!(contains_xss("<script>alert('XSS')</script>"));
    assert!(contains_xss("<img src=x onerror=alert('XSS')>"));
    assert!(contains_xss("javascript:alert('XSS')"));
    assert!(contains_xss("onload=alert('XSS')"));
    assert!(contains_xss("data:text/html"));

    // 测试安全输入
    assert!(!contains_xss("normal text"));
    assert!(!contains_xss("<div>safe html</div>"));
    assert!(!contains_xss("javascript is a language")); // 不是"javascript:"模式
  }

  #[test]
  fn test_contains_xss_case_insensitive() {
    // 测试大小写不敏感
    assert!(contains_xss("<SCRIPT>alert('XSS')</SCRIPT>"));
    assert!(contains_xss("JAVASCRIPT:alert('XSS')"));
    assert!(contains_xss("ONLOAD=alert('XSS')"));
  }

  #[test]
  fn test_security_test_report_new() {
    // 测试SecurityTestReport创建
    let report = SecurityTestReport::new("test input".to_string());

    assert_eq!(report.input, "test input");
    assert!(!report.sql_injection_detected);
    assert!(!report.path_traversal_detected);
    assert!(!report.xss_detected);
    assert!(!report.command_injection_detected);
  }

  #[test]
  fn test_security_test_report_analyze_safe() {
    // 测试分析安全输入
    let report = SecurityTestReport::new("safe input".to_string()).analyze();

    assert!(report.passed());
    assert!(!report.sql_injection_detected);
    assert!(!report.path_traversal_detected);
    assert!(!report.xss_detected);
    assert!(!report.command_injection_detected);
  }

  #[test]
  fn test_security_test_report_analyze_sql_injection() {
    // 测试分析SQL注入
    let report = SecurityTestReport::new("' OR '1'='1".to_string()).analyze();

    assert!(!report.passed());
    assert!(report.sql_injection_detected);
    assert!(!report.path_traversal_detected);
    assert!(!report.xss_detected);
    assert!(!report.command_injection_detected);
  }

  #[test]
  fn test_security_test_report_analyze_path_traversal() {
    // 测试分析路径遍历
    let report = SecurityTestReport::new("../../../etc/passwd".to_string()).analyze();

    assert!(!report.passed());
    assert!(!report.sql_injection_detected);
    assert!(report.path_traversal_detected);
    assert!(!report.xss_detected);
    assert!(!report.command_injection_detected);
  }

  #[test]
  fn test_security_test_report_analyze_xss() {
    // 测试分析XSS
    let report = SecurityTestReport::new("<script>alert('XSS')</script>".to_string()).analyze();

    assert!(!report.passed());
    assert!(!report.sql_injection_detected);
    assert!(!report.path_traversal_detected);
    assert!(report.xss_detected);
    assert!(!report.command_injection_detected);
  }

  #[test]
  fn test_security_test_report_analyze_command_injection() {
    // 测试分析命令注入
    let report = SecurityTestReport::new("; ls -la".to_string()).analyze();

    assert!(!report.passed());
    assert!(report.sql_injection_detected); // 分号也被检测为SQL注入
    assert!(!report.path_traversal_detected);
    assert!(!report.xss_detected);
    assert!(report.command_injection_detected);
  }

  #[test]
  fn test_security_test_report_analyze_multiple_threats() {
    // 测试分析多个威胁
    let malicious_input = "' OR '1'='1; cat ../../../etc/passwd | grep root <script>alert(1)</script>";
    let report = SecurityTestReport::new(malicious_input.to_string()).analyze();

    assert!(!report.passed());
    // 这个输入应该触发所有检测
    assert!(report.sql_injection_detected);
    assert!(report.path_traversal_detected);
    assert!(report.xss_detected);
    assert!(report.command_injection_detected);
  }

  #[test]
  fn test_security_test_report_clone() {
    // 测试Clone实现
    let report = SecurityTestReport::new("test".to_string()).analyze();
    let cloned = report.clone();

    assert_eq!(cloned.input, report.input);
    assert_eq!(cloned.sql_injection_detected, report.sql_injection_detected);
    assert_eq!(cloned.path_traversal_detected, report.path_traversal_detected);
    assert_eq!(cloned.xss_detected, report.xss_detected);
    assert_eq!(cloned.command_injection_detected, report.command_injection_detected);
  }

  #[test]
  fn test_security_test_report_debug() {
    // 测试Debug实现
    let report = SecurityTestReport::new("debug test".to_string());
    let debug_output = format!("{:?}", report);

    assert!(debug_output.contains("debug test"));
    assert!(debug_output.contains("sql_injection_detected"));
    assert!(debug_output.contains("path_traversal_detected"));
  }

  #[test]
  fn test_command_injection_detection_logic() {
    // 测试命令注入检测逻辑
    let test_cases = vec![
      ("; ls", true),
      ("| cat", true),
      ("&& rm", true),
      ("|| whoami", true),
      ("`id`", true),
      ("$(ls)", true),
      ("normal text", false),
      ("test;", true), // 分号被视为命令注入
      ("a|b", true),   // 管道字符被视为命令注入
    ];

    for (input, expected) in test_cases {
      let report = SecurityTestReport::new(input.to_string()).analyze();
      assert_eq!(
        report.command_injection_detected, expected,
        "Failed for input: {}",
        input
      );
    }
  }

  #[test]
  fn test_edge_case_detections() {
    // 测试边界情况
    // 空输入
    let empty_report = SecurityTestReport::new("".to_string()).analyze();
    assert!(empty_report.passed());

    // 非常长的输入
    let long_input = "x".repeat(10000);
    let long_report = SecurityTestReport::new(long_input).analyze();
    assert!(long_report.passed()); // 应该通过，因为没有恶意模式

    // 混合编码
    let mixed_input = "%2e%2e%2fetc%2fpasswd <script>alert(1)</script>";
    let mixed_report = SecurityTestReport::new(mixed_input.to_string()).analyze();
    assert!(!mixed_report.passed());
    assert!(mixed_report.path_traversal_detected);
    assert!(mixed_report.xss_detected);
  }
}
