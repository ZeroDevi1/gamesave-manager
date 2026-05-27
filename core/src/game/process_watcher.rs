// game/process_watcher.rs - 游戏进程监控
//
// 使用 sysinfo 轮询系统进程列表，检测指定 exe 是否在运行。
// 当目标进程退出后，触发回调（自动增量备份）。
// 轮询间隔 3 秒，对系统性能影响极小。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};

/// 进程退出后的回调类型
type OnExitCallback = Box<dyn FnOnce() + Send + 'static>;

/// 游戏进程监控句柄
///
/// 通过 `stop()` 可以提前终止监控（例如用户手动取消）
pub struct ProcessWatcher {
    /// 停止信号标志
    stopped: Arc<AtomicBool>,
    /// 监控任务句柄
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl ProcessWatcher {
    /// 启动进程监控
    ///
    /// # 参数
    /// * `exe_name` - 要监控的 exe 文件名（如 "eldenring.exe"），不区分大小写
    /// * `on_exit` - 进程退出后执行的回调（在 tokio task 中异步执行）
    ///
    /// # 返回
    /// `ProcessWatcher` 句柄，通过 `.stop()` 可提前终止监控
    pub fn start(exe_name: String, on_exit: OnExitCallback) -> Self {
        let stopped = Arc::new(AtomicBool::new(false));
        let stopped_clone = stopped.clone();

        let handle = tokio::spawn(async move {
            let mut system = System::new();
            let exe_name_lower = exe_name.to_lowercase();
            let mut process_found_once = false;

            log::info!("[进程监控] 开始监控: {}", exe_name);

            loop {
                // 检查停止信号
                if stopped_clone.load(Ordering::Relaxed) {
                    log::info!("[进程监控] 收到停止信号，退出监控");
                    return;
                }

                // 刷新进程列表（仅刷新进程信息，不刷新磁盘/网络等）
                system.refresh_processes_specifics(
                    ProcessesToUpdate::All,
                    true,
                    ProcessRefreshKind::nothing(),
                );

                // 按 exe 名称匹配进程
                let is_running = system.processes().values().any(|proc| {
                    proc.name()
                        .to_string_lossy()
                        .to_lowercase()
                        == exe_name_lower
                });

                if is_running {
                    if !process_found_once {
                        log::info!("[进程监控] 检测到游戏进程运行中: {}", exe_name);
                        process_found_once = true;
                    }
                } else if process_found_once {
                    // 进程曾经运行过，现在消失了 → 游戏退出
                    log::info!("[进程监控] 游戏进程已退出: {}，触发回调", exe_name);
                    on_exit();
                    return;
                }

                // 轮询间隔 3 秒
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            }
        });

        Self {
            stopped,
            handle: Some(handle),
        }
    }

    /// 停止监控
    pub fn stop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

impl Drop for ProcessWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}
