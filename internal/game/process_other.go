//go:build !windows

package game

import (
	"errors"
	"os"
	"path/filepath"
	"runtime"
	"strings"
)

// processRunning 在 Linux 上读取 /proc/<pid>/comm 判断进程是否存在；其它平台不支持。
// comm 最长 15 字节，因此也接受目标名的前 15 字节匹配。
func processRunning(lowerName string) (bool, error) {
	if runtime.GOOS != "linux" {
		return false, errors.New("当前平台不支持进程监控")
	}
	matches, err := filepath.Glob("/proc/[0-9]*/comm")
	if err != nil {
		return false, err
	}
	short := lowerName
	if len(short) > 15 {
		short = short[:15]
	}
	for _, m := range matches {
		data, err := os.ReadFile(m)
		if err != nil {
			continue
		}
		name := strings.ToLower(strings.TrimSpace(string(data)))
		if name == lowerName || name == short {
			return true, nil
		}
	}
	return false, nil
}
