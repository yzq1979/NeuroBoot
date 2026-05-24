//! 电源控制：重启 / 关机 / 退出程序 —— PE 真测发现 UI 缺失这些按钮。
//!
//! 阶段 v1.0.1 新增（用户反馈：上次 PE 真测出问题只能长按电源键）。
//!
//! 设计：
//! - PE 里 `wpeutil.exe reboot` / `wpeutil.exe shutdown` 是 ADK 提供的标准命令
//! - 在主系统（开发机）跑会失败：wpeutil 不存在，或者「only valid in WinPE」
//!   失败时返回 Err，UI 显示错误消息（开发机调试时不会崩溃）
//! - 「退出程序」走 std::process::exit(0)，在 PE 里会回到 startnet.cmd 继续往下走（cmd 提示符）

use std::process::Command;

use eframe::egui;

/// 用户可触发的电源动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerAction {
    /// 重启电脑 (`wpeutil reboot`)
    Reboot,
    /// 关机 (`wpeutil shutdown`)
    Shutdown,
    /// 退出 NeuroBoot 程序（回到 startnet.cmd 后续命令，即 PE cmd 提示符）
    ExitToCmd,
}

impl PowerAction {
    pub fn button_label(self) -> &'static str {
        match self {
            PowerAction::Reboot => "重启电脑",
            PowerAction::Shutdown => "关机",
            PowerAction::ExitToCmd => "退出到 cmd",
        }
    }

    pub fn confirm_dialog_title(self) -> &'static str {
        match self {
            PowerAction::Reboot => "确认重启电脑",
            PowerAction::Shutdown => "确认关机",
            PowerAction::ExitToCmd => "确认退出 NeuroBoot",
        }
    }

    /// 弹窗里展示的影响描述。
    pub fn confirm_body(self) -> &'static str {
        match self {
            PowerAction::Reboot => {
                "电脑将立刻重启。如果你有未保存的工具结果或对话，请先复制到 U 盘。\n\n\
                 PE 重启后会再次走 Ventoy 启动菜单 —— 选 NeuroBoot.iso 可重新进 PE。"
            }
            PowerAction::Shutdown => {
                "电脑将立刻关机。如果你有未保存的工具结果或对话，请先复制到 U 盘。"
            }
            PowerAction::ExitToCmd => {
                "NeuroBoot 程序将退出，回到 PE 命令行提示符 (X:\\NeuroBoot>)。\n\n\
                 在 cmd 里可手敲 `wpeutil shutdown` / `wpeutil reboot` / 任意维护命令。\n\
                 重新打开 NeuroBoot 在 cmd 里敲 `neuroboot.exe` 即可。"
            }
        }
    }

    /// 执行此动作。成功时不返回（退出 / 重启 / 关机会让进程消失）；
    /// 真返回的是执行失败的 Err 描述（开发机上跑会进这个分支）。
    pub fn execute(self) -> Result<(), String> {
        match self {
            PowerAction::Reboot => spawn_wpeutil("reboot"),
            PowerAction::Shutdown => spawn_wpeutil("shutdown"),
            PowerAction::ExitToCmd => {
                // 不需要确认 cleanup —— PE 里关进程直接没；主系统里 std::process::exit
                // 不会跑 destructor，但 NeuroBoot 没有需要 flush 的状态
                std::process::exit(0);
            }
        }
    }
}

/// 调 wpeutil.exe，失败返回中文错误描述。
fn spawn_wpeutil(verb: &str) -> Result<(), String> {
    // wpeutil 在 PE 的 X:\Windows\System32\ 下，主系统不存在
    let result = Command::new("wpeutil.exe").arg(verb).status();
    match result {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(format!(
            "wpeutil {verb} 返回非零退出码 {status}。这个命令只能在 WinPE 里跑；\
             如果你在主系统调试 NeuroBoot，这个错误是预期的。"
        )),
        Err(e) => Err(format!(
            "执行 wpeutil.exe {verb} 失败：{e}。\
             如果你在主系统（非 PE）调试 NeuroBoot，这个错误是预期的（主系统不带 wpeutil）。"
        )),
    }
}

/// 渲染电源动作的确认弹窗。
///
/// 返回值：
/// - None：仍在等用户决定
/// - Some(true)：用户确认执行（main.rs 应调 action.execute()）
/// - Some(false)：用户取消（main.rs 应清空 pending）
pub fn draw_power_confirmation_dialog(
    ctx: &egui::Context,
    action: PowerAction,
) -> Option<bool> {
    let mut chosen: Option<bool> = None;

    egui::Window::new(action.confirm_dialog_title())
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.set_min_width(440.0);
            ui.add_space(4.0);
            ui.colored_label(
                egui::Color32::from_rgb(255, 180, 100),
                action.confirm_body(),
            );
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                let confirm_label = format!("确认 · {}", action.button_label());
                if ui
                    .add_sized([160.0, 30.0], egui::Button::new(confirm_label))
                    .clicked()
                {
                    chosen = Some(true);
                }
                ui.add_space(8.0);
                if ui
                    .add_sized([100.0, 30.0], egui::Button::new("取消"))
                    .clicked()
                {
                    chosen = Some(false);
                }
            });
            ui.add_space(4.0);
        });

    chosen
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_are_chinese_and_distinct() {
        let actions = [PowerAction::Reboot, PowerAction::Shutdown, PowerAction::ExitToCmd];
        let labels: Vec<&str> = actions.iter().map(|a| a.button_label()).collect();
        // 三个 label 互不相同（避免 UI 上撞）
        assert_eq!(
            labels.len(),
            labels.iter().collect::<std::collections::HashSet<_>>().len()
        );
        for l in labels {
            assert!(l.chars().any(|c| !c.is_ascii()), "label '{l}' 应含中文");
        }
    }

    #[test]
    fn confirm_body_mentions_relevant_info() {
        assert!(PowerAction::Reboot.confirm_body().contains("重启"));
        assert!(PowerAction::Shutdown.confirm_body().contains("关机"));
        assert!(PowerAction::ExitToCmd.confirm_body().contains("cmd"));
    }
}
