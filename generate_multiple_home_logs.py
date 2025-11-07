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
    "日志轮转完成，归档文件: {archive_file}"
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
    "Log rotation completed, archived file: {archive_file}"
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

def generate_user_id():
    return f"user_{random.randint(1000, 9999)}"

def generate_order_id():
    return f"ORD{random.randint(100000, 999999)}"

def generate_transaction_id():
    return f"TXN{random.randint(1000000, 9999999)}"

def generate_api_path():
    paths = ["/api/users", "/api/orders", "/api/payments", "/api/products", "/api/auth", "/api/gateway", "/api/system"]
    return random.choice(paths)

def generate_action():
    actions = ["login", "logout", "create", "update", "delete", "view", "search", "登录", "登出", "创建", "更新", "删除", "查看", "搜索"]
    return random.choice(actions)

def generate_trace_filename(date_str):
    """生成trace目录下的数字文件名，包含日期"""
    # 格式: HHMMSS04{YYYYMMDD}XXXXXXX.log
    hour = random.randint(0, 23)
    minute = random.randint(0, 59)
    second = random.randint(0, 59)
    random_suffix = random.randint(1000000, 9999999)
    
    # 将日期格式从 2025-08-19 转换为 20250819
    date_numeric = date_str.replace('-', '')
    
    return f"{hour:02d}{minute:02d}{second:02d}04{date_numeric}{random_suffix}.log"

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
        'archive_file': f"archive_{random.randint(1, 1000)}.log"
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

def create_directory_structure(base_dir, date_str):
    """创建与原始目录完全相同的结构，但使用指定的日期"""
    # 创建基础目录
    base_dir.mkdir(exist_ok=True)
    
    # 创建bbipadm目录
    bbipadm_dir = base_dir / "bbipadm"
    bbipadm_dir.mkdir(exist_ok=True)
    
    # 创建logs目录
    logs_dir = bbipadm_dir / "logs"
    logs_dir.mkdir(exist_ok=True)
    
    # 创建所有服务目录
    for service in SERVICE_NAMES:
        service_dir = logs_dir / service
        service_dir.mkdir(exist_ok=True)
        
        # 为大部分服务创建trace目录
        if service not in ["bjbcbp-bjbbfecsbp", "bjbcbp-bjbfrhhz", "bjbcbp-gateway", "bjbbip-agent", 
                          "bjbcbp-bjbcpm", "bjbbip-bbipsche", "bjbcbp-bjlogs", "msk", 
                          "bjbcbp-bjntps", "bjbcbp-bjbetc", "bjbbip-bbipbtp", "bjbcbp-repeater",
                          "bjbcbp-bjbbfebbos", "bjbcbp-bjkfqzw", "bjbcbp-bjegpsyth", "bjbbip-bbipgove",
                          "bjbcbp-bjbfshb", "bjbcbp-bjsypt", "bjbcbp-bbip-forward", "bjbbip-bjagtsx",
                          "bjbcbp-bjbmicp", "bjbcbp-bjagtsdf", "bjbcbp-bjbfcbs", "rocketmqlogs",
                          "bjbcbp-bjbbfecore"]:
            trace_dir = service_dir / "trace"
            trace_dir.mkdir(exist_ok=True)
            
            # 创建指定日期的目录
            date_dir = trace_dir / date_str
            date_dir.mkdir(exist_ok=True)
        
        # 为大部分服务创建指定日期的目录
        if service not in ["bjbcbp-bjbbfecsbp", "bjbcbp-bjbfrhhz", "bjbcbp-bjlogs", "msk", 
                          "bjbcbp-bjntps", "bjbcbp-bjbetc", "rocketmqlogs", "bjbcbp-bjbbfecore"]:
            date_dir = service_dir / date_str
            date_dir.mkdir(exist_ok=True)
    
    return base_dir

def generate_files(base_dir, date_str):
    """在目录中生成日志文件"""
    logs_dir = base_dir / "bbipadm" / "logs"
    
    # 为每个服务生成文件
    for service in SERVICE_NAMES:
        service_dir = logs_dir / service
        
        # 生成system.log文件（大部分服务都有）
        if service not in ["bjbcbp-bjbbfecsbp", "bjbcbp-bjbfrhhz", "bjbcbp-gateway", "bjbbip-agent", 
                          "bjbcbp-bjbcpm", "bjbbip-bbipsche", "bjbcbp-bjlogs", "msk", 
                          "bjbcbp-bjntps", "bjbcbp-bjbetc", "bjbbip-bbipbtp", "bjbcbp-repeater",
                          "bjbcbp-bjbbfebbos", "bjbcbp-bjkfqzw", "bjbcbp-bjegpsyth", "bjbbip-bbipgove",
                          "bjbcbp-bjbfshb", "bjbcbp-bjsypt", "bjbcbp-bbip-forward", "bjbbip-bjagtsx",
                          "bjbcbp-bjbmicp", "bjbcbp-bjagtsdf", "bjbcbp-bjbfcbs", "rocketmqlogs",
                          "bjbcbp-bjbbfecore"]:
            system_log = service_dir / "system.log"
            print(f"生成文件: {system_log} (200 行)")
            generate_log_file(system_log, 200)
        
        # 为gateway生成特殊文件
        if service == "bjbcbp-gateway":
            app_tran_time_log = service_dir / "app_tranTime.log"
            app_sql_time_log = service_dir / "app_sqlTime.log"
            print(f"生成文件: {app_tran_time_log} (150 行)")
            print(f"生成文件: {app_sql_time_log} (120 行)")
            generate_log_file(app_tran_time_log, 150)
            generate_log_file(app_sql_time_log, 120)
        
        # 为有trace目录的服务生成trace文件
        trace_dir = service_dir / "trace" / date_str
        if trace_dir.exists():
            # 生成5-15个trace文件
            trace_count = random.randint(5, 15)
            for i in range(trace_count):
                trace_filename = generate_trace_filename(date_str)
                trace_file = trace_dir / trace_filename
                print(f"生成文件: {trace_file} (50-200 行)")
                line_count = random.randint(50, 200)
                generate_log_file(trace_file, line_count)
        
        # 为有日期目录的服务生成文件
        date_dir = service_dir / date_str
        if date_dir.exists():
            # 生成1-3个文件
            file_count = random.randint(1, 3)
            for i in range(file_count):
                filename = f"{random.randint(1000000, 9999999)}.log"
                date_file = date_dir / filename
                print(f"生成文件: {date_file} (100-300 行)")
                line_count = random.randint(100, 300)
                generate_log_file(date_file, line_count)

def create_tar_gz(base_dir, output_file):
    """创建tar.gz文件"""
    print(f"创建tar.gz文件: {output_file}")
    with tarfile.open(output_file, "w:gz") as tar:
        tar.add(base_dir, arcname="home")

def main():
    print("开始生成测试日志文件...")
    
    # 执行指定批次：20、21、22、23
    for batch in [20, 21, 22, 23]:
        print(f"\n{'='*50}")
        print(f"批次 {batch} 生成开始")
        print(f"{'='*50}")
        
        # 生成 2025-11-06 当天及之前 10 天（共 11 天）的日志
        end_date = datetime.date(2025, 11, 6)
        start_date = end_date - datetime.timedelta(days=10)
        
        for i in range(11):
            current_date = start_date + datetime.timedelta(days=i)
            date_str = current_date.strftime("%Y-%m-%d")
            # 文件名中的日期与内部日期保持一致
            file_date_str = current_date.strftime("%Y-%m-%d")
            
            print(f"\n=== 批次 {batch}，第 {i+1} 个文件，日期: {date_str} ===")
            
            # 创建目录结构
            base_dir = Path(f"home_batch{batch}_{i+1}")
            create_directory_structure(base_dir, date_str)
            print(f"创建目录结构: {base_dir}")
            
            # 生成文件
            generate_files(base_dir, date_str)
            
            # 创建tar.gz文件，命名为 BBIP_{批次}_APPLOG_{YYYY-MM-DD}.tar.gz（日期与内部日期一致）
            output_file = f"BBIP_{batch}_APPLOG_{file_date_str}.tar.gz"
            create_tar_gz(base_dir, output_file)
            
            # 清理临时目录
            import shutil
            shutil.rmtree(base_dir)
            
            print(f"完成！生成的文件: {output_file}")
            print(f"文件大小: {os.path.getsize(output_file) / 1024:.1f} KB")
        
        print(f"\n批次 {batch} 生成完成！")
    
    print(f"\n{'='*50}")
    print("所有批次生成完成！")
    print(f"{'='*50}")

if __name__ == "__main__":
    main()
