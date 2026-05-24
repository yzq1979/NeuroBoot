-- Test schema for app/src/rag/mod.rs unit tests (Phase 1 -- no vec0 table).
-- Mirrors the schema produced by `tools-dev/build-error-db.py
-- --fixtures-only --no-embed`. Phase 2 tests live in test_schema_v2.sql.

CREATE TABLE entries (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    kind      TEXT NOT NULL,
    code      TEXT,
    name      TEXT,
    cn_desc   TEXT NOT NULL,
    causes    TEXT,
    docs_url  TEXT,
    keywords  TEXT,
    embedding BLOB
);
CREATE INDEX idx_entries_code ON entries(code);

CREATE VIRTUAL TABLE entries_fts USING fts5(
    name,
    cn_desc,
    causes,
    keywords,
    code UNINDEXED,
    content='entries',
    content_rowid='id',
    tokenize='trigram'
);

CREATE TRIGGER entries_ai AFTER INSERT ON entries BEGIN
    INSERT INTO entries_fts(rowid, name, cn_desc, causes, keywords, code)
    VALUES (new.id, new.name, new.cn_desc, new.causes, new.keywords, new.code);
END;

CREATE TABLE db_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
INSERT INTO db_meta(key, value) VALUES ('schema_version', '1');
INSERT INTO db_meta(key, value) VALUES ('has_embeddings', '0');

-- Test rows -- match the real fixture wording so the tests are realistic.
INSERT INTO entries (kind, code, name, cn_desc, causes, docs_url, keywords) VALUES
    ('bugcheck', '7B', 'INACCESSIBLE_BOOT_DEVICE',
     '启动时找不到 / 无法访问启动盘',
     '常见原因：启动盘控制器驱动不在；BIOS 改了 AHCI/RAID 模式；系统盘硬件故障',
     'https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/bug-check-0x7b--inaccessible-boot-device',
     '蓝屏 起不来 boot 启动盘 0x7B BSOD AHCI RAID 控制器');

INSERT INTO entries (kind, code, name, cn_desc, causes, docs_url, keywords) VALUES
    ('bugcheck', 'D1', 'DRIVER_IRQL_NOT_LESS_OR_EQUAL',
     '驱动以错误中断级访问可分页内存（驱动 bug 高度嫌疑）',
     '常见原因：驱动 bug（>90% 这个原因）。修复：更新该驱动；回滚到稳定版本',
     'https://learn.microsoft.com/en-us/windows-hardware/drivers/debugger/bug-check-0xd1--driver-irql-not-less-or-equal',
     '蓝屏 0xD1 IRQL 驱动 可分页 BSOD CausedByDriver');

INSERT INTO entries (kind, code, name, cn_desc, causes, docs_url, keywords) VALUES
    ('hresult', '80070005', 'E_ACCESSDENIED',
     '访问被拒绝（权限不足）',
     '原因：操作需要管理员权限；文件被独占占用；NTFS ACL 限制',
     'https://learn.microsoft.com/en-us/windows/win32/debug/system-error-codes--0-499-',
     '访问拒绝 权限 0x80070005 admin TrustedInstaller takeown icacls');
