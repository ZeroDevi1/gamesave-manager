// main.rs - 应用入口
// 阻止 Windows 下发布版本出现额外的控制台窗口
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    gamesave_manager_lib::run();
}
