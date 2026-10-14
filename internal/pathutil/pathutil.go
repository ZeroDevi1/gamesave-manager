// Package pathutil 处理存档路径模板：展开/折叠 Windows 环境变量占位符，解析 glob 通配符。
package pathutil

import (
	"os"
	"path/filepath"
	"sort"
	"strings"
)

// envDirs 返回占位符与当前机器物理目录的对应关系（与 Rust dirs crate 在 Windows 上的语义一致）。
// 取不到的目录不出现在结果中。
func envDirs() map[string]string {
	m := map[string]string{}
	if v := firstNonEmpty(os.Getenv("APPDATA"), configDir()); v != "" {
		m["%APPDATA%"] = v
	}
	if v := firstNonEmpty(os.Getenv("LOCALAPPDATA"), cacheDir()); v != "" {
		m["%LOCALAPPDATA%"] = v
	}
	if v, err := os.UserHomeDir(); err == nil && v != "" {
		m["%USERPROFILE%"] = v
	}
	if v := os.Getenv("PUBLIC"); v != "" {
		m["%PUBLIC%"] = v
	}
	return m
}

func configDir() string { d, _ := os.UserConfigDir(); return d }
func cacheDir() string  { d, _ := os.UserCacheDir(); return d }

func firstNonEmpty(vals ...string) string {
	for _, v := range vals {
		if v != "" {
			return v
		}
	}
	return ""
}

// ExpandEnv 展开路径中的 %APPDATA%、%LOCALAPPDATA%、%USERPROFILE%、%PUBLIC%、
// %PROGRAMFILES%、%PROGRAMFILES(X86)% 占位符。
//
// 展开后的路径若真实存在，返回清理过 `..` 的绝对路径（用于 `%APPDATA%/../LocalLow` 这类模板）；
// 不存在时原样返回展开结果，未知占位符（如 %STEAMPATH%）保留不动。
func ExpandEnv(path string) string {
	result := path
	for k, v := range envDirs() {
		result = strings.ReplaceAll(result, k, v)
	}
	if pf := os.Getenv("ProgramFiles"); pf != "" {
		result = strings.ReplaceAll(result, "%PROGRAMFILES%", pf)
	}
	if pf86 := os.Getenv("ProgramFiles(x86)"); pf86 != "" {
		result = strings.ReplaceAll(result, "%PROGRAMFILES(X86)%", pf86)
	}
	if _, err := os.Stat(result); err == nil {
		if abs, err := filepath.Abs(result); err == nil {
			return abs
		}
	}
	return result
}

// ShrinkEnv 把路径开头属于当前用户的目录折叠为环境变量占位符，并统一为正斜杠，
// 使导出的存档路径模板可在其它机器上展开。
//
// 按物理路径长度降序匹配（%LOCALAPPDATA% 优先于其父目录 %USERPROFILE%），
// 前缀比较不区分大小写，只替换前缀并保留后缀的原始大小写；至多折叠一次。
func ShrinkEnv(path string) string {
	normalized := strings.ReplaceAll(path, `\`, "/")

	type mapping struct{ placeholder, physical string }
	var mappings []mapping
	for k, v := range envDirs() {
		mappings = append(mappings, mapping{k, strings.ReplaceAll(v, `\`, "/")})
	}
	sort.Slice(mappings, func(i, j int) bool { return len(mappings[i].physical) > len(mappings[j].physical) })

	for _, m := range mappings {
		if len(normalized) >= len(m.physical) && strings.EqualFold(normalized[:len(m.physical)], m.physical) {
			return m.placeholder + normalized[len(m.physical):]
		}
	}
	return normalized
}

// ResolveSavePaths 展开存档路径模板，返回实际存在的文件或目录。
//
// 含 `*`、`?`、`[` 的路径按 glob 匹配；无匹配、路径不存在或模式非法时返回空切片。
func ResolveSavePaths(template string) []string {
	expanded := ExpandEnv(template)
	if strings.ContainsAny(expanded, "*?[") {
		matches, err := filepath.Glob(expanded)
		if err != nil {
			return nil
		}
		var out []string
		for _, p := range matches {
			if _, err := os.Stat(p); err == nil {
				out = append(out, p)
			}
		}
		return out
	}
	if _, err := os.Stat(expanded); err == nil {
		return []string{expanded}
	}
	return nil
}
