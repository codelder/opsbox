#!/usr/bin/env python3
"""
为所有支持的编码生成测试文件
"""

import sys
import os

def generate_utf8_file():
    """生成 UTF-8 编码的测试文件（包含各国文字）"""
    content = """2024-01-15 10:30:25 [INFO] System started successfully (English)
2024-01-15 10:30:26 [INFO] 系统启动成功 (简体中文)
2024-01-15 10:30:27 [INFO] システムが正常に起動しました (日本語)
2024-01-15 10:30:28 [INFO] 시스템이 성공적으로 시작되었습니다 (한국어)
2024-01-15 10:30:29 [INFO] Système démarré avec succès (Français)
2024-01-15 10:30:30 [INFO] Sistema iniciado com sucesso (Português)
2024-01-15 10:30:31 [INFO] Система успешно запущена (Русский)
2024-01-15 10:30:32 [INFO] النظام بدأ بنجاح (العربية)
2024-01-15 10:30:33 [DEBUG] Loading configuration file: config.ini
2024-01-15 10:30:34 [WARN] Cache file not found, will create new cache
2024-01-15 10:30:35 [ERROR] User login failed: invalid credentials
2024-01-15 10:30:36 [INFO] User admin logged in successfully
"""
    with open("test_utf8.log", "w", encoding="utf-8") as f:
        f.write(content)
    print("✅ 生成 UTF-8 测试文件: test_utf8.log")

def generate_gbk_file():
    """生成 GBK 编码的测试文件"""
    content = """2024-01-15 10:30:25 [INFO] 系统启动成功
2024-01-15 10:30:26 [DEBUG] 加载配置文件: config.ini
2024-01-15 10:30:27 [INFO] 数据库连接成功
2024-01-15 10:30:28 [WARN] 缓存文件不存在，将创建新缓存
2024-01-15 10:30:29 [ERROR] 用户登录失败: 用户名或密码错误
2024-01-15 10:30:30 [INFO] 用户 admin 登录成功
"""
    with open("test_gbk.log", "w", encoding="gbk") as f:
        f.write(content)
    print("✅ 生成 GBK 测试文件: test_gbk.log")

def generate_big5_file():
    """生成 Big5 编码的测试文件（繁体中文）"""
    content = """2024-01-15 10:30:25 [INFO] 系統啟動成功
2024-01-15 10:30:26 [DEBUG] 載入配置檔案: config.ini
2024-01-15 10:30:27 [INFO] 資料庫連線成功
2024-01-15 10:30:28 [WARN] 快取檔案不存在，將建立新快取
2024-01-15 10:30:29 [ERROR] 使用者登入失敗: 使用者名稱或密碼錯誤
2024-01-15 10:30:30 [INFO] 使用者 admin 登入成功
"""
    with open("test_big5.log", "w", encoding="big5") as f:
        f.write(content)
    print("✅ 生成 Big5 测试文件: test_big5.log")

def generate_shift_jis_file():
    """生成 Shift-JIS 编码的测试文件（日文）"""
    content = """2024-01-15 10:30:25 [INFO] システムが正常に起動しました
2024-01-15 10:30:26 [DEBUG] 設定ファイルを読み込み中: config.ini
2024-01-15 10:30:27 [INFO] データベース接続が確立されました
2024-01-15 10:30:28 [WARN] キャッシュファイルが見つかりません、新しいキャッシュを作成します
2024-01-15 10:30:29 [ERROR] ユーザーログインに失敗しました: 無効な認証情報
2024-01-15 10:30:30 [INFO] ユーザー admin が正常にログインしました
"""
    with open("test_shift_jis.log", "w", encoding="shift_jis") as f:
        f.write(content)
    print("✅ 生成 Shift-JIS 测试文件: test_shift_jis.log")

def generate_euc_kr_file():
    """生成 EUC-KR 编码的测试文件（韩文）"""
    content = """2024-01-15 10:30:25 [INFO] 시스템이 성공적으로 시작되었습니다
2024-01-15 10:30:26 [DEBUG] 설정 파일 로드 중: config.ini
2024-01-15 10:30:27 [INFO] 데이터베이스 연결이 설정되었습니다
2024-01-15 10:30:28 [WARN] 캐시 파일을 찾을 수 없습니다, 새 캐시를 생성합니다
2024-01-15 10:30:29 [ERROR] 사용자 로그인 실패: 잘못된 자격 증명
2024-01-15 10:30:30 [INFO] 사용자 admin이 성공적으로 로그인했습니다
2024-01-15 10:30:31 [INFO] 서버 초기화 완료
2024-01-15 10:30:32 [DEBUG] 메모리 사용량 확인: 512MB 사용 중
2024-01-15 10:30:33 [INFO] 네트워크 연결 상태 확인 중
2024-01-15 10:30:34 [WARN] 일부 설정값이 기본값으로 설정되었습니다
2024-01-15 10:30:35 [INFO] 백업 작업이 시작되었습니다
2024-01-15 10:30:36 [DEBUG] 파일 시스템 스캔 중: /var/log
2024-01-15 10:30:37 [INFO] 로그 파일 압축 완료
2024-01-15 10:30:38 [WARN] 디스크 공간이 부족합니다: 85% 사용 중
2024-01-15 10:30:39 [ERROR] 데이터베이스 쿼리 실행 실패: 타임아웃
2024-01-15 10:30:40 [INFO] 자동 재시도 메커니즘이 활성화되었습니다
2024-01-15 10:30:41 [DEBUG] 세션 만료 시간 확인: 3600초
2024-01-15 10:30:42 [INFO] 사용자 권한 검증 완료
2024-01-15 10:30:43 [WARN] 보안 경고: 비정상적인 접근 시도 감지
2024-01-15 10:30:44 [INFO] 방화벽 규칙이 업데이트되었습니다
2024-01-15 10:30:45 [DEBUG] 시스템 리소스 모니터링 시작
2024-01-15 10:30:46 [INFO] 작업 스케줄러가 정상적으로 실행 중입니다
"""
    with open("test_euc_kr.log", "w", encoding="euc-kr") as f:
        f.write(content)
    print("✅ 生成 EUC-KR 测试文件: test_euc_kr.log")

def generate_windows_1252_file():
    """生成 Windows-1252 编码的测试文件（西欧）"""
    content = """2024-01-15 10:30:25 [INFO] Système démarré avec succès
2024-01-15 10:30:26 [DEBUG] Chargement du fichier de configuration: config.ini
2024-01-15 10:30:27 [INFO] Connexion à la base de données établie
2024-01-15 10:30:28 [WARN] Fichier de cache introuvable, création d'un nouveau cache
2024-01-15 10:30:29 [ERROR] Échec de la connexion utilisateur: identifiants invalides
2024-01-15 10:30:30 [INFO] Utilisateur admin connecté avec succès
"""
    with open("test_windows_1252.log", "w", encoding="windows-1252") as f:
        f.write(content)
    print("✅ 生成 Windows-1252 测试文件: test_windows_1252.log")

def generate_iso_8859_1_file():
    """生成 ISO-8859-1 编码的测试文件（Latin-1）"""
    content = """2024-01-15 10:30:25 [INFO] Sistema iniciado com sucesso
2024-01-15 10:30:26 [DEBUG] Carregando arquivo de configuração: config.ini
2024-01-15 10:30:27 [INFO] Conexão com banco de dados estabelecida
2024-01-15 10:30:28 [WARN] Arquivo de cache não encontrado, criando novo cache
2024-01-15 10:30:29 [ERROR] Falha no login do usuário: credenciais inválidas
2024-01-15 10:30:30 [INFO] Usuário admin fez login com sucesso
"""
    with open("test_iso_8859_1.log", "w", encoding="iso-8859-1") as f:
        f.write(content)
    print("✅ 生成 ISO-8859-1 测试文件: test_iso_8859_1.log")

def generate_utf16_le_file():
    """生成 UTF-16 LE 编码的测试文件（带 BOM）"""
    content = """2024-01-15 10:30:25 [INFO] System started successfully
2024-01-15 10:30:26 [DEBUG] Loading configuration file: config.ini
2024-01-15 10:30:27 [INFO] Database connection established
2024-01-15 10:30:28 [WARN] Cache file not found, will create new cache
2024-01-15 10:30:29 [ERROR] User login failed: invalid credentials
2024-01-15 10:30:30 [INFO] User admin logged in successfully
"""
    with open("test_utf16_le.log", "wb") as f:
        # 写入 UTF-16 LE BOM
        f.write(b'\xFF\xFE')
        # 写入 UTF-16 LE 编码的内容
        f.write(content.encode('utf-16-le'))
    print("✅ 生成 UTF-16 LE 测试文件: test_utf16_le.log")

def generate_utf16_be_file():
    """生成 UTF-16 BE 编码的测试文件（带 BOM）"""
    content = """2024-01-15 10:30:25 [INFO] System started successfully
2024-01-15 10:30:26 [DEBUG] Loading configuration file: config.ini
2024-01-15 10:30:27 [INFO] Database connection established
2024-01-15 10:30:28 [WARN] Cache file not found, will create new cache
2024-01-15 10:30:29 [ERROR] User login failed: invalid credentials
2024-01-15 10:30:30 [INFO] User admin logged in successfully
"""
    with open("test_utf16_be.log", "wb") as f:
        # 写入 UTF-16 BE BOM
        f.write(b'\xFE\xFF')
        # 写入 UTF-16 BE 编码的内容
        f.write(content.encode('utf-16-be'))
    print("✅ 生成 UTF-16 BE 测试文件: test_utf16_be.log")

def generate_windows_1256_file():
    """生成 Windows-1256 编码的测试文件（阿拉伯语）"""
    content = """2024-01-15 10:30:25 [INFO] تم بدء تشغيل النظام بنجاح
2024-01-15 10:30:26 [DEBUG] تحميل ملف الإعدادات: config.ini
2024-01-15 10:30:27 [INFO] تم إنشاء اتصال بقاعدة البيانات
2024-01-15 10:30:28 [WARN] لم يتم العثور على ملف التخزين المؤقت، سيتم إنشاء تخزين مؤقت جديد
2024-01-15 10:30:29 [ERROR] فشل تسجيل دخول المستخدم: بيانات اعتماد غير صحيحة
2024-01-15 10:30:30 [INFO] تم تسجيل دخول المستخدم admin بنجاح
2024-01-15 10:30:31 [INFO] اكتملت تهيئة الخادم
2024-01-15 10:30:32 [DEBUG] التحقق من استخدام الذاكرة: 512 ميجابايت قيد الاستخدام
2024-01-15 10:30:33 [INFO] التحقق من حالة الاتصال بالشبكة
2024-01-15 10:30:34 [WARN] تم تعيين بعض القيم الافتراضية
2024-01-15 10:30:35 [INFO] بدأت عملية النسخ الاحتياطي
"""
    try:
        with open("test_windows_1256.log", "w", encoding="windows-1256") as f:
            f.write(content)
        print("✅ 生成 Windows-1256 测试文件: test_windows_1256.log")
    except LookupError:
        # 如果系统不支持 windows-1256，尝试使用 cp1256
        try:
            with open("test_windows_1256.log", "w", encoding="cp1256") as f:
                f.write(content)
            print("✅ 生成 Windows-1256 测试文件: test_windows_1256.log (使用 cp1256)")
        except LookupError:
            # 如果都不支持，使用 UTF-8 并提示
            print("⚠️  系统不支持 Windows-1256 编码，使用 UTF-8 生成文件")
            with open("test_windows_1256.log", "w", encoding="utf-8") as f:
                f.write(content)
            print("✅ 生成 Windows-1256 测试文件: test_windows_1256.log (UTF-8)")

def main():
    """生成所有编码的测试文件"""
    print("开始生成编码测试文件...\n")
    
    try:
        generate_utf8_file()
        generate_gbk_file()
        generate_big5_file()
        generate_shift_jis_file()
        generate_euc_kr_file()
        generate_windows_1252_file()
        generate_iso_8859_1_file()
        generate_utf16_le_file()
        generate_utf16_be_file()
        generate_windows_1256_file()
        
        print("\n✅ 所有测试文件生成完成！")
        print("\n生成的文件列表：")
        files = [
            "test_utf8.log",
            "test_gbk.log",
            "test_big5.log",
            "test_shift_jis.log",
            "test_euc_kr.log",
            "test_windows_1252.log",
            "test_iso_8859_1.log",
            "test_utf16_le.log",
            "test_utf16_be.log",
            "test_windows_1256.log"
        ]
        for f in files:
            if os.path.exists(f):
                size = os.path.getsize(f)
                print(f"  - {f} ({size} 字节)")
        
    except Exception as e:
        print(f"❌ 生成文件失败: {e}", file=sys.stderr)
        sys.exit(1)

if __name__ == "__main__":
    main()

