"""Common utilities for generating synthetic log archives used in tests."""

from __future__ import annotations

import datetime as _dt
import random
import tarfile
from pathlib import Path
from typing import Optional

__all__ = [
    "DEFAULT_TRACE_DATE",
    "create_directory_structure",
    "create_tar_gz",
    "generate_files",
    "generate_log_entry",
    "generate_log_file",
    "generate_trace_filename",
]

# Shared templates and vocabularies ---------------------------------------------------------

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
]

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
]

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
]

SERVICE_NAMES = [
    "bjbcbp-bjetcs",
    "bjbcbp-bjbfdzd",
    "bjbcbp-bjeups",
    "bjbcbp-bjbfrhhz",
    "bjbcbp-bjmass",
    "bjbcbp-bjchps",
    "bjbcbp-bjegps",
    "bjbcbp-gateway",
    "bjbcbp-bjbfyff",
    "bjbcbp-bjhpfs",
    "bjbbip-agent",
    "bjbcbp-bjbfjzth",
    "bjbcbp-bjbcpm",
    "bjbcbp-bjcpos",
    "bjbbip-bbippubsvr",
    "bjbbip-gateway",
    "bjbcbp-bjbknd",
    "bjbcbp-bjbbts",
    "bjbcbp-bjcbcm",
    "bjbcbp-bjmasstb",
    "bjbbip-bbipsche",
    "bjbcbp-bjrcis",
    "bjbcbp-bjcars",
    "bjbcbp-bjbfrbs",
    "bjbcbp-bjsrsp",
    "bjbcbp-bjbfgjj",
    "bjbcbp-bjevca",
    "bjbcbp-ntpselc",
    "bjbcbp-bjlogs",
    "bjbcbp-bjntps",
    "bjbcbp-bjbetc",
    "bjbbip-bbipbtp",
    "bjbcbp-repeater",
    "bjbcbp-bjptcr",
    "bjbcbp-bjbbfebbos",
    "bjbcbp-bjkfqzw",
    "bjbcbp-bjegpsyth",
    "bjbbip-bbipgove",
    "bjbcbp-bjbfshb",
    "bjbcbp-bjsypt",
    "bjbcbp-bbip-forward",
    "bjbbip-bjagtsx",
    "bjbcbp-bjbmicp",
    "bjbcbp-bjagtsdf",
    "bjbcbp-bjbfcbs",
    "bjbcbp-bjbbfecore",
]

DEFAULT_TRACE_DATE = "2025-08-18"


# Helper factories -------------------------------------------------------------------------

def _generate_user_id() -> str:
    return f"user_{random.randint(1000, 9999)}"


def _generate_order_id() -> str:
    return f"ORD{random.randint(100000, 999999)}"


def _generate_transaction_id() -> str:
    return f"TXN{random.randint(1000000, 9999999)}"


def _generate_api_path() -> str:
    paths = [
        "/api/users",
        "/api/orders",
        "/api/payments",
        "/api/products",
        "/api/auth",
        "/api/gateway",
        "/api/system",
    ]
    return random.choice(paths)


def _generate_action() -> str:
    actions = [
        "login",
        "logout",
        "create",
        "update",
        "delete",
        "view",
        "search",
        "登录",
        "登出",
        "创建",
        "更新",
        "删除",
        "查看",
        "搜索",
    ]
    return random.choice(actions)


def _normalize_date(date_str: Optional[str]) -> str:
    return (date_str or DEFAULT_TRACE_DATE).replace("-", "")


# Public helpers ----------------------------------------------------------------------------

def generate_trace_filename(date_str: Optional[str] = None) -> str:
    """Create a pseudo-random trace log filename for the given date."""

    hour = random.randint(0, 23)
    minute = random.randint(0, 59)
    second = random.randint(0, 59)
    random_suffix = random.randint(1000000, 9999999)
    date_numeric = _normalize_date(date_str)

    return f"{hour:02d}{minute:02d}{second:02d}04{date_numeric}{random_suffix}.log"


def generate_log_entry() -> str:
    """Generate a single synthetic log entry mixing Chinese and English templates."""

    timestamp = _dt.datetime.now() - _dt.timedelta(
        days=random.randint(0, 30),
        hours=random.randint(0, 23),
        minutes=random.randint(0, 59),
    )

    template_pool = CHINESE_LOG_TEMPLATES if random.random() < 0.6 else ENGLISH_LOG_TEMPLATES
    template = random.choice(template_pool)

    log_data = {
        "user_id": _generate_user_id(),
        "order_id": _generate_order_id(),
        "transaction_id": _generate_transaction_id(),
        "amount": random.randint(10, 9999),
        "error_code": random.randint(100, 599),
        "error_msg": random.choice(ERROR_MESSAGES),
        "retry_count": random.randint(1, 5),
        "hit_rate": random.randint(70, 99),
        "api_path": _generate_api_path(),
        "response_time": random.randint(10, 2000),
        "sql_time": random.randint(5, 500),
        "business_time": random.randint(10, 1000),
        "action": _generate_action(),
        "startup_time": random.randint(1, 30),
        "memory_usage": random.randint(20, 90),
        "free_space": random.randint(1, 100),
        "service_name": random.choice(SERVICE_NAMES),
        "exception_msg": random.choice(ERROR_MESSAGES),
        "service_error": random.choice(ERROR_MESSAGES),
        "sync_count": random.randint(100, 10000),
        "task_name": f"task_{random.randint(1, 100)}",
        "config_key": f"config_{random.randint(1, 50)}",
        "config_value": f"value_{random.randint(1, 100)}",
        "archive_file": f"archive_{random.randint(1, 1000)}.log",
    }

    log_message = template.format(**log_data)

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


def generate_log_file(file_path: Path, line_count: int) -> None:
    """Write ``line_count`` synthetic log lines into ``file_path``."""

    with open(file_path, "w", encoding="utf-8") as file_obj:
        for _ in range(line_count):
            file_obj.write(generate_log_entry() + "\n")


def create_directory_structure(base_dir: Path, date_str: Optional[str] = None) -> Path:
    """Create the nested directory layout expected by tests."""

    trace_date = date_str or DEFAULT_TRACE_DATE

    base_dir.mkdir(exist_ok=True)

    bbipadm_dir = base_dir / "bbipadm"
    bbipadm_dir.mkdir(exist_ok=True)

    logs_dir = bbipadm_dir / "logs"
    logs_dir.mkdir(exist_ok=True)

    for service in SERVICE_NAMES:
        service_dir = logs_dir / service
        service_dir.mkdir(exist_ok=True)

        if service not in [
            "bjbcbp-bjbbfecsbp",
            "bjbcbp-bjbfrhhz",
            "bjbcbp-gateway",
            "bjbbip-agent",
            "bjbcbp-bjbcpm",
            "bjbbip-bbipsche",
            "bjbcbp-bjlogs",
            "msk",
            "bjbcbp-bjntps",
            "bjbcbp-bjbetc",
            "bjbbip-bbipbtp",
            "bjbcbp-repeater",
            "bjbcbp-bjbbfebbos",
            "bjbcbp-bjkfqzw",
            "bjbcbp-bjegpsyth",
            "bjbbip-bbipgove",
            "bjbcbp-bjbfshb",
            "bjbcbp-bjsypt",
            "bjbcbp-bbip-forward",
            "bjbbip-bjagtsx",
            "bjbcbp-bjbmicp",
            "bjbcbp-bjagtsdf",
            "bjbcbp-bjbfcbs",
            "rocketmqlogs",
            "bjbcbp-bjbbfecore",
        ]:
            trace_dir = service_dir / "trace"
            trace_dir.mkdir(exist_ok=True)

            date_dir = trace_dir / trace_date
            date_dir.mkdir(exist_ok=True)

        if service not in [
            "bjbcbp-bjbbfecsbp",
            "bjbcbp-bjbfrhhz",
            "bjbcbp-bjlogs",
            "msk",
            "bjbcbp-bjntps",
            "bjbcbp-bjbetc",
            "rocketmqlogs",
            "bjbcbp-bjbbfecore",
        ]:
            date_dir = service_dir / trace_date
            date_dir.mkdir(exist_ok=True)

    return base_dir


def generate_files(base_dir: Path, date_str: Optional[str] = None) -> None:
    """Populate ``base_dir`` with synthetic log files for each service."""

    trace_date = date_str or DEFAULT_TRACE_DATE
    logs_dir = base_dir / "bbipadm" / "logs"

    for service in SERVICE_NAMES:
        service_dir = logs_dir / service

        if service not in [
            "bjbcbp-bjbbfecsbp",
            "bjbcbp-bjbfrhhz",
            "bjbcbp-gateway",
            "bjbbip-agent",
            "bjbcbp-bjbcpm",
            "bjbbip-bbipsche",
            "bjbcbp-bjlogs",
            "msk",
            "bjbcbp-bjntps",
            "bjbcbp-bjbetc",
            "bjbbip-bbipbtp",
            "bjbcbp-repeater",
            "bjbcbp-bjbbfebbos",
            "bjbcbp-bjkfqzw",
            "bjbcbp-bjegpsyth",
            "bjbbip-bbipgove",
            "bjbcbp-bjbfshb",
            "bjbcbp-bjsypt",
            "bjbcbp-bbip-forward",
            "bjbbip-bjagtsx",
            "bjbcbp-bjbmicp",
            "bjbcbp-bjagtsdf",
            "bjbcbp-bjbfcbs",
            "rocketmqlogs",
            "bjbcbp-bjbbfecore",
        ]:
            system_log = service_dir / "system.log"
            print(f"Writing {system_log} (200 lines)")
            generate_log_file(system_log, 200)

        if service == "bjbcbp-gateway":
            app_tran_time_log = service_dir / "app_tranTime.log"
            app_sql_time_log = service_dir / "app_sqlTime.log"
            print(f"Writing {app_tran_time_log} (150 lines)")
            print(f"Writing {app_sql_time_log} (120 lines)")
            generate_log_file(app_tran_time_log, 150)
            generate_log_file(app_sql_time_log, 120)

        trace_dir = service_dir / "trace" / trace_date
        if trace_dir.exists():
            trace_count = random.randint(5, 15)
            for _ in range(trace_count):
                trace_filename = generate_trace_filename(trace_date)
                trace_file = trace_dir / trace_filename
                print(f"Writing {trace_file} (50-200 lines)")
                line_count = random.randint(50, 200)
                generate_log_file(trace_file, line_count)

        date_dir = service_dir / trace_date
        if date_dir.exists():
            file_count = random.randint(1, 3)
            for _ in range(file_count):
                filename = f"{random.randint(1000000, 9999999)}.log"
                date_file = date_dir / filename
                print(f"Writing {date_file} (100-300 lines)")
                line_count = random.randint(100, 300)
                generate_log_file(date_file, line_count)


def create_tar_gz(base_dir: Path, output_file: Path, arcname: str = "home") -> None:
    """Create a ``tar.gz`` archive from ``base_dir``."""

    print(f"Creating tar.gz archive: {output_file}")
    with tarfile.open(output_file, "w:gz") as tar:
        tar.add(base_dir, arcname=arcname)

