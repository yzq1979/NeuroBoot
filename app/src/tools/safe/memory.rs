//! [Safe] memory —— v3.0 W6-7 持久化 memory 工具（单工具 + 6 子命令）。
//!
//! 对齐 [Anthropic Memory Tool](https://platform.claude.com/docs/en/agents-and-tools/tool-use/memory-tool)
//! 的「单工具 command 派发」模式 —— 而不是 6 个独立工具 —— 是因为：
//! 1. 节省 system prompt 工具列表的 token
//! 2. 6 操作语义相关，独立暴露反而让 LLM 难定位用哪个
//! 3. 跟 Claude Code `view/create/str_replace/insert/delete/rename` 的命名一致
//!
//! 文件根：U 盘 `<root>\NeuroBoot\memories\`，由 [`crate::memory::scan_memory_root`] /
//! [`crate::memory::ensure_root_for_create`] 在 PE 环境运行时决定。
//!
//! ## 安全
//!
//! - 所有路径走 [`crate::memory::resolve_inside_root`] 防 traversal
//! - 是 safe 工具 —— 不要求确认弹窗（写入限 memory root 内，不能影响系统目录）
//! - delete 只删文件，不递归删目录

use serde_json::{json, Value};

use crate::memory::{
    create, delete, ensure_root_for_create, insert, rename, resolve_inside_root, scan_memory_root,
    str_replace, view, MemoryError,
};
use crate::tools::registry::{SafetyClass, Tool, ToolError, ToolErrorKind, ToolOutput};

pub struct Memory;

impl Tool for Memory {
    fn name(&self) -> &str {
        "memory"
    }

    fn description(&self) -> &str {
        "持久化记忆（跨 PE 重启 / 跨会话）—— 在 U 盘 `NeuroBoot\\memories\\` 下管理 markdown 文件。\n\
         \n\
         **When to use**:\n\
         - 用户说「记住 X / 下次再来时还要看这个」→ `create` 或 `str_replace`\n\
         - 用户问「之前那台机是什么 SN / 上次蓝屏什么时候」→ `view MEMORY.md`\n\
         - 启动时 `MEMORY.md` 已自动注入 system prompt，**不要重复 view** 除非用户问最新\n\
         - **不**用于：单次会话临时上下文；系统状态查询（用 `read_system_info` 等专门工具）\n\
         \n\
         **Parameters**:\n\
         - `command`: 6 选 1 —— view / create / str_replace / insert / delete / rename\n\
         - `path`: memory root 内的相对路径，不能含 `..` / 绝对路径。典型：`MEMORY.md` / `projects/foo.md`\n\
         - `content` (for create): 文件全文（覆盖现有）\n\
         - `old_str` / `new_str` (for str_replace): old_str 必须文件内唯一出现\n\
         - `insert_line` / `new_lines` (for insert): 在第 N 行**前**插入；0=最前，N=总行数=末尾\n\
         - `new_path` (for rename): 新相对路径\n\
         \n\
         **Returns**:\n\
         - `view 文件` → 文件全文；`view 目录`（path='.'）→ 子项列表，目录后带 `/`\n\
         - 写操作 → 简短确认信息\n\
         \n\
         **Example output**:\n\
         - `view MEMORY.md` → `# 用户偏好\\n- 用 PowerShell 不用 cmd\\n- C 盘是 SSD ...`\n\
         - `create projects/laptop.md` content=`X1 Gen11` → `（已创建）projects/laptop.md (8 bytes)`\n\
         \n\
         **Notes**:\n\
         - **路径只能在 memory root 内**；PE 是 ramdisk，只有 U 盘能持久化\n\
         - `str_replace` 要 old_str 文件内唯一；多次出现返 SubstringAmbiguous\n\
         - 用户没要求时不要主动写；写前先 view 看现有结构"
    }

    fn safety(&self) -> SafetyClass {
        SafetyClass::Safe
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "enum": ["view", "create", "str_replace", "insert", "delete", "rename"],
                    "description": "6 子命令之一"
                },
                "path": {
                    "type": "string",
                    "description": "memory root 内的相对路径（如 'MEMORY.md' / 'projects/foo.md'）；view 目录传 '.'"
                },
                "content": {
                    "type": "string",
                    "description": "仅 create 用 —— 文件全文"
                },
                "old_str": {
                    "type": "string",
                    "description": "仅 str_replace 用 —— 要替换的子串（必须文件内唯一出现）"
                },
                "new_str": {
                    "type": "string",
                    "description": "仅 str_replace 用 —— 新子串"
                },
                "insert_line": {
                    "type": "integer",
                    "description": "仅 insert 用 —— 在第 N 行之前插入（0=文件最前，N=总行数=末尾）",
                    "minimum": 0
                },
                "new_lines": {
                    "type": "string",
                    "description": "仅 insert 用 —— 要插入的新行（可多行用 \\n 分隔）"
                },
                "new_path": {
                    "type": "string",
                    "description": "仅 rename 用 —— 新相对路径"
                }
            },
            "required": ["command", "path"]
        })
    }

    fn execute(&self, args: &Value) -> ToolOutput {
        let command = require_string(args, "command")?;

        // 先验 command 合法性 —— 让 LLM 立刻看到 InvalidArgument，
        // 而不是被「找不到 memory root」掩盖
        const VALID_COMMANDS: &[&str] =
            &["view", "create", "str_replace", "insert", "delete", "rename"];
        if !VALID_COMMANDS.contains(&command) {
            return Err(ToolError::with_kind(
                ToolErrorKind::InvalidArgument,
                format!(
                    "未知 command '{command}'，合法值：view / create / str_replace / insert / delete / rename"
                ),
            ));
        }

        // create 命令需要可写 root（自动建）；其它命令只需现有 root
        let root = if command == "create" {
            ensure_root_for_create().map_err(memory_err_to_tool_err)?
        } else {
            scan_memory_root().ok_or_else(|| {
                ToolError::with_kind(
                    ToolErrorKind::NotFound,
                    "找不到 memory root（应在 U 盘的 NeuroBoot\\memories\\ 目录）。\
                     先调 memory(command='create', path='MEMORY.md', content='...') 会在第一个可写 U 盘上自动建。",
                )
            })?
        };

        let path = require_string(args, "path")?;

        // 防御性预校验：让 LLM 立刻看到 traversal 错误而不是被某子命令吞
        if command != "rename" {
            resolve_inside_root(&root, path).map_err(memory_err_to_tool_err)?;
        }

        match command {
            "view" => view(&root, path).map_err(memory_err_to_tool_err),
            "create" => {
                let content = args
                    .get("content")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        ToolError::with_kind(
                            ToolErrorKind::InvalidArgument,
                            "create 命令需要 content 参数",
                        )
                    })?;
                create(&root, path, content).map_err(memory_err_to_tool_err)?;
                Ok(format!(
                    "（已创建）{path} ({} bytes)",
                    content.as_bytes().len()
                ))
            }
            "str_replace" => {
                let old_str = require_string(args, "old_str")?;
                let new_str = args.get("new_str").and_then(Value::as_str).unwrap_or("");
                str_replace(&root, path, old_str, new_str).map_err(memory_err_to_tool_err)?;
                Ok(format!("（已替换）{path}"))
            }
            "insert" => {
                let insert_line = args.get("insert_line").and_then(Value::as_u64).ok_or_else(|| {
                    ToolError::with_kind(
                        ToolErrorKind::InvalidArgument,
                        "insert 命令需要 insert_line（非负整数）参数",
                    )
                })? as usize;
                let new_lines = require_string(args, "new_lines")?;
                insert(&root, path, insert_line, new_lines).map_err(memory_err_to_tool_err)?;
                Ok(format!("（已插入）{path} 在第 {insert_line} 行前"))
            }
            "delete" => {
                delete(&root, path).map_err(memory_err_to_tool_err)?;
                Ok(format!("（已删除）{path}"))
            }
            "rename" => {
                let new_path = require_string(args, "new_path")?;
                rename(&root, path, new_path).map_err(memory_err_to_tool_err)?;
                Ok(format!("（已重命名）{path} -> {new_path}"))
            }
            // unreachable: 上方已用 VALID_COMMANDS 白名单提前 reject
            _ => unreachable!("command 白名单已过滤"),
        }
    }
}

fn require_string<'a>(args: &'a Value, key: &str) -> Result<&'a str, ToolError> {
    args.get(key).and_then(Value::as_str).ok_or_else(|| {
        ToolError::with_kind(
            ToolErrorKind::InvalidArgument,
            format!("缺少 {key} 参数（应为字符串）"),
        )
    })
}

fn memory_err_to_tool_err(e: MemoryError) -> ToolError {
    let kind = match &e {
        MemoryError::PathTraversal(_) => ToolErrorKind::InvalidArgument,
        MemoryError::NoWritableDrive => ToolErrorKind::NotFound,
        MemoryError::NotFound(_) => ToolErrorKind::NotFound,
        MemoryError::SubstringNotFound | MemoryError::SubstringAmbiguous(_) => {
            ToolErrorKind::InvalidArgument
        }
        MemoryError::LineOutOfRange { .. } => ToolErrorKind::InvalidArgument,
        MemoryError::IoError(_) => ToolErrorKind::Other,
        MemoryError::InvalidArgument(_) => ToolErrorKind::InvalidArgument,
    };
    ToolError::with_kind(kind, e.display_for_model())
}

#[cfg(test)]
mod tests {
    use super::*;

    // 注：本工具**故意**叫 `memory`（无下划线）—— 对齐 Anthropic 官方 Memory Tool 命名，
    // 跟 `load_skill` / `propose_plan` 这种 verb_object 工具语义不同：它命名的是资源不是动作。
    // 所以不用 `assert_v30_description_convention`（强制 verb_object），改用宽松等价检查：
    // 仍要 4 个 section marker + 长度在 [200, 1500]。
    #[test]
    fn description_has_4_sections_and_valid_length() {
        let desc = Memory.description();
        for marker in ["**When to use**", "**Parameters**", "**Returns**", "**Notes**"] {
            assert!(desc.contains(marker), "missing section `{marker}`");
        }
        let len = desc.chars().count();
        assert!(
            (200..=1500).contains(&len),
            "description length {len} out of band [200, 1500]"
        );
    }

    #[test]
    fn rejects_missing_command() {
        let r = Memory.execute(&json!({ "path": "MEMORY.md" }));
        assert!(r.is_err());
        let err = r.unwrap_err();
        assert_eq!(err.kind, ToolErrorKind::InvalidArgument);
        assert!(err.message.contains("command"));
    }

    #[test]
    fn rejects_unknown_command() {
        // command 是 enum 但 schema 不强制运行时校验；execute 自己兜底
        let r = Memory.execute(&json!({ "command": "wat", "path": "x.md" }));
        assert!(r.is_err());
        let err = r.unwrap_err();
        assert_eq!(err.kind, ToolErrorKind::InvalidArgument);
        assert!(err.message.contains("未知 command"));
    }

    // 注：不测全 6 个子命令的正向路径 —— 那些已经在 crate::memory::tests 覆盖。
    // 这里只测「工具入口的参数解析 + 错误映射」表层。
}
