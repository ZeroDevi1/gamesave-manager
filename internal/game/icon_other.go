//go:build !windows

package game

// ExtractIconDataURI 只在 Windows 上实现；其它平台没有 exe 图标可提取，返回 nil。
func ExtractIconDataURI(string) (*string, error) { return nil, nil }
