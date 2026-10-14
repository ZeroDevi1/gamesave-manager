// Package appdir 定位应用的本地数据目录（配置、备份清单、封面缓存）。
package appdir

import (
	"os"
	"path/filepath"
	"runtime"
)

// Identifier 是应用标识符，也是数据目录名。
// 与 Tauri 版的 identifier 保持一致，使改用 MyGo 后仍能读到原有的 config.json、manifests 与封面缓存。
const Identifier = "com.gamesave.manager"

// Dir 返回应用数据根目录并确保其存在。
//
// 路径与 Tauri 的 app_local_data_dir 相同：
// Windows 为 %LOCALAPPDATA%\<id>，macOS 为 ~/Library/Application Support/<id>，
// Linux 为 $XDG_DATA_HOME/<id>（缺省 ~/.local/share/<id>）。
func Dir() (string, error) {
	base, err := localDataBase()
	if err != nil {
		return "", err
	}
	dir := filepath.Join(base, Identifier)
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return "", err
	}
	return dir, nil
}

// Sub 返回数据根目录下的子目录（如 "manifests"、"cache/logos"）并确保其存在。
func Sub(elem ...string) (string, error) {
	root, err := Dir()
	if err != nil {
		return "", err
	}
	dir := filepath.Join(append([]string{root}, elem...)...)
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return "", err
	}
	return dir, nil
}

// LogosDir 返回游戏封面缓存目录 {data}/cache/logos。
func LogosDir() (string, error) { return Sub("cache", "logos") }

// ManifestsDir 返回本地备份清单目录 {data}/manifests。
func ManifestsDir() (string, error) { return Sub("manifests") }

func localDataBase() (string, error) {
	switch runtime.GOOS {
	case "windows":
		// os.UserCacheDir 在 Windows 上即 %LOCALAPPDATA%
		return os.UserCacheDir()
	case "darwin":
		return os.UserConfigDir()
	default:
		if xdg := os.Getenv("XDG_DATA_HOME"); xdg != "" {
			return xdg, nil
		}
		home, err := os.UserHomeDir()
		if err != nil {
			return "", err
		}
		return filepath.Join(home, ".local", "share"), nil
	}
}
