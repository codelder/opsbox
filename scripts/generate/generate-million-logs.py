#!/usr/bin/env python3
# -*- coding: utf-8 -*-

import os
import random
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
    "磁盘空间不足，剩余: {free_space}GB",
    "交易处理完成，交易号: {transaction_id}",
    "网关转发请求到 {service_name}",
    "SQL执行时间: {sql_time}ms",
    "业务处理时间: {business_time}ms",
    "系统异常: {exception_msg}",
    "服务调用失败: {service_error}",
    "数据同步完成，同步记录数: {sync_count}",
    "定时任务执行: {task_name}",
    "配置更新: {config_key} = {config_value}",
    "日志轮转完成，归档文件: {archive_file}",
    "用户 {user_id} 查询数据，结果数量: {result_count}",
    "文件上传完成: {filename}，大小: {file_size}MB",
    "邮件发送成功: {email_address}",
    "短信验证码发送: {phone_number}",
    "权限验证通过: {user_id} 访问 {resource}",
    "数据备份完成: {backup_file}",
    "系统监控告警: {alert_type} - {alert_msg}",
    "负载均衡器状态: {lb_status}",
    "微服务 {service_name} 健康检查通过",
    "队列处理完成: {queue_name}，处理数量: {processed_count}"
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
    "Disk space low, remaining: {free_space}GB",
    "Transaction processed, transaction ID: {transaction_id}",
    "Gateway forwarding request to {service_name}",
    "SQL execution time: {sql_time}ms",
    "Business processing time: {business_time}ms",
    "System exception: {exception_msg}",
    "Service call failed: {service_error}",
    "Data sync completed, synced records: {sync_count}",
    "Scheduled task executed: {task_name}",
    "Configuration updated: {config_key} = {config_value}",
    "Log rotation completed, archived file: {archive_file}",
    "User {user_id} queried data, result count: {result_count}",
    "File upload completed: {filename}, size: {file_size}MB",
    "Email sent successfully: {email_address}",
    "SMS verification code sent: {phone_number}",
    "Permission verified: {user_id} accessing {resource}",
    "Data backup completed: {backup_file}",
    "System monitoring alert: {alert_type} - {alert_msg}",
    "Load balancer status: {lb_status}",
    "Microservice {service_name} health check passed",
    "Queue processing completed: {queue_name}, processed count: {processed_count}"
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
    "请求错误",
    "网络异常",
    "数据库锁定",
    "文件系统错误",
    "内存不足"
]

# 服务名称
SERVICE_NAMES = [
    "bjbcbp-bjetcs", "bjbcbp-bjbfdzd", "bjbcbp-bjeups", "bjbcbp-bjbfrhhz",
    "bjbcbp-bjmass", "bjbcbp-bjchps", "bjbcbp-bjegps", "bjbcbp-gateway",
    "bjbcbp-bjbfyff", "bjbcbp-bjhpfs", "bjbbip-agent", "bjbcbp-bjbfjzth",
    "bjbcbp-bjbcpm", "bjbcbp-bjcpos", "bjbbip-bbippubsvr", "bjbbip-gateway",
    "bjbcbp-bjbknd", "bjbcbp-bjbbts", "bjbcbp-bjcbcm", "bjbcbp-bjmasstb",
    "bjbbip-bbipsche", "bjbcbp-bjrcis", "bjbcbp-bjcars", "bjbcbp-bjbfrbs",
    "bjbcbp-bjsrsp", "bjbcbp-bjbfgjj", "bjbcbp-bjevca", "bjbcbp-ntpselc",
    "bjbcbp-bjlogs", "bjbcbp-bjntps", "bjbcbp-bjbetc", "bjbbip-bbipbtp",
    "bjbcbp-repeater", "bjbcbp-bjptcr", "bjbcbp-bjbbfebbos", "bjbcbp-bjkfqzw",
    "bjbcbp-bjegpsyth", "bjbbip-bbipgove", "bjbcbp-bjbfshb", "bjbcbp-bjsypt",
    "bjbcbp-bbip-forward", "bjbbip-bjagtsx", "bjbcbp-bjbmicp", "bjbcbp-bjagtsdf",
    "bjbcbp-bjbfcbs", "bjbcbp-bjbbfecore"
]

# 日志级别
LOG_LEVELS = ["INFO", "WARN", "ERROR", "DEBUG", "TRACE"]

def generate_user_id():
    return f"user_{random.randint(1000, 9999)}"

def generate_order_id():
    return f"ORD{random.randint(100000, 999999)}"

def generate_transaction_id():
    return f"TXN{random.randint(1000000, 9999999)}"

def generate_api_path():
    paths = ["/api/users", "/api/orders", "/api/payments", "/api/products", "/api/auth", 
             "/api/gateway", "/api/system", "/api/reports", "/api/config", "/api/monitor",
             "/api/backup", "/api/search", "/api/upload", "/api/download", "/api/export"]
    return random.choice(paths)

def generate_action():
    actions = ["login", "logout", "create", "update", "delete", "view", "search", 
               "登录", "登出", "创建", "更新", "删除", "查看", "搜索", "导出", "导入",
               "备份", "恢复", "配置", "监控", "告警", "统计", "分析"]
    return random.choice(actions)

def generate_email():
    domains = ["example.com", "test.com", "demo.org", "sample.net", "company.cn"]
    username = f"user{random.randint(100, 999)}"
    return f"{username}@{random.choice(domains)}"

def generate_phone():
    return f"1{random.randint(3000000000, 9999999999)}"

def generate_filename():
    extensions = [".log", ".txt", ".json", ".xml", ".csv", ".pdf", ".doc", ".xls"]
    name = f"file_{random.randint(1000, 9999)}"
    return f"{name}{random.choice(extensions)}"

def generate_log_entry():
    """生成一条随机日志条目"""
    # 生成时间戳，范围在最近30天内
    timestamp = datetime.datetime.now() - datetime.timedelta(
        days=random.randint(0, 30),
        hours=random.randint(0, 23),
        minutes=random.randint(0, 59),
        seconds=random.randint(0, 59)
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
        'transaction_id': generate_transaction_id(),
        'amount': random.randint(10, 9999),
        'error_code': random.randint(100, 599),
        'error_msg': random.choice(ERROR_MESSAGES),
        'retry_count': random.randint(1, 5),
        'hit_rate': random.randint(70, 99),
        'api_path': generate_api_path(),
        'response_time': random.randint(10, 2000),
        'sql_time': random.randint(5, 500),
        'business_time': random.randint(10, 1000),
        'action': generate_action(),
        'startup_time': random.randint(1, 30),
        'memory_usage': random.randint(20, 90),
        'free_space': random.randint(1, 100),
        'service_name': random.choice(SERVICE_NAMES),
        'exception_msg': random.choice(ERROR_MESSAGES),
        'service_error': random.choice(ERROR_MESSAGES),
        'sync_count': random.randint(100, 10000),
        'task_name': f"task_{random.randint(1, 100)}",
        'config_key': f"config_{random.randint(1, 50)}",
        'config_value': f"value_{random.randint(1, 100)}",
        'archive_file': f"archive_{random.randint(1, 1000)}.log",
        'result_count': random.randint(1, 1000),
        'filename': generate_filename(),
        'file_size': random.randint(1, 100),
        'email_address': generate_email(),
        'phone_number': generate_phone(),
        'resource': f"/resource/{random.randint(1, 100)}",
        'backup_file': f"backup_{random.randint(1, 1000)}.tar.gz",
        'alert_type': random.choice(["CPU", "MEMORY", "DISK", "NETWORK", "CPU", "内存", "磁盘", "网络"]),
        'alert_msg': random.choice(["High usage", "Low space", "Connection failed", "高使用率", "空间不足", "连接失败"]),
        'lb_status': random.choice(["UP", "DOWN", "DEGRADED", "正常", "异常", "降级"]),
        'queue_name': f"queue_{random.randint(1, 20)}",
        'processed_count': random.randint(1, 1000)
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
    if random.random() < 0.15:
        additional_content.append(f" Trace ID: {random.randint(100000000, 999999999)}")
    
    # 随机选择日志级别
    log_level = random.choice(LOG_LEVELS)
    
    full_log = f"{timestamp.strftime('%Y-%m-%d %H:%M:%S')} [{log_level}] {log_message}"
    if additional_content:
        full_log += "".join(additional_content)
    
    return full_log

def generate_million_logs(output_file):
    """生成10万行日志文件"""
    print(f"开始生成10万行日志文件: {output_file}")
    print("这可能需要几秒钟时间，请耐心等待...")
    
    start_time = datetime.datetime.now()
    
    with open(output_file, 'w', encoding='utf-8') as f:
        for i in range(100000):
            if i % 10000 == 0:  # 每1万行显示一次进度
                progress = (i / 100000) * 100
                elapsed = datetime.datetime.now() - start_time
                print(f"进度: {progress:.1f}% ({i:,}/100,000) - 已用时: {elapsed}")
            
            log_entry = generate_log_entry()
            f.write(log_entry + '\n')
    
    end_time = datetime.datetime.now()
    total_time = end_time - start_time
    
    # 获取文件大小
    file_size = os.path.getsize(output_file)
    file_size_mb = file_size / (1024 * 1024)
    
    print(f"\n生成完成！")
    print(f"文件: {output_file}")
    print(f"行数: 100,000")
    print(f"文件大小: {file_size_mb:.2f} MB")
    print(f"总耗时: {total_time}")
    print(f"平均速度: {100000 / total_time.total_seconds():.0f} 行/秒")

def main():
    print("=" * 60)
    print("10万行日志文件生成器")
    print("=" * 60)
    
    # 生成文件名
    timestamp = datetime.datetime.now().strftime("%Y%m%d_%H%M%S")
    output_file = f"hundred_thousand_logs_{timestamp}.log"
    
    try:
        generate_million_logs(output_file)
        print(f"\n✅ 成功生成文件: {output_file}")
    except KeyboardInterrupt:
        print("\n❌ 用户中断了生成过程")
        if os.path.exists(output_file):
            os.remove(output_file)
            print(f"已清理未完成的文件: {output_file}")
    except Exception as e:
        print(f"\n❌ 生成过程中出现错误: {e}")
        if os.path.exists(output_file):
            os.remove(output_file)
            print(f"已清理未完成的文件: {output_file}")

if __name__ == "__main__":
    main()
