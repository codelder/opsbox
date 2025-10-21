#!/usr/bin/env python3
# -*- coding: utf-8 -*-

import os
import random
import tarfile
import datetime
from pathlib import Path

# 中文日志模板
CHINESE_LOG_TEMPLATES = [
    "用户 {user_id} 登录系统",
    "订单 {order_id} 创建成功，金额: ¥{amount}",
    "支付失败: {error_code} - {error_msg}",
    "数据库连接超时，重试次数: {retry_count}",
    "缓存命中率: {hit_rate}%",
    "API请求 {api_path} 响应时间: {response_time}ms",
    "用户 {user_id} 执行操作: {action}",
    "系统启动完成，耗时: {startup_time}s",
    "内存使用率: {memory_usage}%",
    "磁盘空间不足，剩余: {free_space}GB"
]

# 英文日志模板
ENGLISH_LOG_TEMPLATES = [
    "User {user_id} logged in successfully",
    "Order {order_id} created with amount: ${amount}",
    "Payment failed: {error_code} - {error_msg}",
    "Database connection timeout, retry count: {retry_count}",
    "Cache hit rate: {hit_rate}%",
    "API request {api_path} response time: {response_time}ms",
    "User {user_id} performed action: {action}",
    "System startup completed in {startup_time}s",
    "Memory usage: {memory_usage}%",
    "Disk space low, remaining: {free_space}GB"
]

# 错误消息模板
ERROR_MESSAGES = [
    "Connection refused",
    "Timeout occurred",
    "Invalid credentials",
    "Resource not found",
    "Permission denied",
    "Internal server error",
    "Service unavailable",
    "Bad request",
    "连接被拒绝",
    "超时发生",
    "凭据无效",
    "资源未找到",
    "权限被拒绝",
    "内部服务器错误",
    "服务不可用",
    "请求错误"
]

# 用户ID和订单ID生成
def generate_user_id():
    return f"user_{random.randint(1000, 9999)}"

def generate_order_id():
    return f"ORD{random.randint(100000, 999999)}"

def generate_api_path():
    paths = ["/api/users", "/api/orders", "/api/payments", "/api/products", "/api/auth"]
    return random.choice(paths)

def generate_action():
    actions = ["login", "logout", "create", "update", "delete", "view", "search", "登录", "登出", "创建", "更新", "删除", "查看", "搜索"]
    return random.choice(actions)

def generate_log_entry():
    """生成一条随机日志条目"""
    timestamp = datetime.datetime.now() - datetime.timedelta(
        days=random.randint(0, 30),
        hours=random.randint(0, 23),
        minutes=random.randint(0, 59)
    )
    
    # 随机选择中文或英文模板
    if random.random() < 0.6:  # 60%概率使用中文
        template = random.choice(CHINESE_LOG_TEMPLATES)
    else:
        template = random.choice(ENGLISH_LOG_TEMPLATES)
    
    # 填充模板变量
    log_data = {
        'user_id': generate_user_id(),
        'order_id': generate_order_id(),
        'amount': random.randint(10, 9999),
        'error_code': random.randint(100, 599),
        'error_msg': random.choice(ERROR_MESSAGES),
        'retry_count': random.randint(1, 5),
        'hit_rate': random.randint(70, 99),
        'api_path': generate_api_path(),
        'response_time': random.randint(10, 2000),
        'action': generate_action(),
        'startup_time': random.randint(1, 30),
        'memory_usage': random.randint(20, 90),
        'free_space': random.randint(1, 100)
    }
    
    log_message = template.format(**log_data)
    
    # 添加一些随机的中文和英文内容
    additional_content = []
    if random.random() < 0.3:
        additional_content.append(f" 详细信息: 处理时间 {random.randint(1, 100)}ms")
    if random.random() < 0.2:
        additional_content.append(f" Additional info: Processing time {random.randint(1, 100)}ms")
    if random.random() < 0.1:
        additional_content.append(f" 调试信息: 内存使用 {random.randint(100, 1000)}MB")
    
    full_log = f"{timestamp.strftime('%Y-%m-%d %H:%M:%S')} [INFO] {log_message}"
    if additional_content:
        full_log += "".join(additional_content)
    
    return full_log

def generate_log_file(file_path, line_count):
    """生成一个日志文件"""
    with open(file_path, 'w', encoding='utf-8') as f:
        for _ in range(line_count):
            f.write(generate_log_entry() + '\n')

def create_directory_structure():
    """创建目录结构"""
    base_dir = Path("test_logs")
    base_dir.mkdir(exist_ok=True)
    
    # 定义目录结构
    directories = [
        "logs/2025/01/application",
        "logs/2025/01/system",
        "logs/2025/01/error",
        "logs/2025/02/application",
        "logs/2025/02/system",
        "logs/2025/02/error",
        "logs/2025/03/application",
        "logs/2025/03/system",
        "logs/2025/03/error",
        "data/backup/2025-01",
        "data/backup/2025-02",
        "data/backup/2025-03",
        "temp/cache/redis",
        "temp/cache/memcached",
        "temp/sessions",
        "config/production",
        "config/development",
        "config/testing",
        "reports/daily/2025-01",
        "reports/daily/2025-02",
        "reports/weekly/2025-01",
        "reports/weekly/2025-02",
        "reports/monthly/2025-01",
        "reports/monthly/2025-02"
    ]
    
    # 创建所有目录
    for dir_path in directories:
        (base_dir / dir_path).mkdir(parents=True, exist_ok=True)
    
    return base_dir

def generate_files(base_dir):
    """在目录中生成日志文件"""
    file_configs = [
        # (相对路径, 文件名, 行数)
        ("logs/2025/01/application", "app.log", 150),
        ("logs/2025/01/application", "access.log", 200),
        ("logs/2025/01/system", "system.log", 100),
        ("logs/2025/01/system", "kernel.log", 80),
        ("logs/2025/01/error", "error.log", 50),
        ("logs/2025/02/application", "app.log", 180),
        ("logs/2025/02/application", "access.log", 220),
        ("logs/2025/02/system", "system.log", 120),
        ("logs/2025/02/system", "kernel.log", 90),
        ("logs/2025/02/error", "error.log", 60),
        ("logs/2025/03/application", "app.log", 160),
        ("logs/2025/03/application", "access.log", 190),
        ("logs/2025/03/system", "system.log", 110),
        ("logs/2025/03/system", "kernel.log", 85),
        ("logs/2025/03/error", "error.log", 45),
        ("data/backup/2025-01", "backup_20250101.log", 300),
        ("data/backup/2025-01", "backup_20250115.log", 280),
        ("data/backup/2025-02", "backup_20250201.log", 320),
        ("data/backup/2025-02", "backup_20250215.log", 290),
        ("data/backup/2025-03", "backup_20250301.log", 310),
        ("temp/cache/redis", "redis.log", 75),
        ("temp/cache/memcached", "memcached.log", 65),
        ("temp/sessions", "session.log", 40),
        ("config/production", "config.log", 25),
        ("config/development", "config.log", 30),
        ("config/testing", "config.log", 20),
        ("reports/daily/2025-01", "daily_report.log", 100),
        ("reports/daily/2025-02", "daily_report.log", 120),
        ("reports/weekly/2025-01", "weekly_report.log", 80),
        ("reports/weekly/2025-02", "weekly_report.log", 90),
        ("reports/monthly/2025-01", "monthly_report.log", 60),
        ("reports/monthly/2025-02", "monthly_report.log", 70),
    ]
    
    for dir_path, filename, line_count in file_configs:
        file_path = base_dir / dir_path / filename
        print(f"生成文件: {file_path} ({line_count} 行)")
        generate_log_file(file_path, line_count)

def create_tar_gz(base_dir, output_file):
    """创建tar.gz文件"""
    print(f"创建tar.gz文件: {output_file}")
    with tarfile.open(output_file, "w:gz") as tar:
        tar.add(base_dir, arcname="test_logs")

def main():
    print("开始生成测试日志文件...")
    
    # 创建目录结构
    base_dir = create_directory_structure()
    print(f"创建目录结构: {base_dir}")
    
    # 生成文件
    generate_files(base_dir)
    
    # 创建tar.gz文件
    output_file = "test_logs.tar.gz"
    create_tar_gz(base_dir, output_file)
    
    # 清理临时目录
    import shutil
    shutil.rmtree(base_dir)
    
    print(f"完成！生成的文件: {output_file}")
    print(f"文件大小: {os.path.getsize(output_file) / 1024:.1f} KB")

if __name__ == "__main__":
    main()
