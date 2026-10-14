package game

import (
	"log/slog"
	"strings"
	"time"
)

// watchInterval 是进程列表的轮询间隔；3 秒足以及时触发备份，对系统开销可忽略。
const watchInterval = 3 * time.Second

// WatchProcess 在后台轮询进程列表：exe（不区分大小写）出现过之后又消失时调用一次 onExit，随后结束。
// 进程始终未出现时会一直等待；无法枚举进程的平台上记录警告后放弃监控。
func WatchProcess(exeName string, onExit func()) {
	target := strings.ToLower(exeName)
	go func() {
		slog.Info("[进程监控] 开始监控", "exe", exeName)
		seen := false
		for {
			running, err := processRunning(target)
			if err != nil {
				slog.Warn("[进程监控] 无法枚举进程，停止监控", "exe", exeName, "err", err)
				return
			}
			switch {
			case running && !seen:
				slog.Info("[进程监控] 检测到游戏进程运行中", "exe", exeName)
				seen = true
			case !running && seen:
				slog.Info("[进程监控] 游戏进程已退出，触发回调", "exe", exeName)
				onExit()
				return
			}
			time.Sleep(watchInterval)
		}
	}()
}
