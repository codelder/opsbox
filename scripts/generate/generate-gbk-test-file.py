#!/usr/bin/env python3
"""
生成 GBK 编码的测试文件
用于测试 GBK 文件检索功能
"""

import sys

def generate_gbk_test_file():
    # 测试内容：包含中文和英文的日志内容
    content = """2024-01-15 10:30:25 [INFO] 系统启动成功
2024-01-15 10:30:26 [DEBUG] 加载配置文件: config.ini
2024-01-15 10:30:27 [INFO] 数据库连接成功
2024-01-15 10:30:28 [WARN] 缓存文件不存在，将创建新缓存
2024-01-15 10:30:29 [ERROR] 用户登录失败: 用户名或密码错误
2024-01-15 10:30:30 [INFO] 用户 admin 登录成功
2024-01-15 10:30:31 [DEBUG] 查询用户信息: user_id=12345
2024-01-15 10:30:32 [INFO] 订单处理完成: order_id=ORD-2024-001
2024-01-15 10:30:33 [WARN] 库存不足: product_id=PROD-001, 当前库存=5
2024-01-15 10:30:34 [ERROR] 支付失败: 余额不足
2024-01-15 10:30:35 [INFO] 支付成功: transaction_id=TX-2024-001, amount=99.99
2024-01-15 10:30:36 [DEBUG] 发送邮件通知: recipient=user@example.com
2024-01-15 10:30:37 [INFO] 邮件发送成功
2024-01-15 10:30:38 [ERROR] 文件上传失败: 文件大小超过限制
2024-01-15 10:30:39 [INFO] 文件上传成功: filename=report.pdf, size=1024KB
2024-01-15 10:30:40 [WARN] API 请求超时: endpoint=/api/users
2024-01-15 10:30:41 [INFO] API 请求成功: status_code=200
2024-01-15 10:30:42 [DEBUG] 处理任务队列: queue_size=10
2024-01-15 10:30:43 [INFO] 任务处理完成: task_id=TASK-001
2024-01-15 10:30:44 [ERROR] 数据库查询失败: SQL syntax error
2024-01-15 10:30:45 [INFO] 数据库查询成功: rows=100
"""

    # 使用 GBK 编码写入文件
    output_file = "test_gbk.log"
    try:
        with open(output_file, 'w', encoding='gbk') as f:
            f.write(content)
        print(f"✅ 成功生成 GBK 编码测试文件: {output_file}")
        print(f"   文件大小: {len(content.encode('gbk'))} 字节")
        print(f"   行数: {len(content.splitlines())} 行")
        print(f"\n文件内容预览（前 3 行）:")
        for i, line in enumerate(content.splitlines()[:3], 1):
            print(f"   {i}. {line}")
        return output_file
    except Exception as e:
        print(f"❌ 生成文件失败: {e}", file=sys.stderr)
        sys.exit(1)

if __name__ == "__main__":
    generate_gbk_test_file()

