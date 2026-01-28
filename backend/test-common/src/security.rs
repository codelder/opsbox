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
        "' OR ",
        "' AND ",
        " UNION ",
        " SELECT ",
        " INSERT ",
        " UPDATE ",
        " DELETE ",
        " DROP ",
        "--",
        "/*",
        "*/",
        "#",
        ";",
    ];

    let input_upper = input.to_uppercase();
    patterns.iter().any(|&pattern| input_upper.contains(&pattern.to_uppercase()))
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
        "....",  // 替代表示
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
        self.command_injection_detected = self.input.contains(';') ||
                                         self.input.contains('|') ||
                                         self.input.contains('&') ||
                                         self.input.contains('`') ||
                                         self.input.contains('$');

        self
    }

    /// 检查是否通过安全测试
    pub fn passed(&self) -> bool {
        !self.sql_injection_detected &&
        !self.path_traversal_detected &&
        !self.xss_detected &&
        !self.command_injection_detected
    }
}