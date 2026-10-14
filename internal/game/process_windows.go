package game

import (
	"strings"
	"syscall"
	"unsafe"
)

// processRunning 用 Toolhelp 快照枚举进程，判断是否存在可执行文件名为 lowerName 的进程。
func processRunning(lowerName string) (bool, error) {
	snap, err := syscall.CreateToolhelp32Snapshot(syscall.TH32CS_SNAPPROCESS, 0)
	if err != nil {
		return false, err
	}
	defer syscall.CloseHandle(snap)

	var pe syscall.ProcessEntry32
	pe.Size = uint32(unsafe.Sizeof(pe))
	if err := syscall.Process32First(snap, &pe); err != nil {
		return false, err
	}
	for {
		if strings.ToLower(syscall.UTF16ToString(pe.ExeFile[:])) == lowerName {
			return true, nil
		}
		if err := syscall.Process32Next(snap, &pe); err != nil {
			// ERROR_NO_MORE_FILES 表示枚举结束
			return false, nil
		}
	}
}
