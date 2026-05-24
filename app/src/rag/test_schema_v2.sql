-- Phase 2 test schema: entries + entries_fts + entries_vec virtual table
-- + db_meta.has_embeddings = '1'. Synthetic 4-dim vectors so we can
-- hand-craft KNN expectations in tests.

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
    name, cn_desc, causes, keywords, code UNINDEXED,
    content='entries', content_rowid='id', tokenize='trigram'
);
CREATE TRIGGER entries_ai AFTER INSERT ON entries BEGIN
    INSERT INTO entries_fts(rowid, name, cn_desc, causes, keywords, code)
    VALUES (new.id, new.name, new.cn_desc, new.causes, new.keywords, new.code);
END;

-- 4-dim vectors for test brevity (production uses 1024).
CREATE VIRTUAL TABLE entries_vec USING vec0(embedding float[4]);

CREATE TABLE db_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
INSERT INTO db_meta(key, value) VALUES ('schema_version', '2');
INSERT INTO db_meta(key, value) VALUES ('has_embeddings', '1');
INSERT INTO db_meta(key, value) VALUES ('embedding_dim', '4');

-- 3 fixture entries chosen so vector vs fts5 disagree usefully:
--   id=1 'INACCESSIBLE_BOOT_DEVICE' (vec=[1,0,0,0]) -- exact code match
--   id=2 'DRIVER_IRQL_NOT_LESS_OR_EQUAL' (vec=[0,1,0,0]) -- fts5 only
--   id=3 'BSOD_GENERIC' (vec=[0.7,0.7,0,0]) -- vec hit only
INSERT INTO entries (kind, code, name, cn_desc, causes, docs_url, keywords) VALUES
    ('bugcheck', '7B', 'INACCESSIBLE_BOOT_DEVICE',
     '启动时找不到启动盘',
     '启动盘控制器驱动 / AHCI/RAID 切换',
     'https://learn.microsoft.com/x/7b',
     '蓝屏 启动盘 boot 0x7B');
INSERT INTO entries (kind, code, name, cn_desc, causes, docs_url, keywords) VALUES
    ('bugcheck', 'D1', 'DRIVER_IRQL_NOT_LESS_OR_EQUAL',
     '驱动以错误中断级访问内存',
     '驱动 bug；analyze_minidump 看 CausedByDriver',
     'https://learn.microsoft.com/x/d1',
     '蓝屏 0xD1 IRQL 驱动');
INSERT INTO entries (kind, code, name, cn_desc, causes, docs_url, keywords) VALUES
    ('bugcheck', 'FF', 'BSOD_GENERIC',
     '蓝色屏幕通用条目（仅向量命中）',
     '本条用来测试向量召回独立于 FTS5',
     'https://learn.microsoft.com/x/ff',
     'unrelated_token_xyz123');

-- Sync vec table rows with entries. sqlite-vec INSERT uses the same rowid
-- so we can JOIN entries.id = entries_vec.rowid. 4 f32 little-endian bytes
-- per element = 16 bytes per vector.
--
-- Embeddings:
--   id=1: [1.0, 0.0, 0.0, 0.0] -> 0000803F 00000000 00000000 00000000
--   id=2: [0.0, 1.0, 0.0, 0.0] -> 00000000 0000803F 00000000 00000000
--   id=3: [0.7, 0.7, 0.0, 0.0] -> 33333F3F 33333F3F 00000000 00000000
INSERT INTO entries_vec(rowid, embedding) VALUES
    (1, X'0000803F000000000000000000000000'),
    (2, X'000000000000803F0000000000000000'),
    (3, X'3333333F3333333F0000000000000000');
